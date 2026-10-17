//! Owns the layer instances (one visual per layer slot), renders the enabled
//! ones with their own parameters and audio band, runs each layer's modifier
//! chain, levels it, and adds it into the composite. Scene switches dissolve
//! the previous picture into the new one (a frozen snapshot, cheap). In auto
//! mode it raises cues (track change, drops, long quiet, dwell) and the app
//! picks the next scene.

use std::time::Instant;

use audioviz_analysis::FeatureFrame;
use audioviz_metadata::ArtImage;
use bytemuck::{Pod, Zeroable};

use crate::bandfilter::{BandPass, band_features};
use crate::color;
use crate::scene::{self, FrameCtx, HDR_FORMAT, Scene, fullscreen_pass, fullscreen_pass2, hdr_texture};
use crate::scenes::kaleido::Kaleido;
use crate::scenes::ribbons::Ribbons;
use crate::scenes::scope::Scope;
use crate::scenes::sigil::Sigil;
use crate::scenes::starfield::Starfield;
use crate::settings::{FoldMode, FractalMode, MAX_LAYERS, MAX_MODS, MaskShape, Modifier, Settings, TintMode, VisualKind};
use crate::shaders::ShaderSet;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct LayerParams {
    opacity: f32,
    dt: f32,
    /// bit0: apply the layer's levelling gain.
    flags: u32,
    _pad: f32,
}

/// Per-layer meter state: a 1/8 downsample and a ping-ponged 1x1 gain texture.
struct Meter {
    down: wgpu::TextureView,
    state: [wgpu::TextureView; 2],
    cur: usize,
}

impl Meter {
    fn new(device: &wgpu::Device, i: usize, width: u32, height: u32) -> Self {
        Self { down: hdr_texture(device, &format!("layer {i} down"), width / 8, height / 8), state: [hdr_texture(device, &format!("layer {i} meter a"), 1, 1), hdr_texture(device, &format!("layer {i} meter b"), 1, 1)], cur: 0 }
    }
}

/// Generic modifier uniform; each modifier shader documents its p0..p3.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct ModParams {
    resolution: [f32; 2],
    time: f32,
    dt: f32,
    p0: [f32; 4],
    p1: [f32; 4],
    p2: [f32; 4],
    p3: [f32; 4],
}

/// The palette grade modifier's uniform: the generic params plus the table.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GradeParams {
    m: ModParams,
    pal: color::Table,
}

/// Per-layer palette state: the baked table, what it was baked from, and the shift.
struct PaletteState {
    key: u64,
    table: color::Table,
    shift: f32,
    last_beat: f32,
}

/// Per-slot modifier state that lives across frames (angles, beat counters, rings).
#[derive(Clone, Copy)]
struct ModState {
    angle: f32,
    seg_cur: f32,
    seg_target: f32,
    beats: u32,
    last_beat: f32,
    last_onset: f32,
    wander: f32,
    rings: [f32; 4],
    ring_i: usize,
}

impl Default for ModState {
    fn default() -> Self {
        Self { angle: 0.0, seg_cur: 0.0, seg_target: 0.0, beats: 0, last_beat: 0.0, last_onset: 0.0, wander: 0.0, rings: [-100.0; 4], ring_i: 0 }
    }
}

/// Segment counts the fold steps through on beats.
const SEG_SET: [f32; 6] = [6.0, 8.0, 5.0, 10.0, 4.0, 12.0];

/// Per-layer modifier chain resources: two ping-pong textures, a history
/// pair for feedback, a uniform buffer and state per slot.
struct Chain {
    ping: [wgpu::TextureView; 2],
    hist: [wgpu::TextureView; 2],
    hist_cur: usize,
    bufs: Vec<wgpu::Buffer>,
    states: Vec<ModState>,
}

impl Chain {
    fn new(device: &wgpu::Device, i: usize, width: u32, height: u32) -> Self {
        Self {
            ping: [hdr_texture(device, &format!("layer {i} mod a"), width, height), hdr_texture(device, &format!("layer {i} mod b"), width, height)],
            hist: [hdr_texture(device, &format!("layer {i} hist a"), width, height), hdr_texture(device, &format!("layer {i} hist b"), width, height)],
            hist_cur: 0,
            bufs: (0..MAX_MODS).map(|j| scene::uniform_buffer(device, &format!("layer {i} mod {j}"), std::mem::size_of::<GradeParams>() as u64)).collect(),
            states: vec![ModState::default(); MAX_MODS],
        }
    }

    fn resize(&mut self, device: &wgpu::Device, i: usize, width: u32, height: u32) {
        self.ping = [hdr_texture(device, &format!("layer {i} mod a"), width, height), hdr_texture(device, &format!("layer {i} mod b"), width, height)];
        self.hist = [hdr_texture(device, &format!("layer {i} hist a"), width, height), hdr_texture(device, &format!("layer {i} hist b"), width, height)];
    }
}

/// One pipeline per modifier kind, in `MOD_SHADERS` order.
const MOD_SHADERS: [&str; 10] = ["fold.wgsl", "feedback.wgsl", "polar.wgsl", "ripple.wgsl", "edge.wgsl", "chroma.wgsl", "mask.wgsl", "fractal.wgsl", "lumi.wgsl", "grade.wgsl"];
/// Pipeline indices with special plumbing.
const MOD_FEEDBACK: usize = 1;
const MOD_LUMI: usize = 8;

fn mod_index(m: &Modifier) -> usize {
    match m {
        Modifier::Fold(_) => 0,
        Modifier::Feedback(_) => 1,
        Modifier::Polar(_) => 2,
        Modifier::Ripple(_) => 3,
        Modifier::Edge(_) => 4,
        Modifier::Chroma(_) => 5,
        Modifier::Mask(_) => 6,
        Modifier::Fractal(_) => 7,
        Modifier::Lumi(_) => 8,
        Modifier::Grade(_) => 9,
    }
}

/// Rotate an RGB colour's hue by `deg` degrees (YIQ rotation; cheap and fine for tints).
fn hue_rotate(c: [f32; 3], deg: f32) -> [f32; 3] {
    let (r, g, b) = (c[0], c[1], c[2]);
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    let i = 0.596 * r - 0.274 * g - 0.322 * b;
    let q = 0.211 * r - 0.523 * g + 0.312 * b;
    let (s, co) = deg.to_radians().sin_cos();
    let (i2, q2) = (i * co - q * s, i * s + q * co);
    [(y + 0.956 * i2 + 0.621 * q2).max(0.0), (y - 0.272 * i2 - 0.647 * q2).max(0.0), (y - 1.106 * i2 + 1.703 * q2).max(0.0)]
}

struct LayerPipelines {
    add: wgpu::RenderPipeline,
    copy: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    meter: wgpu::RenderPipeline,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct MixParams {
    t: f32,
    _pad: [f32; 3],
}

/// Manual scene switches dissolve fast; the auto director takes its time.
pub const FADE_MANUAL_S: f32 = 0.5;
pub const FADE_AUTO_S: f32 = 4.0;
const MIN_DWELL_S: f32 = 90.0;
const MAX_DWELL_S: f32 = 360.0;
const QUIET_FOR_S: f32 = 6.0;

pub struct Director {
    pub layers: Vec<Box<dyn Scene>>,
    filters: Vec<BandPass>,
    meters: Vec<Meter>,
    chains: Vec<Chain>,
    palettes: Vec<PaletteState>,
    mod_layout: wgpu::BindGroupLayout,
    mod_pipelines: Vec<wgpu::RenderPipeline>,
    mirror_sampler: wgpu::Sampler,
    /// Smoothed effective tint per layer.
    tints: Vec<[f32; 3]>,
    layer_bufs: Vec<wgpu::Buffer>,
    sampler: wgpu::Sampler,
    layer_layout: wgpu::BindGroupLayout,
    layer_pipelines: LayerPipelines,
    mix_layout: wgpu::BindGroupLayout,
    mix_pipeline: wgpu::RenderPipeline,
    mix_buf: wgpu::Buffer,
    one_buf: wgpu::Buffer,
    composite: wgpu::TextureView,
    prev: wgpu::TextureView,
    width: u32,
    height: u32,
    /// (progress 0..1, seconds)
    dissolve: Option<(f32, f32)>,
    pending_dissolve: Option<f32>,
    last_switch: Instant,
    // Auto-mode state.
    last_track: Option<u64>,
    quiet_for: f32,
    trend_peak: f32,
    pub last_cue: &'static str,
    pub last_error: Option<String>,
}

fn make(kind: VisualKind, device: &wgpu::Device, w: u32, h: u32, shaders: &ShaderSet) -> Result<Box<dyn Scene>, String> {
    Ok(match kind {
        VisualKind::Scope => Box::new(Scope::new(device, w, h, shaders)?),
        VisualKind::Ribbons => Box::new(Ribbons::new(device, w, h, shaders)?),
        VisualKind::Starfield => Box::new(Starfield::new(device, w, h, shaders)?),
        VisualKind::Kaleido => Box::new(Kaleido::new(device, w, h, shaders)?),
        VisualKind::Sigil => Box::new(Sigil::new(device, w, h, shaders)?),
    })
}

fn energy_of(f: &FeatureFrame) -> f32 {
    ((f.bass_att + f.mid_att + f.treb_att) / 3.0).clamp(0.0, 2.5)
}

/// Advance a modifier's state for this frame and produce its uniform.
/// Returns the params and whether the pass writes the layer's history (feedback).
fn mod_params(m: &Modifier, st: &mut ModState, f: &FeatureFrame, tint: [f32; 3], dt: f32, time: f32, width: u32, height: u32) -> ModParams {
    let energy = energy_of(f);
    let onset = f.onset_pulse[3].min(1.0);
    let bass = (f.bass_att - 1.0).clamp(-1.0, 1.5);
    let beat = f.beat_pulse * f.beat_confidence;
    let beat_hit = beat > st.last_beat + 0.4;
    st.last_beat = beat;
    let onset_hit = onset > st.last_onset + 0.25;
    st.last_onset = onset;
    let mut p = ModParams { resolution: [width as f32, height as f32], time, dt, ..Default::default() };
    match m {
        Modifier::Fold(k) => {
            st.angle += dt * k.speed * (0.4 + 0.6 * energy * k.react) + dt * 1.5 * k.react * onset;
            if st.seg_cur <= 0.0 {
                st.seg_cur = k.segments as f32;
                st.seg_target = st.seg_cur;
            }
            if k.step_beats > 0 {
                if beat_hit {
                    st.beats += 1;
                    if st.beats % k.step_beats == 0 {
                        st.seg_target = SEG_SET[(st.beats / k.step_beats) as usize % SEG_SET.len()];
                    }
                }
            } else {
                st.seg_target = k.segments.clamp(1, 32) as f32;
            }
            st.seg_cur += (st.seg_target - st.seg_cur) * (1.0 - (-dt / 0.15).exp());
            st.wander += dt * 0.12 * k.wander * (0.5 + 0.5 * energy);
            let off = [k.offset * (st.wander * 0.7).sin(), k.offset * (st.wander * 0.9 + 1.0).cos()];
            let zoom = (k.zoom * (1.0 + 0.06 * k.react * bass)).max(0.05);
            let mode = match k.mode {
                FoldMode::Radial => 0.0,
                FoldMode::Tiled => 1.0,
                FoldMode::Pinwheel => 2.0,
            };
            p.p0 = [st.seg_cur, st.angle, zoom, mode];
            p.p1 = [0.45, off[0], off[1], 0.0];
        }
        Modifier::Feedback(k) => {
            let zoom = 1.0 + k.zoom * dt * (0.6 + 0.4 * energy * k.react);
            let spin = k.spin * dt * (1.0 + 3.0 * k.react * onset);
            let decay = (k.decay * (1.0 - 0.15 * k.react * (1.0 - energy).clamp(0.0, 1.0))).clamp(0.0, 0.985);
            p.p0 = [decay, zoom, spin, 0.0];
            p.p1 = [0.0, 0.0, 0.0, 0.0];
        }
        Modifier::Polar(k) => {
            st.angle += dt * k.spin * (0.5 + 0.5 * energy * k.react);
            let radius = (k.radius * (1.0 + 0.08 * k.react * bass)).max(0.05);
            p.p0 = [k.unwrap as u32 as f32, radius, st.angle, 0.0];
        }
        Modifier::Ripple(k) => {
            if beat_hit || (k.react > 1.5 && onset_hit) {
                st.rings[st.ring_i] = time;
                st.ring_i = (st.ring_i + 1) % 4;
            }
            let amount = k.amount * (0.6 + 0.4 * energy * k.react);
            p.p0 = [amount, k.wavelength, k.speed, 0.08];
            p.p1 = [time - st.rings[0], time - st.rings[1], time - st.rings[2], time - st.rings[3]];
            p.p2 = [k.sine * 0.02, 12.0, time * 1.5, 0.0];
        }
        Modifier::Edge(k) => {
            let strength = k.strength * (0.7 + 0.3 * energy * k.react);
            p.p0 = [strength, k.keep, k.tinted as u32 as f32, 0.0];
            p.p1 = [tint[0], tint[1], tint[2], 0.0];
        }
        Modifier::Chroma(k) => {
            let amount = k.amount * (1.0 + k.react * (2.0 * onset + 0.5 * (f.treb_att - 1.0).clamp(-1.0, 2.0)));
            p.p0 = [amount, k.radial as u32 as f32, 0.0, 0.0];
        }
        Modifier::Fractal(k) => {
            st.angle += dt * k.angle * 0.3 * (0.5 + 0.5 * energy * k.react) + dt * 0.8 * k.react * onset;
            let scale = (k.scale * (1.0 + 0.05 * k.react * bass)).max(1.05);
            match k.mode {
                FractalMode::Kifs => {
                    p.p0 = [0.0, k.iterations.clamp(1, 10) as f32, scale, st.angle];
                    p.p1 = [k.offset, k.zoom.max(0.05), 0.0, 0.0];
                }
                FractalMode::Julia => {
                    // c orbits slowly inside the interesting region; onsets nudge it.
                    let r = 0.25 + 0.5 * k.offset.clamp(0.0, 1.0);
                    let c = [-0.4 + r * st.angle.cos() * 0.6, r * (st.angle * 1.3).sin() * 0.6];
                    p.p0 = [1.0, k.iterations.clamp(1, 10) as f32, scale, 0.0];
                    p.p1 = [0.0, k.zoom.max(0.05), c[0], c[1]];
                }
            }
            p.p2 = [k.falloff.clamp(0.1, 1.0), 0.0, 0.0, 0.0];
        }
        Modifier::Lumi(k) => {
            let fresh = tint;
            let faded = hue_rotate(tint, k.hue_shift);
            p.p0 = [k.fresh_s.max(0.02), k.afterglow_s.max(0.1), k.fresh_gain, k.afterglow_gain];
            p.p1 = [fresh[0], fresh[1], fresh[2], k.excite * k.react];
            p.p2 = [faded[0], faded[1], faded[2], k.sparkle * k.react];
            // Charge rate keeps the slow pool's steady state near the picture's own level.
            p.p3 = [k.uv, onset, 1.0 / k.afterglow_s.max(0.1), 0.0];
        }
        Modifier::Grade(k) => {
            // p0.z (shift) is filled in by the caller from the layer's palette state.
            let amount = (k.amount * (1.0 + 0.15 * k.react * onset)).clamp(0.0, 1.0);
            p.p0 = [k.by_hue as u32 as f32, amount, 0.0, k.react];
        }
        Modifier::Mask(k) => {
            let radius = (k.radius * (1.0 + 0.1 * k.react * bass)).max(0.01);
            let shape = match k.shape {
                MaskShape::Circle => 0.0,
                MaskShape::Band => 1.0,
                MaskShape::Hex => 2.0,
            };
            p.p0 = [shape, radius, k.soft, k.invert as u32 as f32];
        }
    }
    p
}

impl Director {
    pub fn new(device: &wgpu::Device, shaders: &ShaderSet, width: u32, height: u32, settings: &Settings) -> Result<Self, String> {
        let sampler = scene::linear_sampler(device);
        let layer_layout = scene::tex_layout(device, "layer", 1);
        let layer_pipelines = build_layer(device, shaders, &layer_layout)?;
        let mix_layout = scene::tex_layout(device, "mix", 1);
        let mix_pipeline = build_mix(device, shaders, &mix_layout)?;
        let mix_buf = scene::uniform_buffer(device, "mix params", std::mem::size_of::<MixParams>() as u64);
        let one_buf = scene::uniform_buffer(device, "layer one", std::mem::size_of::<LayerParams>() as u64);
        let mut layers = Vec::new();
        for i in 0..MAX_LAYERS {
            layers.push(make(settings.layer(i).visual, device, width, height, shaders)?);
        }
        let filters = (0..MAX_LAYERS).map(|_| BandPass::new(20.0, 400.0, 48_000.0)).collect();
        let meters = (0..MAX_LAYERS).map(|i| Meter::new(device, i, width, height)).collect();
        let tints = (0..MAX_LAYERS).map(|i| settings.layer(i).tint.color).collect();
        let chains = (0..MAX_LAYERS).map(|i| Chain::new(device, i, width, height)).collect();
        let palettes = (0..MAX_LAYERS).map(|_| PaletteState { key: 0, table: color::bake(color::preset("phosphor").unwrap().stops, true), shift: 0.0, last_beat: 0.0 }).collect();
        let mod_layout = scene::tex_layout(device, "mod", 1);
        let mod_pipelines = build_mods(device, shaders, &mod_layout)?;
        let mirror_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mod mirror"),
            address_mode_u: wgpu::AddressMode::MirrorRepeat,
            address_mode_v: wgpu::AddressMode::MirrorRepeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layer_bufs = (0..MAX_LAYERS).map(|i| scene::uniform_buffer(device, &format!("layer {i}"), std::mem::size_of::<LayerParams>() as u64)).collect();
        Ok(Self {
            layers,
            filters,
            meters,
            chains,
            palettes,
            mod_layout,
            mod_pipelines,
            mirror_sampler,
            tints,
            layer_bufs,
            sampler,
            layer_layout,
            layer_pipelines,
            mix_layout,
            mix_pipeline,
            mix_buf,
            one_buf,
            composite: hdr_texture(device, "composite", width, height),
            prev: hdr_texture(device, "composite prev", width, height),
            width,
            height,
            dissolve: None,
            pending_dissolve: None,
            last_switch: Instant::now(),
            last_track: None,
            quiet_for: 0.0,
            trend_peak: 0.0,
            last_cue: "manual",
            last_error: None,
        })
    }

    /// Rebuild any layer whose visual kind changed in the settings.
    pub fn sync(&mut self, device: &wgpu::Device, shaders: &ShaderSet, settings: &Settings) {
        for i in 0..MAX_LAYERS {
            let want = settings.layer(i).visual;
            if self.layers[i].kind() != want {
                match make(want, device, self.width, self.height, shaders) {
                    Ok(s) => self.layers[i] = s,
                    Err(e) => self.last_error = Some(format!("{}: {e}", want.name())),
                }
            }
        }
    }

    /// Move layer `from` to position `to`, carrying its live state (particles,
    /// persistence, meter, modifier chain) with it. The caller reorders
    /// `settings.layers` the same way.
    pub fn move_layer(&mut self, from: usize, to: usize) {
        let n = self.layers.len().min(MAX_LAYERS);
        if from >= n || to >= n || from == to {
            return;
        }
        let l = self.layers.remove(from);
        self.layers.insert(to, l);
        let f = self.filters.remove(from);
        self.filters.insert(to, f);
        let m = self.meters.remove(from);
        self.meters.insert(to, m);
        let t = self.tints.remove(from);
        self.tints.insert(to, t);
        let c = self.chains.remove(from);
        self.chains.insert(to, c);
        let p = self.palettes.remove(from);
        self.palettes.insert(to, p);
    }

    /// Move a modifier within layer `layer`'s chain so its live state follows it.
    pub fn move_modifier(&mut self, layer: usize, from: usize, to: usize) {
        if let Some(c) = self.chains.get_mut(layer) {
            if from < c.states.len() && to < c.states.len() && from != to {
                let s = c.states.remove(from);
                c.states.insert(to, s);
            }
        }
    }

    /// Drop a modifier's live state when it is removed from the chain.
    pub fn remove_modifier(&mut self, layer: usize, at: usize) {
        if let Some(c) = self.chains.get_mut(layer) {
            if at < c.states.len() {
                c.states.remove(at);
                c.states.push(ModState::default());
            }
        }
    }

    pub fn seconds_since_switch(&self) -> f32 {
        self.last_switch.elapsed().as_secs_f32()
    }

    /// Dissolve the current picture into whatever renders next.
    pub fn begin_dissolve(&mut self, seconds: f32) {
        self.pending_dissolve = Some(seconds.max(0.05));
        self.last_switch = Instant::now();
    }

    pub fn is_fading(&self) -> bool {
        self.dissolve.is_some() || self.pending_dissolve.is_some()
    }

    /// Auto-mode cue evaluation. Call once per frame; returns a cue when the
    /// director wants a new scene.
    pub fn consider(&mut self, f: &FeatureFrame, track_id: Option<u64>, dt: f32, auto: bool) -> Option<&'static str> {
        let track_changed = matches!((self.last_track, track_id), (Some(a), Some(b)) if a != b);
        if track_id.is_some() {
            self.last_track = track_id;
        }
        self.quiet_for = if f.quiet || f.silent { self.quiet_for + dt } else { 0.0 };
        self.trend_peak = if f.energy_trend > self.trend_peak { f.energy_trend } else { self.trend_peak * (1.0 - dt * 0.2) };
        let drop = self.trend_peak > 0.45 && f.energy_trend < -0.35;
        if !auto || self.is_fading() {
            return None;
        }
        let since = self.seconds_since_switch();
        let cue = if track_changed && since > 30.0 {
            Some("track change")
        } else if drop && since > MIN_DWELL_S {
            Some("drop")
        } else if self.quiet_for > QUIET_FOR_S && since > MIN_DWELL_S {
            Some("quiet")
        } else if since > MAX_DWELL_S {
            Some("dwell limit")
        } else {
            None
        };
        if let Some(c) = cue {
            self.last_cue = c;
            self.trend_peak = 0.0;
            self.quiet_for = 0.0;
        }
        cue
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        for s in &mut self.layers {
            s.resize(device, width, height);
        }
        self.composite = hdr_texture(device, "composite", width, height);
        self.prev = hdr_texture(device, "composite prev", width, height);
        for (i, m) in self.meters.iter_mut().enumerate() {
            m.down = hdr_texture(device, &format!("layer {i} down"), width / 8, height / 8);
        }
        for (i, c) in self.chains.iter_mut().enumerate() {
            c.resize(device, i, width, height);
        }
        self.dissolve = None;
    }

    pub fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) {
        let mut errors = Vec::new();
        match build_layer(device, shaders, &self.layer_layout) {
            Ok(p) => self.layer_pipelines = p,
            Err(e) => errors.push(format!("layer: {e}")),
        }
        match build_mods(device, shaders, &self.mod_layout) {
            Ok(p) => self.mod_pipelines = p,
            Err(e) => errors.push(format!("modifiers: {e}")),
        }
        if let Err(e) = build_mix(device, shaders, &self.mix_layout).map(|p| self.mix_pipeline = p) {
            errors.push(format!("mix: {e}"));
        }
        for s in &mut self.layers {
            if let Err(e) = s.reload_shaders(device, shaders) {
                errors.push(format!("{}: {e}", s.name()));
            }
        }
        self.last_error = (!errors.is_empty()).then(|| errors.join("\n"));
    }

    /// Render every enabled layer and composite into `target`.
    #[allow(clippy::too_many_arguments)]
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, settings: &Settings, features: &FeatureFrame, points: &[[f32; 2]], sample_rate: u32, tint: [f32; 3], art: Option<[f32; 3]>, art_image: Option<&ArtImage>, art_id: u64, dt: f32, time: f32, auto_gain: f32, target: &wgpu::TextureView) {
        queue.write_buffer(&self.one_buf, 0, bytemuck::bytes_of(&LayerParams { opacity: 1.0, dt, flags: 0, _pad: 0.0 }));
        // Levelling: with auto exposure on, each layer is brought to its own
        // dark-field mean before mixing, so opacity is the mix ratio and a dim
        // layer is not lost under a bright one.
        let level = settings.post.auto_exposure;
        // Snapshot last frame's composite before anything renders over it.
        if let Some(secs) = self.pending_dissolve.take() {
            let group = scene::tex_group(device, &self.layer_layout, &[&self.composite, &self.meters[0].state[0]], &self.sampler, &self.one_buf);
            fullscreen_pass(encoder, "dissolve snapshot", &self.prev, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.layer_pipelines.copy, &group);
            self.dissolve = Some((0.0, secs));
        }

        let mut first = true;
        for i in 0..MAX_LAYERS.min(self.layers.len()) {
            let layer = settings.layer(i);
            if !layer.enabled || layer.opacity <= 0.0 {
                continue;
            }
            let band = layer.band;
            let filtered_features;
            let feats: &FeatureFrame = if band.enabled {
                filtered_features = band_features(features, band.lo_hz, band.hi_hz);
                &filtered_features
            } else {
                features
            };
            let filtered_points;
            let pts: &[[f32; 2]] = if band.enabled && layer.visual == VisualKind::Scope {
                self.filters[i].retune(band.lo_hz, band.hi_hz, sample_rate as f32);
                filtered_points = self.filters[i].process(points);
                &filtered_points
            } else {
                points
            };
            // Per-layer tint: the scene's, the art's, or its own, slewed like the scene tint.
            let want = match layer.tint.mode {
                TintMode::Scene => tint,
                TintMode::Art => art.unwrap_or(tint),
                TintMode::Custom => layer.tint.color,
            };
            let k = 1.0 - (-dt / 0.8).exp();
            for c in 0..3 {
                self.tints[i][c] += (want[c] - self.tints[i][c]) * k;
            }
            // Palette: rebake when the selection changed; shift it with the music.
            {
                let ps = &mut self.palettes[i];
                let key = color::key(&layer.palette.name, &layer.palette.custom, layer.palette.cyclic);
                if key != ps.key {
                    ps.key = key;
                    ps.table = color::bake(&layer.palette.stops(), layer.palette.cyclic);
                }
                let energy = energy_of(feats);
                let beat = feats.beat_pulse * feats.beat_confidence;
                if beat > ps.last_beat + 0.4 && layer.palette.cycle != 0.0 {
                    ps.shift += 0.04 * layer.palette.react * layer.palette.cycle.signum();
                }
                ps.last_beat = beat;
                ps.shift = (ps.shift + dt * layer.palette.cycle * (0.5 + 0.5 * energy * layer.palette.react)).rem_euclid(1.0);
            }
            let pal_active = layer.palette.active();
            let (pal_table, pal_shift, pal_amount) = (&self.palettes[i].table, self.palettes[i].shift, if pal_active { layer.palette.amount } else { 0.0 });
            let ctx = FrameCtx { features: feats, tint: self.tints[i], dt, time, width: self.width, height: self.height, points: pts, auto_gain, art: art_image, art_id, photos_folder: &settings.photos_folder, palette: pal_active.then_some(pal_table), palette_shift: pal_shift, palette_amount: pal_amount };
            self.layers[i].render(queue, encoder, &ctx, &layer);
            queue.write_buffer(&self.layer_bufs[i], 0, bytemuck::bytes_of(&LayerParams { opacity: layer.opacity, dt, flags: level as u32, _pad: 0.0 }));

            // Modifier chain: advance every slot's state and write its uniform
            // first (mutable), then run the passes (immutable borrows only).
            let mut passes: Vec<(usize, usize)> = Vec::new(); // (slot, pipeline index)
            {
                let chain = &mut self.chains[i];
                for (j, m) in layer.modifiers.iter().enumerate().take(MAX_MODS) {
                    if !m.enabled() {
                        continue;
                    }
                    let mut p = mod_params(m, &mut chain.states[j], feats, self.tints[i], dt, time, self.width, self.height);
                    if matches!(m, Modifier::Grade(_)) {
                        p.p0[2] = pal_shift;
                        queue.write_buffer(&chain.bufs[j], 0, bytemuck::bytes_of(&GradeParams { m: p, pal: *pal_table }));
                    } else {
                        queue.write_buffer(&chain.bufs[j], 0, bytemuck::bytes_of(&p));
                    }
                    passes.push((j, mod_index(m)));
                }
            }
            let chain = &self.chains[i];
            let mut src: &wgpu::TextureView = self.layers[i].output();
            let mut pp = 0usize;
            let mut hist_flip = false;
            for (j, pi) in passes {
                let (hist_prev, hist_cur) = (chain.hist_cur, 1 - chain.hist_cur);
                let g = scene::tex_group(device, &self.mod_layout, &[src, &chain.hist[hist_prev]], &self.mirror_sampler, &chain.bufs[j]);
                if pi == MOD_FEEDBACK {
                    // Feedback's output is its own history.
                    fullscreen_pass(encoder, "modifier", &chain.hist[hist_cur], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.mod_pipelines[pi], &g);
                    src = &chain.hist[hist_cur];
                    hist_flip = true;
                } else if pi == MOD_LUMI {
                    // Luminescence writes its picture and, beside it, its pool state.
                    fullscreen_pass2(encoder, "luminescence", &chain.ping[pp], &chain.hist[hist_cur], &self.mod_pipelines[pi], &g);
                    src = &chain.ping[pp];
                    pp = 1 - pp;
                    hist_flip = true;
                } else {
                    fullscreen_pass(encoder, "modifier", &chain.ping[pp], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.mod_pipelines[pi], &g);
                    src = &chain.ping[pp];
                    pp = 1 - pp;
                }
            }
            let out = src;
            let m = &self.meters[i];
            let (prev_s, cur_s) = (m.cur, 1 - m.cur);
            if level {
                let g = scene::tex_group(device, &self.layer_layout, &[out, &m.state[prev_s]], &self.sampler, &self.layer_bufs[i]);
                fullscreen_pass(encoder, "layer down", &m.down, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.layer_pipelines.down, &g);
                let g = scene::tex_group(device, &self.layer_layout, &[&m.down, &m.state[prev_s]], &self.sampler, &self.layer_bufs[i]);
                fullscreen_pass(encoder, "layer meter", &m.state[cur_s], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.layer_pipelines.meter, &g);
            }
            let group = scene::tex_group(device, &self.layer_layout, &[out, &m.state[cur_s]], &self.sampler, &self.layer_bufs[i]);
            let load = if first { wgpu::LoadOp::Clear(wgpu::Color::BLACK) } else { wgpu::LoadOp::Load };
            fullscreen_pass(encoder, "layer", &self.composite, load, &self.layer_pipelines.add, &group);
            if level {
                self.meters[i].cur = cur_s;
            }
            if hist_flip {
                self.chains[i].hist_cur = 1 - self.chains[i].hist_cur;
            }
            first = false;
        }
        if first {
            scene::clear_pass(encoder, "layers (none)", &self.composite);
        }

        // Composite → target, through the dissolve when one is running.
        match &mut self.dissolve {
            Some((t, secs)) => {
                *t += dt / *secs;
                let k = smoothstep(*t);
                queue.write_buffer(&self.mix_buf, 0, bytemuck::bytes_of(&MixParams { t: k, _pad: [0.0; 3] }));
                let group = scene::tex_group(device, &self.mix_layout, &[&self.prev, &self.composite], &self.sampler, &self.mix_buf);
                fullscreen_pass(encoder, "dissolve", target, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.mix_pipeline, &group);
                if *t >= 1.0 {
                    self.dissolve = None;
                }
            }
            None => {
                let group = scene::tex_group(device, &self.layer_layout, &[&self.composite, &self.meters[0].state[0]], &self.sampler, &self.one_buf);
                fullscreen_pass(encoder, "composite", target, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.layer_pipelines.copy, &group);
            }
        }
    }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn build_mods(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<Vec<wgpu::RenderPipeline>, String> {
    scene::guarded(device, || {
        MOD_SHADERS
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let m = scene::shader(device, shaders, name);
                if i == MOD_LUMI {
                    scene::pipeline2(device, name, layout, &m, HDR_FORMAT)
                } else {
                    scene::pipeline(device, name, layout, &m, HDR_FORMAT, None)
                }
            })
            .collect()
    })
}

fn build_layer(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<LayerPipelines, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "layer.wgsl");
        let d = scene::shader(device, shaders, "layer_down.wgsl");
        let g = scene::shader(device, shaders, "layer_meter.wgsl");
        LayerPipelines {
            add: scene::pipeline(device, "layer add", layout, &m, HDR_FORMAT, Some(scene::ADDITIVE)),
            copy: scene::pipeline(device, "layer copy", layout, &m, HDR_FORMAT, None),
            down: scene::pipeline(device, "layer down", layout, &d, HDR_FORMAT, None),
            meter: scene::pipeline(device, "layer meter", layout, &g, HDR_FORMAT, None),
        }
    })
}

fn build_mix(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "mix.wgsl");
        scene::pipeline(device, "mix", layout, &m, HDR_FORMAT, None)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_params_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<ModParams>(), 80);
        assert_eq!(std::mem::offset_of!(ModParams, p0), 16);
        assert_eq!(std::mem::offset_of!(ModParams, p3), 64);
    }

    #[test]
    fn every_modifier_kind_has_a_shader() {
        for m in Modifier::KINDS.iter().map(|(_, make)| make()) {
            assert!(mod_index(&m) < MOD_SHADERS.len());
        }
    }
}
