//! Owns the layer instances (one visual per layer slot), renders the visible
//! ones with their own parameters and audio band, runs each layer's modifier
//! chain, levels it, and blends it into the composite. Every slot's opacity is
//! an eased ramp toward its target, so a scene switch fades layer by layer: a
//! slot that keeps its visual retunes in place, a slot whose visual changes
//! keeps the old one rendering underneath while it fades out. In auto mode it
//! raises cues (track change, drops, long quiet, dwell) and the app picks the
//! next scene.

use std::time::Instant;

use audioviz_analysis::FeatureFrame;
use audioviz_metadata::ArtImage;
use bytemuck::{Pod, Zeroable};

use crate::bandfilter::{BandPass, band_features};
use crate::color;
use crate::scene::{self, FrameCtx, HDR_FORMAT, Pyramid, Scene, fullscreen_pass, fullscreen_pass2, hdr_pyramid, hdr_texture};
use crate::scenes::kaleido::Kaleido;
use crate::scenes::ribbons::Ribbons;
use crate::scenes::scope::Scope;
use crate::scenes::sigil::Sigil;
use crate::scenes::starfield::Starfield;
use crate::scenes::waves::Waves;
use crate::settings::{EdgeColour, FoldMode, FractalMode, HalftoneShape, Layer, MAX_LAYERS, MAX_MODS, MaskShape, Modifier, Settings, TintMode, VisualKind};
use crate::shaders::ShaderSet;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct LayerParams {
    opacity: f32,
    dt: f32,
    /// bit0: apply the layer's levelling gain; bit1: first layer (meets black).
    flags: u32,
    /// `settings::Blend` as u32.
    blend: u32,
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

/// Per-layer modifier chain resources: three ping-pong textures, a history
/// pair per slot that holds a feedback or luminescence modifier (made on
/// demand, dropped when the slot changes kind), a uniform buffer and state
/// per slot.
struct Chain {
    index: usize,
    ping: [wgpu::TextureView; 3],
    hist: Vec<Option<[wgpu::TextureView; 2]>>,
    hist_cur: Vec<usize>,
    /// Mip pyramid of a fractal slot's input, made on demand.
    pyramid: Vec<Option<Pyramid>>,
    /// Wet/dry uniforms, one per slot.
    mix_bufs: Vec<wgpu::Buffer>,
    bufs: Vec<wgpu::Buffer>,
    states: Vec<ModState>,
}

impl Chain {
    fn new(device: &wgpu::Device, i: usize, width: u32, height: u32) -> Self {
        Self {
            index: i,
            ping: [hdr_texture(device, &format!("layer {i} mod a"), width, height), hdr_texture(device, &format!("layer {i} mod b"), width, height), hdr_texture(device, &format!("layer {i} mod c"), width, height)],
            hist: (0..MAX_MODS).map(|_| None).collect(),
            hist_cur: vec![0; MAX_MODS],
            pyramid: (0..MAX_MODS).map(|_| None).collect(),
            mix_bufs: (0..MAX_MODS).map(|j| scene::uniform_buffer(device, &format!("layer {i} mix {j}"), std::mem::size_of::<MixParams>() as u64)).collect(),
            bufs: (0..MAX_MODS).map(|j| scene::uniform_buffer(device, &format!("layer {i} mod {j}"), std::mem::size_of::<GradeParams>() as u64)).collect(),
            states: vec![ModState::default(); MAX_MODS],
        }
    }

    fn resize(&mut self, device: &wgpu::Device, i: usize, width: u32, height: u32) {
        self.index = i;
        self.ping = [hdr_texture(device, &format!("layer {i} mod a"), width, height), hdr_texture(device, &format!("layer {i} mod b"), width, height), hdr_texture(device, &format!("layer {i} mod c"), width, height)];
        // History and pyramids are remade on demand at the new size.
        self.hist = (0..MAX_MODS).map(|_| None).collect();
        self.pyramid = (0..MAX_MODS).map(|_| None).collect();
    }

    /// Give slot `j` a mip pyramid (or take it away) to match its modifier.
    fn want_pyramid(&mut self, device: &wgpu::Device, j: usize, want: bool, width: u32, height: u32) {
        match (want, self.pyramid[j].is_some()) {
            (true, false) => self.pyramid[j] = Some(hdr_pyramid(device, &format!("layer {} pyramid {j}", self.index), width, height, PYRAMID_LEVELS)),
            (false, true) => self.pyramid[j] = None,
            _ => {}
        }
    }

    /// Give slot `j` a history pair (or take it away) to match its modifier.
    fn want_history(&mut self, device: &wgpu::Device, j: usize, want: bool, width: u32, height: u32) {
        match (want, self.hist[j].is_some()) {
            (true, false) => {
                let i = self.index;
                self.hist[j] = Some([hdr_texture(device, &format!("layer {i} hist {j} a"), width, height), hdr_texture(device, &format!("layer {i} hist {j} b"), width, height)]);
                self.hist_cur[j] = 0;
            }
            (false, true) => self.hist[j] = None,
            _ => {}
        }
    }
}

/// One pipeline per modifier kind, in `MOD_SHADERS` order.
const MOD_SHADERS: [&str; 12] = ["fold.wgsl", "feedback.wgsl", "polar.wgsl", "ripple.wgsl", "edge.wgsl", "chroma.wgsl", "mask.wgsl", "fractal.wgsl", "lumi.wgsl", "grade.wgsl", "halftone.wgsl", "warp.wgsl"];
/// Pipeline indices with special plumbing: both keep a history pair.
const MOD_FEEDBACK: usize = 1;
const MOD_LUMI: usize = 8;

const MOD_FRACTAL: usize = 7;
/// Mip levels a fractal's input keeps: 1.9^5 of magnification is lod 4.6.
const PYRAMID_LEVELS: u32 = 6;

fn uses_history(pi: usize) -> bool {
    pi == MOD_FEEDBACK || pi == MOD_LUMI
}

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
        Modifier::Halftone(_) => 10,
        Modifier::Warp(_) => 11,
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
    blend: wgpu::RenderPipeline,
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

/// Manual scene switches and layer switches fade fast; the auto director
/// takes its time and brings the layers in one after another, bottom first.
pub const FADE_MANUAL_S: f32 = 0.5;
/// (seconds, stagger per slot) for a manual scene switch.
pub const FADE_MANUAL: (f32, f32) = (FADE_MANUAL_S, 0.0);
/// (seconds, stagger per slot) for the auto director.
pub const FADE_AUTO: (f32, f32) = (4.0, 0.8);
/// Opacity changes that do not switch a layer on or off (slider drags) follow closely.
const FADE_SLIDER_S: f32 = 0.08;
const MIN_DWELL_S: f32 = 90.0;
const MAX_DWELL_S: f32 = 360.0;
const QUIET_FOR_S: f32 = 6.0;

/// An eased ramp of a layer's opacity from one value to another, after an
/// optional delay.
#[derive(Clone, Copy, Debug)]
struct Fade {
    from: f32,
    to: f32,
    t: f32,
    secs: f32,
    delay: f32,
}

impl Fade {
    /// A finished ramp sitting at `v`.
    fn at(v: f32) -> Self {
        Self { from: v, to: v, t: 1.0, secs: 1.0, delay: 0.0 }
    }

    fn level(&self) -> f32 {
        self.from + (self.to - self.from) * smoothstep(self.t)
    }

    fn done(&self) -> bool {
        self.t >= 1.0
    }

    /// Head for `to` over `secs` seconds, starting after `delay`. A ramp
    /// already aimed there is left alone; otherwise the new one starts from
    /// wherever the current level is.
    fn aim(&mut self, to: f32, secs: f32, delay: f32) {
        if (to - self.to).abs() < 1e-4 {
            return;
        }
        *self = Self { from: self.level(), to, t: 0.0, secs: secs.max(0.02), delay: delay.max(0.0) };
    }

    /// Advance by `dt` and return the level for this frame.
    fn step(&mut self, dt: f32) -> f32 {
        if self.delay > 0.0 {
            self.delay -= dt;
        } else if self.t < 1.0 {
            self.t = (self.t + dt / self.secs).min(1.0);
        }
        self.level()
    }
}

/// Everything a layer slot carries besides its visual instance: band filter,
/// levelling meter, modifier chain, palette, slewed tint, compositor uniform
/// and the opacity ramp.
struct Slot {
    filter: BandPass,
    meter: Meter,
    chain: Chain,
    palette: PaletteState,
    tint: [f32; 3],
    buf: wgpu::Buffer,
    fade: Fade,
}

impl Slot {
    fn new(device: &wgpu::Device, i: usize, width: u32, height: u32, layer: &Layer, level: f32) -> Self {
        Self {
            filter: BandPass::new(20.0, 400.0, 48_000.0),
            meter: Meter::new(device, i, width, height),
            chain: Chain::new(device, i, width, height),
            palette: PaletteState { key: 0, table: color::bake(color::preset("phosphor").unwrap().stops, true), shift: 0.0, last_beat: 0.0 },
            tint: layer.tint.color,
            buf: scene::uniform_buffer(device, &format!("layer {i}"), std::mem::size_of::<LayerParams>() as u64),
            fade: Fade::at(level),
        }
    }

    fn resize(&mut self, device: &wgpu::Device, i: usize, width: u32, height: u32) {
        self.meter.down = hdr_texture(device, &format!("layer {i} down"), width / 8, height / 8);
        self.chain.resize(device, i, width, height);
    }
}

/// A visual that was replaced in its slot. It keeps rendering with the layer
/// settings it had while its opacity ramps to zero, then it is dropped.
struct Outgoing {
    scene: Box<dyn Scene>,
    slot: Slot,
    layer: Layer,
}

/// GPU resources shared by every layer pass.
struct Pipes {
    mod_layout: wgpu::BindGroupLayout,
    mod_pipelines: Vec<wgpu::RenderPipeline>,
    /// Copy / 2x box downsample on the modifier layout (builds pyramids).
    blit: wgpu::RenderPipeline,
    mirror_sampler: wgpu::Sampler,
    sampler: wgpu::Sampler,
    layer_layout: wgpu::BindGroupLayout,
    layer_pipelines: LayerPipelines,
    mix_layout: wgpu::BindGroupLayout,
    mix_pipeline: wgpu::RenderPipeline,
    blend_layout: wgpu::BindGroupLayout,
}

/// Per-frame inputs every layer pass reads.
struct Frame<'a> {
    features: &'a FeatureFrame,
    points: &'a [[f32; 2]],
    sample_rate: u32,
    tint: [f32; 3],
    art: Option<[f32; 3]>,
    art_image: Option<&'a ArtImage>,
    art_id: u64,
    photos_folder: &'a str,
    dt: f32,
    time: f32,
    auto_gain: f32,
    width: u32,
    height: u32,
    /// Levelling on (auto exposure).
    level: bool,
}

pub struct Director {
    pub layers: Vec<Box<dyn Scene>>,
    slots: Vec<Slot>,
    /// Per slot, a replaced visual still fading out under the new one.
    outgoing: Vec<Option<Outgoing>>,
    /// Last frame's layer settings, frozen for a visual on its way out.
    last_layers: Vec<Layer>,
    pipes: Pipes,
    one_buf: wgpu::Buffer,
    composite: wgpu::TextureView,
    /// The other half of the composite ping-pong (blend passes read one, write the other).
    composite_b: wgpu::TextureView,
    width: u32,
    height: u32,
    /// (seconds, stagger per slot) for whatever changes next frame.
    pending: Option<(f32, f32)>,
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
        VisualKind::Waves => Box::new(Waves::new(device, w, h, shaders)?),
    })
}

/// The opacity a slot is heading for: its own when it is on (and not
/// soloed out), else zero.
fn target_of(settings: &Settings, i: usize) -> f32 {
    let l = settings.layer(i);
    if l.enabled && !settings.solo.is_some_and(|k| k != i) { l.opacity.max(0.0) } else { 0.0 }
}

/// Bottom slots lead a staggered fade; slot 1 (on top) comes last.
fn stagger_delay(stagger: f32, i: usize) -> f32 {
    stagger * (MAX_LAYERS - 1 - i) as f32
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
            // p0.w (palette shift) is filled in by the caller from the layer's palette state.
            let strength = k.strength * (0.7 + 0.3 * energy * k.react);
            let colour = match k.colour {
                EdgeColour::Picture => 0.0,
                EdgeColour::Tint => 1.0,
                EdgeColour::Palette => 2.0,
            };
            p.p0 = [strength, k.keep, colour, 0.0];
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
            p.p0 = [k.fresh_s.max(0.02), k.afterglow_s.max(0.1), k.fresh_gain, k.afterglow_gain];
            if k.own_colour {
                // The shader glows in the picture's remembered colour and turns
                // the afterglow by this angle (cos, sin) in YIQ.
                let a = k.hue_shift.to_radians();
                p.p1 = [a.cos(), a.sin(), 0.0, k.excite * k.react];
                p.p2 = [0.0, 0.0, 0.0, k.sparkle * k.react];
            } else {
                let fresh = tint;
                let faded = hue_rotate(tint, k.hue_shift);
                p.p1 = [fresh[0], fresh[1], fresh[2], k.excite * k.react];
                p.p2 = [faded[0], faded[1], faded[2], k.sparkle * k.react];
            }
            // Charge rate keeps the slow pool's steady state near the picture's own level.
            p.p3 = [k.uv, onset, 1.0 / k.afterglow_s.max(0.1), k.own_colour as u32 as f32];
        }
        Modifier::Grade(k) => {
            // p0.z (shift) is filled in by the caller from the layer's palette state.
            let amount = (k.amount * (1.0 + 0.15 * k.react * onset)).clamp(0.0, 1.0);
            p.p0 = [k.by_hue as u32 as f32, amount, 0.0, k.react];
        }
        Modifier::Halftone(k) => {
            // p2.x (palette shift) is filled in by the caller from the layer's palette state.
            let cell = (k.cell * (1.0 + 0.12 * k.react * bass)).max(2.0);
            // Mono wanders slowly on its own and jumps toward one ink on onsets.
            let wander = 0.5 * k.drift * ((time * 0.37).sin() * 0.6 + (time * 0.11).sin() * 0.4);
            let mono = (k.mono + wander + 0.6 * k.drift * k.react * onset).clamp(0.0, 1.0);
            let shape = match k.shape {
                HalftoneShape::Dot => 0.0,
                HalftoneShape::Line => 1.0,
                HalftoneShape::Cross => 2.0,
            };
            p.p0 = [cell, k.angle.to_radians(), shape, mono];
            p.p1 = [tint[0], tint[1], tint[2], k.invert as u32 as f32];
            p.p2 = [0.0, k.palette as u32 as f32, k.gamma, 0.0];
        }
        Modifier::Warp(k) => {
            // The field's clock runs with the music: energy speeds it, onsets kick it.
            st.angle += dt * k.speed * (0.5 + 0.5 * energy * k.react) + dt * 2.0 * k.react * onset;
            let amount = k.amount * (1.0 + 0.6 * k.react * bass.max(0.0));
            p.p0 = [amount, k.scale.max(0.1), st.angle, k.twist];
            p.p1 = [k.octaves.clamp(1, 5) as f32, 0.0, 0.0, 0.0];
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
        let blend_layout = scene::tex_layout(device, "layer blend", 2);
        let layer_pipelines = build_layer(device, shaders, &layer_layout, &blend_layout)?;
        let mix_layout = scene::tex_layout(device, "mix", 1);
        let mix_pipeline = build_mix(device, shaders, &mix_layout)?;
        let one_buf = scene::uniform_buffer(device, "layer one", std::mem::size_of::<LayerParams>() as u64);
        let mut layers = Vec::new();
        let mut slots = Vec::new();
        for i in 0..MAX_LAYERS {
            layers.push(make(settings.layer(i).visual, device, width, height, shaders)?);
            slots.push(Slot::new(device, i, width, height, &settings.layer(i), target_of(settings, i)));
        }
        let mod_layout = scene::tex_layout(device, "mod", 1);
        let mod_pipelines = build_mods(device, shaders, &mod_layout)?;
        let blit = build_blit(device, shaders, &mod_layout)?;
        let mirror_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mod mirror"),
            address_mode_u: wgpu::AddressMode::MirrorRepeat,
            address_mode_v: wgpu::AddressMode::MirrorRepeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Ok(Self {
            layers,
            slots,
            outgoing: (0..MAX_LAYERS).map(|_| None).collect(),
            last_layers: (0..MAX_LAYERS).map(|i| settings.layer(i)).collect(),
            pipes: Pipes { mod_layout, mod_pipelines, blit, mirror_sampler, sampler, layer_layout, layer_pipelines, mix_layout, mix_pipeline, blend_layout },
            one_buf,
            composite: hdr_texture(device, "composite", width, height),
            composite_b: hdr_texture(device, "composite b", width, height),
            width,
            height,
            pending: None,
            last_switch: Instant::now(),
            last_track: None,
            quiet_for: 0.0,
            trend_peak: 0.0,
            last_cue: "manual",
            last_error: None,
        })
    }

    /// Rebuild any layer whose visual kind changed in the settings. The old
    /// visual moves to the slot's outgoing seat and fades out, with the
    /// settings it had, while the new one fades in.
    pub fn sync(&mut self, device: &wgpu::Device, shaders: &ShaderSet, settings: &Settings) {
        let (secs, stagger) = self.pending.unwrap_or((FADE_MANUAL_S, 0.0));
        for i in 0..MAX_LAYERS {
            let want = settings.layer(i).visual;
            if self.layers[i].kind() == want {
                continue;
            }
            match make(want, device, self.width, self.height, shaders) {
                Ok(s) => {
                    let old = std::mem::replace(&mut self.layers[i], s);
                    let mut slot = std::mem::replace(&mut self.slots[i], Slot::new(device, i, self.width, self.height, &settings.layer(i), 0.0));
                    slot.fade.aim(0.0, secs, stagger_delay(stagger, i));
                    self.outgoing[i] = Some(Outgoing { scene: old, slot, layer: self.last_layers[i].clone() });
                }
                Err(e) => self.last_error = Some(format!("{}: {e}", want.name())),
            }
        }
    }

    /// Move layer `from` to position `to`, carrying its live state (particles,
    /// persistence, meter, modifier chain, fade) with it. The caller reorders
    /// `settings.layers` the same way.
    pub fn move_layer(&mut self, from: usize, to: usize) {
        let n = self.layers.len().min(MAX_LAYERS);
        if from >= n || to >= n || from == to {
            return;
        }
        let l = self.layers.remove(from);
        self.layers.insert(to, l);
        let s = self.slots.remove(from);
        self.slots.insert(to, s);
        let o = self.outgoing.remove(from);
        self.outgoing.insert(to, o);
        let p = self.last_layers.remove(from);
        self.last_layers.insert(to, p);
    }

    /// Move a modifier within layer `layer`'s chain so its live state
    /// (counters, history) follows it.
    pub fn move_modifier(&mut self, layer: usize, from: usize, to: usize) {
        if let Some(c) = self.slots.get_mut(layer).map(|s| &mut s.chain) {
            if from < c.states.len() && to < c.states.len() && from != to {
                let s = c.states.remove(from);
                c.states.insert(to, s);
                let h = c.hist.remove(from);
                c.hist.insert(to, h);
                let y = c.pyramid.remove(from);
                c.pyramid.insert(to, y);
                let k = c.hist_cur.remove(from);
                c.hist_cur.insert(to, k);
            }
        }
    }

    /// Drop a modifier's live state when it is removed from the chain.
    pub fn remove_modifier(&mut self, layer: usize, at: usize) {
        if let Some(c) = self.slots.get_mut(layer).map(|s| &mut s.chain) {
            if at < c.states.len() {
                c.states.remove(at);
                c.states.push(ModState::default());
                c.hist.remove(at);
                c.hist.push(None);
                c.pyramid.remove(at);
                c.pyramid.push(None);
                c.hist_cur.remove(at);
                c.hist_cur.push(0);
            }
        }
    }

    pub fn seconds_since_switch(&self) -> f32 {
        self.last_switch.elapsed().as_secs_f32()
    }

    /// Fade whatever the next frame changes (layers switching on, off, to a
    /// new opacity or a new visual) over `seconds`, each slot starting
    /// `stagger` seconds after the one below it.
    pub fn begin_fade(&mut self, (seconds, stagger): (f32, f32)) {
        self.pending = Some((seconds.max(0.05), stagger.max(0.0)));
        self.last_switch = Instant::now();
    }

    pub fn is_fading(&self) -> bool {
        self.pending.is_some() || self.outgoing.iter().any(Option::is_some) || self.slots.iter().any(|s| !s.fade.done())
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
        self.composite_b = hdr_texture(device, "composite b", width, height);
        for (i, s) in self.slots.iter_mut().enumerate() {
            s.resize(device, i, width, height);
        }
        for (i, o) in self.outgoing.iter_mut().enumerate() {
            if let Some(o) = o {
                o.scene.resize(device, width, height);
                o.slot.resize(device, i, width, height);
            }
        }
    }

    pub fn reload_shaders(&mut self, device: &wgpu::Device, shaders: &ShaderSet) {
        let mut errors = Vec::new();
        match build_layer(device, shaders, &self.pipes.layer_layout, &self.pipes.blend_layout) {
            Ok(p) => self.pipes.layer_pipelines = p,
            Err(e) => errors.push(format!("layer: {e}")),
        }
        match build_mods(device, shaders, &self.pipes.mod_layout) {
            Ok(p) => self.pipes.mod_pipelines = p,
            Err(e) => errors.push(format!("modifiers: {e}")),
        }
        match build_blit(device, shaders, &self.pipes.mod_layout) {
            Ok(p) => self.pipes.blit = p,
            Err(e) => errors.push(format!("blit: {e}")),
        }
        if let Err(e) = build_mix(device, shaders, &self.pipes.mix_layout).map(|p| self.pipes.mix_pipeline = p) {
            errors.push(format!("mix: {e}"));
        }
        for s in self.layers.iter_mut().chain(self.outgoing.iter_mut().flatten().map(|o| &mut o.scene)) {
            if let Err(e) = s.reload_shaders(device, shaders) {
                errors.push(format!("{}: {e}", s.name()));
            }
        }
        self.last_error = (!errors.is_empty()).then(|| errors.join("\n"));
    }

    /// Render every visible layer (and any visual still fading out) and
    /// composite into `target`.
    #[allow(clippy::too_many_arguments)]
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, settings: &Settings, features: &FeatureFrame, points: &[[f32; 2]], sample_rate: u32, tint: [f32; 3], art: Option<[f32; 3]>, art_image: Option<&ArtImage>, art_id: u64, dt: f32, time: f32, auto_gain: f32, target: &wgpu::TextureView) {
        queue.write_buffer(&self.one_buf, 0, bytemuck::bytes_of(&LayerParams { opacity: 1.0, dt, flags: 0, blend: 0 }));
        // Levelling: with auto exposure on, each layer is brought to its own
        // dark-field mean before mixing, so opacity is the mix ratio and a dim
        // layer is not lost under a bright one.
        let frame = Frame { features, points, sample_rate, tint, art, art_image, art_id, photos_folder: &settings.photos_folder, dt, time, auto_gain, width: self.width, height: self.height, level: settings.post.auto_exposure };

        // Aim every slot's ramp at its target and advance it. A scene switch
        // fades over its own time (staggered for the auto director); a layer
        // switching on or off takes the manual time; a slider drag follows
        // closely. Entries are bottom to top, so slot 1 ends on top, with a
        // slot's outgoing visual just under its new one.
        let pending = self.pending.take();
        let mut entries: Vec<(bool, usize, f32)> = Vec::new(); // (outgoing, slot, level)
        for i in (0..MAX_LAYERS.min(self.layers.len())).rev() {
            if let Some(o) = self.outgoing[i].as_mut() {
                let level = o.slot.fade.step(dt);
                if level > 0.0 {
                    entries.push((true, i, level));
                }
            }
            let to = target_of(settings, i);
            let slot = &mut self.slots[i];
            let (secs, delay) = match pending {
                Some((secs, stagger)) => (secs, stagger_delay(stagger, i)),
                None if (slot.fade.to > 0.0) != (to > 0.0) => (FADE_MANUAL_S, 0.0),
                None => (FADE_SLIDER_S, 0.0),
            };
            slot.fade.aim(to, secs, delay);
            let level = slot.fade.step(dt);
            if level > 0.0 {
                entries.push((false, i, level));
            }
        }

        // The composite ping-pongs between two textures; the last pass lands
        // in `composite`.
        let n = entries.len();
        for (pass, &(out, i, level)) in entries.iter().enumerate() {
            let first = pass == 0;
            let (dst_read, dst_write) = if (n - 1 - pass) % 2 == 0 { (&self.composite_b, &self.composite) } else { (&self.composite, &self.composite_b) };
            if out {
                let o = self.outgoing[i].as_mut().expect("outgoing entry");
                render_layer(device, queue, encoder, &self.pipes, &frame, o.scene.as_mut(), &mut o.slot, &o.layer, level, first, dst_read, dst_write);
            } else {
                let layer = settings.layer(i);
                render_layer(device, queue, encoder, &self.pipes, &frame, self.layers[i].as_mut(), &mut self.slots[i], &layer, level, first, dst_read, dst_write);
            }
        }
        if n == 0 {
            scene::clear_pass(encoder, "layers (none)", &self.composite);
        }
        for o in &mut self.outgoing {
            if o.as_ref().is_some_and(|o| o.slot.fade.done()) {
                *o = None;
            }
        }
        for i in 0..MAX_LAYERS.min(self.last_layers.len()) {
            self.last_layers[i] = settings.layer(i);
        }

        let group = scene::tex_group(device, &self.pipes.layer_layout, &[&self.composite, &self.slots[0].meter.state[0]], &self.pipes.sampler, &self.one_buf);
        fullscreen_pass(encoder, "composite", target, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipes.layer_pipelines.copy, &group);
    }
}

/// One layer's frame: filter its audio, slew its tint, shift its palette,
/// render the visual, run the modifier chain, meter it, and blend it onto
/// the composite at `level` opacity.
#[allow(clippy::too_many_arguments)]
fn render_layer(device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, pipes: &Pipes, fr: &Frame, visual: &mut dyn Scene, slot: &mut Slot, layer: &Layer, level: f32, first: bool, dst_read: &wgpu::TextureView, dst_write: &wgpu::TextureView) {
    let (dt, time) = (fr.dt, fr.time);
    let band = layer.band;
    let filtered_features;
    let feats: &FeatureFrame = if band.enabled {
        filtered_features = band_features(fr.features, band.lo_hz, band.hi_hz);
        &filtered_features
    } else {
        fr.features
    };
    let filtered_points;
    let pts: &[[f32; 2]] = if band.enabled && layer.visual == VisualKind::Scope {
        slot.filter.retune(band.lo_hz, band.hi_hz, fr.sample_rate as f32);
        filtered_points = slot.filter.process(fr.points);
        &filtered_points
    } else {
        fr.points
    };
    // Per-layer tint: the scene's, the art's, or its own, slewed like the scene tint.
    let want = match layer.tint.mode {
        TintMode::Scene => fr.tint,
        TintMode::Art => fr.art.unwrap_or(fr.tint),
        TintMode::Custom => layer.tint.color,
    };
    let k = 1.0 - (-dt / 0.8).exp();
    for c in 0..3 {
        slot.tint[c] += (want[c] - slot.tint[c]) * k;
    }
    // Palette: rebake when the selection changed; shift it with the music.
    {
        let ps = &mut slot.palette;
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
    let (pal_table, pal_shift, pal_amount) = (&slot.palette.table, slot.palette.shift, if pal_active { layer.palette.amount } else { 0.0 });
    let ctx = FrameCtx { features: feats, tint: slot.tint, dt, time, sample_rate: fr.sample_rate, width: fr.width, height: fr.height, points: pts, auto_gain: fr.auto_gain, art: fr.art_image, art_id: fr.art_id, photos_folder: fr.photos_folder, palette: pal_active.then_some(pal_table), palette_shift: pal_shift, palette_amount: pal_amount };
    visual.render(queue, encoder, &ctx, layer);
    queue.write_buffer(&slot.buf, 0, bytemuck::bytes_of(&LayerParams { opacity: level, dt, flags: fr.level as u32 | if first { 2 } else { 0 }, blend: layer.blend as u32 }));

    // Modifier chain: advance every slot's state and write its uniform
    // first (mutable), then run the passes (immutable borrows only).
    let mut passes: Vec<(usize, usize, f32)> = Vec::new(); // (slot, pipeline index, wet/dry)
    {
        let chain = &mut slot.chain;
        for (j, m) in layer.modifiers.iter().enumerate().take(MAX_MODS) {
            if !m.enabled() {
                chain.want_history(device, j, false, fr.width, fr.height);
                chain.want_pyramid(device, j, false, fr.width, fr.height);
                continue;
            }
            chain.want_history(device, j, uses_history(mod_index(m)), fr.width, fr.height);
            chain.want_pyramid(device, j, mod_index(m) == MOD_FRACTAL, fr.width, fr.height);
            // The modifier follows the layer's band unless it has its own.
            let own_band;
            let mf: &FeatureFrame = match m.band() {
                None => feats,
                Some(b) if b.enabled => {
                    own_band = band_features(fr.features, b.lo_hz, b.hi_hz);
                    &own_band
                }
                Some(_) => fr.features,
            };
            let mut p = mod_params(m, &mut chain.states[j], mf, slot.tint, dt, time, fr.width, fr.height);
            if matches!(m, Modifier::Grade(_) | Modifier::Edge(_) | Modifier::Halftone(_)) {
                // All three read the palette table; the shift goes where each shader documents.
                match m {
                    Modifier::Grade(_) => p.p0[2] = pal_shift,
                    Modifier::Edge(_) => p.p0[3] = pal_shift,
                    _ => p.p2[0] = pal_shift,
                }
                queue.write_buffer(&chain.bufs[j], 0, bytemuck::bytes_of(&GradeParams { m: p, pal: *pal_table }));
            } else {
                queue.write_buffer(&chain.bufs[j], 0, bytemuck::bytes_of(&p));
            }
            let mix = m.mix().clamp(0.0, 1.0);
            if mix < 0.999 {
                queue.write_buffer(&chain.mix_bufs[j], 0, bytemuck::bytes_of(&MixParams { t: mix, _pad: [0.0; 3] }));
            }
            passes.push((j, mod_index(m), mix));
        }
    }
    let chain = &slot.chain;
    let mut src: &wgpu::TextureView = visual.output();
    let mut pp = 0usize;
    let mut flips: Vec<usize> = Vec::new();
    for (j, pi, mix) in passes {
        let input = src;
        // Each history user has its own pair, so two in one chain never
        // read the texture the other is writing.
        let hist = chain.hist[j].as_ref();
        let (hist_prev, hist_cur) = (chain.hist_cur[j], 1 - chain.hist_cur[j]);
        let second: &wgpu::TextureView = hist.map_or(src, |h| &h[hist_prev]);
        // A fractal reads a mip pyramid of its input: copy, then halve level
        // by level (the blit's one linear tap at half size is a 2x2 box).
        let mut first_tex = src;
        if let (MOD_FRACTAL, Some(py)) = (pi, chain.pyramid[j].as_ref()) {
            let mut from = src;
            for (lvl, view) in py.levels.iter().enumerate() {
                let g = scene::tex_group(device, &pipes.mod_layout, &[from, from], &pipes.mirror_sampler, &chain.bufs[j]);
                fullscreen_pass(encoder, if lvl == 0 { "pyramid copy" } else { "pyramid level" }, view, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.blit, &g);
                from = view;
            }
            first_tex = &py.all;
        }
        let g = scene::tex_group(device, &pipes.mod_layout, &[first_tex, second], &pipes.mirror_sampler, &chain.bufs[j]);
        if let (MOD_FEEDBACK, Some(h)) = (pi, hist) {
            // Feedback's output is its own history.
            fullscreen_pass(encoder, "modifier", &h[hist_cur], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.mod_pipelines[pi], &g);
            src = &h[hist_cur];
            flips.push(j);
        } else if let (MOD_LUMI, Some(h)) = (pi, hist) {
            // Luminescence writes its picture and, beside it, its pool state.
            fullscreen_pass2(encoder, "luminescence", &chain.ping[pp], &h[hist_cur], &pipes.mod_pipelines[pi], &g);
            src = &chain.ping[pp];
            pp = (pp + 1) % 3;
            flips.push(j);
        } else {
            fullscreen_pass(encoder, "modifier", &chain.ping[pp], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.mod_pipelines[pi], &g);
            src = &chain.ping[pp];
            pp = (pp + 1) % 3;
        }
        // Wet/dry: blend the pass's output back toward its input. Three
        // ping textures guarantee the input is not the one being written.
        if mix < 0.999 {
            let g = scene::tex_group(device, &pipes.mix_layout, &[input, src], &pipes.sampler, &chain.mix_bufs[j]);
            fullscreen_pass(encoder, "modifier mix", &chain.ping[pp], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.mix_pipeline, &g);
            src = &chain.ping[pp];
            pp = (pp + 1) % 3;
        }
    }
    let out = src;
    let m = &slot.meter;
    let (prev_s, cur_s) = (m.cur, 1 - m.cur);
    if fr.level {
        let g = scene::tex_group(device, &pipes.layer_layout, &[out, &m.state[prev_s]], &pipes.sampler, &slot.buf);
        fullscreen_pass(encoder, "layer down", &m.down, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.layer_pipelines.down, &g);
        let g = scene::tex_group(device, &pipes.layer_layout, &[&m.down, &m.state[prev_s]], &pipes.sampler, &slot.buf);
        fullscreen_pass(encoder, "layer meter", &m.state[cur_s], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.layer_pipelines.meter, &g);
    }
    let group = scene::tex_group(device, &pipes.blend_layout, &[out, &m.state[cur_s], dst_read], &pipes.sampler, &slot.buf);
    fullscreen_pass(encoder, "layer", dst_write, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.layer_pipelines.blend, &group);
    if fr.level {
        slot.meter.cur = cur_s;
    }
    for j in flips {
        slot.chain.hist_cur[j] = 1 - slot.chain.hist_cur[j];
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

fn build_layer(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout, blend_layout: &wgpu::BindGroupLayout) -> Result<LayerPipelines, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "layer.wgsl");
        let b = scene::shader(device, shaders, "layer_blend.wgsl");
        let d = scene::shader(device, shaders, "layer_down.wgsl");
        let g = scene::shader(device, shaders, "layer_meter.wgsl");
        LayerPipelines {
            blend: scene::pipeline(device, "layer blend", blend_layout, &b, HDR_FORMAT, None),
            copy: scene::pipeline(device, "layer copy", layout, &m, HDR_FORMAT, None),
            down: scene::pipeline(device, "layer down", layout, &d, HDR_FORMAT, None),
            meter: scene::pipeline(device, "layer meter", layout, &g, HDR_FORMAT, None),
        }
    })
}

fn build_blit(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "blit.wgsl");
        scene::pipeline(device, "blit", layout, &m, HDR_FORMAT, None)
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
    fn fade_eases_to_its_target_after_the_delay() {
        let mut f = Fade::at(1.0);
        assert!(f.done());
        f.aim(0.0, 1.0, 0.5);
        assert_eq!(f.step(0.25), 1.0, "holds during the delay");
        assert_eq!(f.step(0.25), 1.0);
        let mid = f.step(0.5);
        assert!((mid - 0.5).abs() < 1e-5, "{mid}");
        assert!(!f.done());
        assert_eq!(f.step(0.5), 0.0);
        assert!(f.done());
    }

    #[test]
    fn aim_restarts_from_the_current_level_and_ignores_repeats() {
        let mut f = Fade::at(0.0);
        f.aim(1.0, 1.0, 0.0);
        f.step(0.5);
        let before = f;
        f.aim(1.0, 0.1, 0.0);
        assert_eq!(f.t, before.t, "same target: left alone");
        f.aim(0.2, 1.0, 0.0);
        assert!((f.from - 0.5).abs() < 1e-5 && f.t == 0.0 && f.to == 0.2);
    }

    #[test]
    fn every_modifier_kind_has_a_shader() {
        for m in Modifier::KINDS.iter().map(|(_, make)| make()) {
            assert!(mod_index(&m) < MOD_SHADERS.len());
        }
    }
}
