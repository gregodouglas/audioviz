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
use crate::rng::Rng;
use crate::settings::{DirectorMode, EdgeColour, Space, FoldMode, FractalMode, HalftoneShape, Layer, MirrorSide, MAX_LAYERS, MAX_MODS, MaskShape, Modifier, Settings, TintMode};
use crate::shaders::ShaderSet;
use crate::gpu_timer::{GpuTimer, Part};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct LayerParams {
    opacity: f32,
    dt: f32,
    /// bit0: apply the layer's levelling gain; bit1: first layer (meets
    /// black); bit2: level by the bright part only (`Scene::level_bright`);
    /// bit3: a shared space (a higher bright-part floor).
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
    /// A glow slot's quarter-size pair and sub-pass uniforms, made on demand.
    glow: Vec<Option<GlowTex>>,
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
            glow: (0..MAX_MODS).map(|_| None).collect(),
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
        self.glow = (0..MAX_MODS).map(|_| None).collect();
    }

    /// Give slot `j` a glow pair (or take it away) to match its modifier.
    fn want_glow(&mut self, device: &wgpu::Device, j: usize, want: bool, width: u32, height: u32) {
        match (want, self.glow[j].is_some()) {
            (true, false) => {
                let i = self.index;
                let (w, h) = ((width / 4).max(1), (height / 4).max(1));
                self.glow[j] = Some(GlowTex {
                    a: hdr_texture(device, &format!("layer {i} glow {j} a"), w, h),
                    b: hdr_texture(device, &format!("layer {i} glow {j} b"), w, h),
                    bufs: std::array::from_fn(|k| scene::uniform_buffer(device, &format!("layer {i} glow {j} pass {k}"), std::mem::size_of::<ModParams>() as u64)),
                });
            }
            (false, true) => self.glow[j] = None,
            _ => {}
        }
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

/// A glow slot's quarter-size ping-pong and the uniforms of its three
/// sub-passes (prefilter, horizontal blur, vertical blur).
struct GlowTex {
    a: wgpu::TextureView,
    b: wgpu::TextureView,
    bufs: [wgpu::Buffer; 3],
}

/// One pipeline per modifier kind, in `MOD_SHADERS` order.
const MOD_SHADERS: [&str; 17] = ["fold.wgsl", "feedback.wgsl", "polar.wgsl", "ripple.wgsl", "edge.wgsl", "chroma.wgsl", "mask.wgsl", "fractal.wgsl", "lumi.wgsl", "grade.wgsl", "halftone.wgsl", "warp.wgsl", "lines.wgsl", "mirror.wgsl", "glow.wgsl", "vignette.wgsl", "colorify.wgsl"];
/// Pipeline indices with special plumbing: both keep a history pair.
const MOD_FEEDBACK: usize = 1;
const MOD_LUMI: usize = 8;

const MOD_FRACTAL: usize = 7;
/// Glow blurs a quarter-size copy in sub-passes before its composite.
const MOD_GLOW: usize = 14;
/// Colorify reads the layer meter (levelled brightness picks the palette place).
const MOD_COLORIFY: usize = 16;
/// Mip levels a fractal's input keeps: 1.9^5 of magnification is lod 4.6.
const PYRAMID_LEVELS: u32 = 6;

/// Below this zoom a kaleidoscope reads its input through a mip pyramid.
const FOLD_PYRAMID_ZOOM: f32 = 0.9;
const MOD_FOLD: usize = 0;

/// The mip level a kaleidoscope at `zoom` reads (0 when it has no pyramid).
fn fold_lod(zoom: f32) -> f32 {
    if zoom < FOLD_PYRAMID_ZOOM { (-zoom.max(0.05).log2()).clamp(0.0, (PYRAMID_LEVELS - 1) as f32) } else { 0.0 }
}

/// Whether this modifier reads its input through a mip pyramid.
fn uses_pyramid(m: &Modifier) -> bool {
    match m {
        Modifier::Fractal(_) => true,
        // The zoom breathes with the bass by a few percent; the margin keeps
        // the pyramid from being made and dropped around the threshold.
        Modifier::Fold(k) => k.zoom < FOLD_PYRAMID_ZOOM,
        _ => false,
    }
}

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
        Modifier::Lines(_) => 12,
        Modifier::Mirror(_) => 13,
        Modifier::Glow(_) => 14,
        Modifier::Vignette(_) => 15,
        Modifier::Colorify(_) => 16,
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
    /// The same, added to what is there (a space's glow onto its picture's).
    down_add: wgpu::RenderPipeline,
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
/// A layer whose visual had to be built first (at start, or a visual
/// switched to) fades up out of the dark this slowly: after seconds of
/// black, a scene snapping in all at once is a jolt. The very first scene
/// fades on the screen instead (`screen`), so its layers draw at full
/// strength from their first frame and the levelling measures the real
/// picture (fading the layer, the meters chased the fade and overshot).
const FADE_ARRIVE_S: f32 = 2.0;
/// The first scene's screen stays dark this long before fading up, while
/// the levelling finds its level: the first lit frame is not the picture
/// (a jelly scene's first frame is only its plankton, its bells come a frame
/// later), so the level it gives is too high until the meters come down.
const ARRIVE_HOLD_S: f32 = 0.6;
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

/// The effects director's live state for one layer: beat count, roll count,
/// which pool slots are live, and each slot's slewed gate.
struct FxState {
    beats: u32,
    rolls: u32,
    live: [bool; MAX_MODS],
    gate: [f32; MAX_MODS],
    last_beat: f32,
    started: bool,
}

impl Default for FxState {
    fn default() -> Self {
        Self { beats: 0, rolls: 0, live: [true; MAX_MODS], gate: [1.0; MAX_MODS], last_beat: 0.0, started: false }
    }
}

/// Everything a layer slot carries besides its visual instance: band filter,
/// levelling meter, modifier chain, palette, slewed tint, compositor uniform,
/// the opacity ramp and the effects director's state.
struct Slot {
    filter: BandPass,
    meter: Meter,
    chain: Chain,
    palette: PaletteState,
    tint: [f32; 3],
    buf: wgpu::Buffer,
    fade: Fade,
    /// A scene switch's (seconds, delay) for a visual that was not ready
    /// when it came, used once it is.
    held: Option<(f32, f32)>,
    fx: FxState,
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
            held: None,
            fx: FxState::default(),
        }
    }

    fn resize(&mut self, device: &wgpu::Device, i: usize, width: u32, height: u32) {
        self.meter.down = hdr_texture(device, &format!("layer {i} down"), width / 8, height / 8);
        self.chain.resize(device, i, width, height);
    }
}

/// A shared space's targets (ADR-0002): the multisampled picture every
/// member draws into and what it resolves to; the light target beside it
/// (rgb the glow, alpha the light depth) and its resolve; the diffusion's
/// quarter- and eighth-size pairs and the uniforms of its passes; the
/// space's one meter and compositor uniform. Made when a scene shares its
/// space, dropped when not.
struct SpaceTargets {
    ms: wgpu::TextureView,
    out: wgpu::TextureView,
    light_ms: wgpu::TextureView,
    light: wgpu::TextureView,
    quarter: [wgpu::TextureView; 2],
    eighth: [wgpu::TextureView; 2],
    /// Down, across, up at quarter size; the same at eighth size; adding back.
    light_bufs: [wgpu::Buffer; 7],
    meter: Meter,
    buf: wgpu::Buffer,
}

impl SpaceTargets {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let (qw, qh) = ((width / 4).max(1), (height / 4).max(1));
        let (ew, eh) = ((width / 8).max(1), (height / 8).max(1));
        Self {
            ms: crate::shader_visual::msaa_target(device, width, height),
            out: hdr_texture(device, "space", width, height),
            light_ms: crate::shader_visual::msaa_target(device, width, height),
            light: hdr_texture(device, "space light", width, height),
            quarter: [hdr_texture(device, "space light quarter a", qw, qh), hdr_texture(device, "space light quarter b", qw, qh)],
            eighth: [hdr_texture(device, "space light eighth a", ew, eh), hdr_texture(device, "space light eighth b", ew, eh)],
            light_bufs: std::array::from_fn(|k| scene::uniform_buffer(device, &format!("space light pass {k}"), std::mem::size_of::<ModParams>() as u64)),
            meter: Meter::new(device, MAX_LAYERS, width, height),
            buf: scene::uniform_buffer(device, "space", std::mem::size_of::<LayerParams>() as u64),
        }
    }
}

/// Diffusion's two blurs, as a fraction of the picture's short side (the
/// Gaussian's sigma), and how much of each is added back.
const DIFFUSE_NARROW: f32 = 0.006;
const DIFFUSE_WIDE: f32 = 0.028;
const DIFFUSE_MIX: [f32; 2] = [0.65, 0.35];
/// The far plane of the light depth (`LIGHT_FAR` in the visual API).
const LIGHT_FAR: f64 = 50.0;

/// One entry of the stack, bottom to top: a layer (outgoing or not, its
/// slot, its level), or the shared space, drawn where its top member sits.
#[derive(Clone, Copy)]
enum Entry {
    Layer(bool, usize, f32),
    Space,
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
    /// Glow's prefilter and blur sub-passes, on the modifier layout.
    glow_blur: wgpu::RenderPipeline,
    /// A shared space's diffusion added back (visual_light.wgsl).
    light: wgpu::RenderPipeline,
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
    /// A scene switch landed this frame: effects directors roll.
    scene_changed: bool,
}

pub struct Director {
    pub layers: Vec<Box<dyn Scene>>,
    /// The visual id each slot was built for (a missing visual draws the scope).
    ids: Vec<String>,
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
    /// The shared space, while the scene has one.
    space: Option<SpaceTargets>,
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
    /// The lab's view of a shared space (ADR-0003): the camera turned by
    /// (yaw, pitch) degrees, its distance scaled, what it looks at slid
    /// (right, up) in units of the distance; and which pass shows.
    pub camera_nudge: [f32; 5],
    pub pass: SpacePass,
    nudge: Option<crate::shader_visual::CameraNudge>,
    /// The screen fading up when the first scene arrives (see FADE_ARRIVE_S),
    /// and whether anything has drawn yet.
    screen: Fade,
    drawn: bool,
}

/// The camera nudge that leaves the camera as the visual put it.
pub const LAB_CAMERA: [f32; 5] = [0.0, 0.0, 1.0, 0.0, 0.0];

/// What of a shared space reaches the picture (the lab's passes; the app
/// always shows `All`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpacePass {
    /// The space as it is: sharp light and its diffusion.
    #[default]
    All,
    /// Only the sharp light (no diffusion).
    Sharp,
    /// Only what diffusion adds.
    Glow,
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
            p.p1 = [0.45, off[0], off[1], fold_lod(zoom)];
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
        Modifier::Lines(k) => {
            st.angle += dt * k.react * (0.4 + 2.0 * energy) + 3.0 * k.react * onset * dt;
            p.p0 = [k.pitch.max(1.0), k.angle.to_radians(), k.cut, k.glow];
            p.p1 = [tint[0], tint[1], tint[2], st.angle];
        }
        Modifier::Mirror(k) => {
            // `beats` counts flips; `wander` holds the flipped state.
            if beat_hit && k.flip > 0.0 && k.react > 0.0 {
                st.beats = st.beats.wrapping_add(1);
                // A cheap hash of the beat count decides the flip.
                let h = (st.beats.wrapping_mul(2654435761) >> 8) as f32 / (1u32 << 24) as f32;
                if h < k.flip {
                    st.wander = 1.0 - st.wander;
                }
            }
            let side = match k.side {
                MirrorSide::Left => 0.0,
                MirrorSide::Right => 1.0,
                MirrorSide::Top => 2.0,
                MirrorSide::Bottom => 3.0,
                MirrorSide::Quad => 4.0,
            };
            p.p0 = [side, k.additive as u32 as f32, st.wander, 0.0];
        }
        Modifier::Glow(k) => {
            // p1 (threshold, level on) and the blur width go to the sub-passes; the caller writes those.
            let amount = k.amount * (1.0 + k.react * (0.5 * (energy - 1.0).clamp(-1.0, 1.5) + 0.8 * onset));
            p.p0 = [amount.max(0.0), 0.0, 0.0, 0.0];
        }
        Modifier::Vignette(k) => {
            // The frame opens a little on the bass.
            let open = 0.15 * k.react * bass.max(0.0);
            p.p0 = [k.amount, k.softness, k.roundness, open];
        }
        Modifier::Colorify(k) => {
            // p0.w (level on) and p1.w (palette shift) are filled in by the caller.
            st.angle = (st.angle + dt * k.speed / 360.0 * (0.5 + 0.5 * energy * k.react) + 0.3 * k.react * onset * dt).rem_euclid(1.0);
            let strength = (k.strength * (1.0 + 0.2 * k.react * onset)).clamp(0.0, 1.0);
            p.p0 = [strength, k.palette as u32 as f32, st.angle, 0.0];
            p.p1 = [tint[0], tint[1], tint[2], 0.0];
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
            layers.push(crate::visuals::make(&settings.layer(i).visual, device, width, height, shaders)?);
            slots.push(Slot::new(device, i, width, height, &settings.layer(i), target_of(settings, i)));
        }
        let mod_layout = scene::tex_layout(device, "mod", 1);
        let mod_pipelines = build_mods(device, shaders, &mod_layout)?;
        let blit = build_blit(device, shaders, &mod_layout)?;
        let glow_blur = build_glow_blur(device, shaders, &mod_layout)?;
        let light = build_light(device, shaders, &mod_layout)?;
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
            ids: (0..MAX_LAYERS).map(|i| settings.layer(i).visual.clone()).collect(),
            layers,
            slots,
            outgoing: (0..MAX_LAYERS).map(|_| None).collect(),
            last_layers: (0..MAX_LAYERS).map(|i| settings.layer(i)).collect(),
            pipes: Pipes { mod_layout, mod_pipelines, blit, glow_blur, light, mirror_sampler, sampler, layer_layout, layer_pipelines, mix_layout, mix_pipeline, blend_layout },
            one_buf,
            composite: hdr_texture(device, "composite", width, height),
            composite_b: hdr_texture(device, "composite b", width, height),
            space: None,
            width,
            height,
            pending: None,
            last_switch: Instant::now(),
            last_track: None,
            quiet_for: 0.0,
            trend_peak: 0.0,
            last_cue: "manual",
            last_error: None,
            camera_nudge: LAB_CAMERA,
            pass: SpacePass::All,
            nudge: None,
            screen: Fade::at(0.0),
            drawn: false,
        })
    }

    /// Rebuild any layer whose visual kind changed in the settings. The old
    /// visual moves to the slot's outgoing seat and fades out, with the
    /// settings it had, while the new one fades in.
    pub fn sync(&mut self, device: &wgpu::Device, shaders: &ShaderSet, settings: &Settings) {
        let (secs, stagger) = self.pending.unwrap_or((FADE_MANUAL_S, 0.0));
        for i in 0..MAX_LAYERS {
            let want = &settings.layer(i).visual;
            if &self.ids[i] == want {
                continue;
            }
            self.ids[i] = want.clone();
            match crate::visuals::make(want, device, self.width, self.height, shaders) {
                Ok(s) => {
                    let old = std::mem::replace(&mut self.layers[i], s);
                    let mut slot = std::mem::replace(&mut self.slots[i], Slot::new(device, i, self.width, self.height, &settings.layer(i), 0.0));
                    slot.fade.aim(0.0, secs, stagger_delay(stagger, i));
                    self.outgoing[i] = Some(Outgoing { scene: old, slot, layer: self.last_layers[i].clone() });
                }
                Err(e) => self.last_error = Some(format!("{}: {e}", crate::visuals::name(want))),
            }
        }
    }

    /// Let every visual check its sources (loaded visuals reload when saved).
    pub fn poll(&mut self, device: &wgpu::Device, shaders: &ShaderSet) {
        for s in self.layers.iter_mut().chain(self.outgoing.iter_mut().flatten().map(|o| &mut o.scene)) {
            s.poll(device, shaders);
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
        let k = self.ids.remove(from);
        self.ids.insert(to, k);
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
                let g = c.glow.remove(from);
                c.glow.insert(to, g);
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
                c.glow.remove(at);
                c.glow.push(None);
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
        if self.space.is_some() {
            self.space = Some(SpaceTargets::new(device, width, height));
        }
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
        match build_glow_blur(device, shaders, &self.pipes.mod_layout) {
            Ok(p) => self.pipes.glow_blur = p,
            Err(e) => errors.push(format!("glow: {e}")),
        }
        match build_light(device, shaders, &self.pipes.mod_layout) {
            Ok(p) => self.pipes.light = p,
            Err(e) => errors.push(format!("diffusion: {e}")),
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

    /// How far the finished picture is dimmed this frame (PostParams::dark):
    /// the screen fading up as the first scene arrives.
    pub fn screen_dark(&self) -> f32 {
        1.0 - self.screen.level()
    }

    /// Render every visible layer (and any visual still fading out) and
    /// composite into `target`.
    #[allow(clippy::too_many_arguments)]
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, settings: &Settings, features: &FeatureFrame, points: &[[f32; 2]], sample_rate: u32, tint: [f32; 3], art: Option<[f32; 3]>, art_image: Option<&ArtImage>, art_id: u64, dt: f32, time: f32, auto_gain: f32, target: &wgpu::TextureView, timer: &mut GpuTimer) {
        queue.write_buffer(&self.one_buf, 0, bytemuck::bytes_of(&LayerParams { opacity: 1.0, dt, flags: 0, blend: 0 }));
        // Levelling: with auto exposure on, each layer is brought to its own
        // dark-field mean before mixing, so opacity is the mix ratio and a dim
        // layer is not lost under a bright one.
        let frame = Frame { features, points, sample_rate, tint, art, art_image, art_id, photos_folder: &settings.photos_folder, dt, time, auto_gain, width: self.width, height: self.height, level: settings.post.auto_exposure, scene_changed: self.pending.is_some() };

        // Aim every slot's ramp at its target and advance it. A scene switch
        // fades over its own time (staggered for the auto director); a layer
        // switching on or off takes the manual time; a slider drag follows
        // closely. Entries are bottom to top, so slot 1 ends on top, with a
        // slot's outgoing visual just under its new one.
        let pending = self.pending.take();
        let mut entries: Vec<(bool, usize, f32)> = Vec::new(); // (outgoing, slot, level)
        for i in (0..MAX_LAYERS.min(self.layers.len())).rev() {
            // A visual still compiling holds its slot dark, and the visual it
            // replaced stays as it is, until it can draw.
            let ready = self.layers[i].ready();
            if let Some(o) = self.outgoing[i].as_mut() {
                let level = if ready { o.slot.fade.step(dt) } else { o.slot.fade.level() };
                if level > 0.0 {
                    entries.push((true, i, level));
                }
            }
            let to = target_of(settings, i);
            let slot = &mut self.slots[i];
            if !ready {
                if let Some((secs, stagger)) = pending {
                    slot.held = Some((secs, stagger_delay(stagger, i)));
                } else if slot.held.is_none() {
                    slot.held = Some((FADE_ARRIVE_S, 0.0));
                }
                slot.fade = Fade::at(0.0);
                continue;
            }
            if !self.drawn {
                // The first scene: in at full strength, the screen fades.
                slot.held = None;
                slot.fade = Fade::at(to);
            }
            let (secs, delay) = match (pending, slot.held.take()) {
                (Some((secs, stagger)), _) => (secs, stagger_delay(stagger, i)),
                (None, Some(held)) => held,
                (None, None) if (slot.fade.to > 0.0) != (to > 0.0) => (FADE_MANUAL_S, 0.0),
                (None, None) => (FADE_SLIDER_S, 0.0),
            };
            slot.fade.aim(to, secs, delay);
            let level = slot.fade.step(dt);
            if level > 0.0 {
                entries.push((false, i, level));
            }
        }

        if !self.drawn && !entries.is_empty() {
            self.drawn = true;
            self.screen.aim(1.0, FADE_ARRIVE_S, ARRIVE_HOLD_S);
        }
        if self.drawn {
            self.screen.step(dt);
        }

        // In a shared space, the layers whose visual can join draw as one
        // entry where the top one of them sits; the rest stay flat.
        let shared = settings.space == Space::Shared;
        let members: Vec<(bool, usize, f32)> = if shared { entries.iter().copied().filter(|&(out, i, _)| self.visual(out, i).joins_space()).collect() } else { Vec::new() };
        let top = members.last().copied();
        let stack: Vec<Entry> = entries.iter().filter_map(|&e| if !members.contains(&e) { Some(Entry::Layer(e.0, e.1, e.2)) } else if Some(e) == top { Some(Entry::Space) } else { None }).collect();
        if members.is_empty() {
            self.space = None;
        } else if self.space.is_none() {
            self.space = Some(SpaceTargets::new(device, self.width, self.height));
        }

        // The composite ping-pongs between two textures; the last pass lands
        // in `composite`.
        let n = stack.len();
        for (pass, &entry) in stack.iter().enumerate() {
            let first = pass == 0;
            let (dst_read, dst_write) = if (n - 1 - pass) % 2 == 0 { (&self.composite_b, &self.composite) } else { (&self.composite, &self.composite_b) };
            match entry {
                Entry::Layer(true, i, level) => {
                    let o = self.outgoing[i].as_mut().expect("outgoing entry");
                    render_layer(device, queue, encoder, &self.pipes, &frame, o.scene.as_mut(), &mut o.slot, &o.layer, level, first, dst_read, dst_write, timer);
                }
                Entry::Layer(false, i, level) => {
                    let layer = settings.layer(i);
                    render_layer(device, queue, encoder, &self.pipes, &frame, self.layers[i].as_mut(), &mut self.slots[i], &layer, level, first, dst_read, dst_write, timer);
                }
                Entry::Space => {
                    let (dst_read, dst_write) = (dst_read.clone(), dst_write.clone());
                    self.render_space(device, queue, encoder, settings, &frame, &members, first, &dst_read, &dst_write, timer);
                }
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

        timer.mark(encoder, Part::Composite);
        let group = scene::tex_group(device, &self.pipes.layer_layout, &[&self.composite, &self.slots[0].meter.state[0]], &self.pipes.sampler, &self.one_buf);
        fullscreen_pass(encoder, "composite", target, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.pipes.layer_pipelines.copy, &group);
    }
}

impl Director {
    /// The visual of a stack entry: slot `i`'s, or the one fading out of it.
    fn visual(&self, out: bool, i: usize) -> &dyn Scene {
        if out { self.outgoing[i].as_ref().expect("outgoing entry").scene.as_ref() } else { self.layers[i].as_ref() }
    }

    /// The shared space's frame (ADR-0002): every member's update (the
    /// leading one first, which hands the space its camera), then one
    /// multisampled pass they all draw into, bottom to top (the picture and
    /// the light target), the diffusion of their glow, then one meter and
    /// one blend onto the composite. Members' effects are bypassed: the
    /// space is one picture.
    #[allow(clippy::too_many_arguments)]
    fn render_space(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, settings: &Settings, fr: &Frame, members: &[(bool, usize, f32)], first: bool, dst_read: &wgpu::TextureView, dst_write: &wgpu::TextureView, timer: &mut GpuTimer) {
        let lead = members.iter().rposition(|&(out, i, _)| self.visual(out, i).leads_space());
        let order: Vec<usize> = lead.into_iter().chain((0..members.len()).filter(|&k| Some(k) != lead)).collect();
        for k in order {
            let (out, i, level) = members[k];
            timer.mark(encoder, Part::Visual(i as u8));
            let (visual, slot, layer) = if out {
                let o = self.outgoing[i].as_mut().expect("outgoing entry");
                (o.scene.as_mut(), &mut o.slot, o.layer.clone())
            } else {
                (self.layers[i].as_mut(), &mut self.slots[i], settings.layer(i))
            };
            let input = layer_input(fr, slot, &layer);
            let ctx = input.ctx(fr, slot, &layer);
            visual.space_update(queue, encoder, &ctx, &layer, level * layer.brightness.exp2());
            if Some(k) == lead {
                visual.space_camera(encoder);
                if self.camera_nudge != LAB_CAMERA {
                    self.nudge.get_or_insert_with(|| crate::shader_visual::CameraNudge::new(device)).apply(queue, encoder, self.camera_nudge);
                }
            }
        }
        let glows = members.iter().any(|&(out, i, _)| self.visual(out, i).space_glows());
        let bright = members.iter().any(|&(out, i, _)| self.visual(out, i).level_bright());
        // Stacked, each layer is levelled to the target and they add; the
        // space is levelled once, so it counts as its members (by their
        // levels) to reach the Picture as bright as they would have.
        let weight = members.iter().map(|m| m.2.min(1.0)).sum::<f32>().max(1.0);
        let Some(sp) = self.space.as_mut() else { return };
        queue.write_buffer(&sp.buf, 0, bytemuck::bytes_of(&LayerParams { opacity: if fr.level { weight } else { 1.0 }, dt: fr.dt, flags: fr.level as u32 | if first { 2 } else { 0 } | if bright { 4 | 8 } else { 0 }, blend: 0 }));
        let top = members.last().map_or(0, |m| m.1) as u8;
        {
            // The light target starts with no glow, everything at the far plane.
            let far = wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: LIGHT_FAR };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shared space"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment { view: &sp.ms, depth_slice: None, resolve_target: Some(&sp.out), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Discard } }),
                    Some(wgpu::RenderPassColorAttachment { view: &sp.light_ms, depth_slice: None, resolve_target: Some(&sp.light), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(far), store: wgpu::StoreOp::Discard } }),
                ],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            for &(out, i, _) in members {
                let v: &dyn Scene = if out { self.outgoing[i].as_ref().expect("outgoing entry").scene.as_ref() } else { self.layers[i].as_ref() };
                v.space_draw(&mut pass);
            }
        }
        // The meter sees the light as drawn, the glow still sharp: spreading
        // light over more of the picture lowers the mean of its lit part,
        // and the levelling would lift everything to make up for it.
        if fr.level {
            meter_down(device, encoder, &self.pipes, &sp.meter, &sp.buf, &sp.out, glows.then_some(&sp.light));
        }
        if self.pass == SpacePass::Glow {
            scene::clear_pass(encoder, "space glow only", &sp.out);
        }
        if glows && self.pass != SpacePass::Sharp {
            timer.mark(encoder, Part::Diffusion);
            diffuse(device, queue, encoder, &self.pipes, sp, fr);
        }
        timer.mark(encoder, Part::Blend(top));
        meter_and_blend(device, encoder, &self.pipes, fr.level, true, &mut sp.meter, &sp.buf, &sp.out, dst_read, dst_write);
    }
}

/// The light of a shared space's members, diffused: the light target taken
/// down to a quarter and blurred narrow, that taken down to an eighth and
/// blurred wide (glow's own prefilter and blur, with no threshold), both
/// added back into the space's picture.
fn diffuse(device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, pipes: &Pipes, sp: &SpaceTargets, fr: &Frame) {
    let short = fr.width.min(fr.height) as f32;
    let (narrow, wide) = ((DIFFUSE_NARROW * short / 4.0).max(0.5), (DIFFUSE_WIDE * short / 8.0).max(0.5));
    let sub = |mode: f32, dir: [f32; 2], sigma: f32| ModParams { resolution: [fr.width as f32, fr.height as f32], time: fr.time, dt: fr.dt, p0: [mode, dir[0], dir[1], sigma], ..Default::default() };
    let subs = [sub(0.0, [0.0, 0.0], narrow), sub(1.0, [1.0, 0.0], narrow), sub(1.0, [0.0, 1.0], narrow), sub(0.0, [0.0, 0.0], wide), sub(1.0, [1.0, 0.0], wide), sub(1.0, [0.0, 1.0], wide), ModParams { p0: [DIFFUSE_MIX[0], DIFFUSE_MIX[1], 0.0, 0.0], ..Default::default() }];
    for (buf, p) in sp.light_bufs.iter().zip(subs.iter()) {
        queue.write_buffer(buf, 0, bytemuck::bytes_of(p));
    }
    let (q, e) = (&sp.quarter, &sp.eighth);
    let steps: [(&wgpu::TextureView, &wgpu::TextureView, &str); 6] = [(&sp.light, &q[0], "diffusion down"), (&q[0], &q[1], "diffusion narrow x"), (&q[1], &q[0], "diffusion narrow y"), (&q[0], &e[0], "diffusion down"), (&e[0], &e[1], "diffusion wide x"), (&e[1], &e[0], "diffusion wide y")];
    for (k, (from, to, label)) in steps.into_iter().enumerate() {
        let g = scene::tex_group(device, &pipes.mod_layout, &[from, from], &pipes.sampler, &sp.light_bufs[k]);
        fullscreen_pass(encoder, label, to, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.glow_blur, &g);
    }
    let g = scene::tex_group(device, &pipes.mod_layout, &[&q[0], &e[0]], &pipes.sampler, &sp.light_bufs[6]);
    fullscreen_pass(encoder, "diffusion", &sp.out, wgpu::LoadOp::Load, &pipes.light, &g);
}

/// A layer's own view of the frame: its band's features and waveform (when
/// it filters), after slewing its tint and moving its palette.
struct LayerInput {
    features: Option<FeatureFrame>,
    points: Option<Vec<[f32; 2]>>,
}

impl LayerInput {
    /// The frame as the layer's visual sees it.
    fn ctx<'a>(&'a self, fr: &'a Frame, slot: &'a Slot, layer: &Layer) -> FrameCtx<'a> {
        let pal_active = layer.palette.active();
        FrameCtx {
            features: self.features.as_ref().unwrap_or(fr.features),
            tint: slot.tint,
            dt: fr.dt,
            time: fr.time,
            sample_rate: fr.sample_rate,
            width: fr.width,
            height: fr.height,
            points: self.points.as_deref().unwrap_or(fr.points),
            auto_gain: fr.auto_gain,
            art: fr.art_image,
            art_id: fr.art_id,
            photos_folder: fr.photos_folder,
            palette: pal_active.then_some(&slot.palette.table),
            palette_shift: slot.palette.shift,
            palette_amount: if pal_active { layer.palette.amount } else { 0.0 },
        }
    }
}

/// Filter a layer's audio, slew its tint and shift its palette for this frame.
fn layer_input(fr: &Frame, slot: &mut Slot, layer: &Layer) -> LayerInput {
    let dt = fr.dt;
    let band = layer.band;
    let features = band.enabled.then(|| band_features(fr.features, band.lo_hz, band.hi_hz));
    let points = (band.enabled && crate::visuals::get(&layer.visual).is_some_and(|d| d.band_waveform)).then(|| {
        slot.filter.retune(band.lo_hz, band.hi_hz, fr.sample_rate as f32);
        slot.filter.process(fr.points)
    });
    let feats = features.as_ref().unwrap_or(fr.features);
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
    LayerInput { features, points }
}

/// Take a picture down to 1/8 for its meter, with `glow` added (the box
/// average is linear, so that is the picture with the glow in it).
#[allow(clippy::too_many_arguments)]
fn meter_down(device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, pipes: &Pipes, m: &Meter, buf: &wgpu::Buffer, out: &wgpu::TextureView, glow: Option<&wgpu::TextureView>) {
    let g = scene::tex_group(device, &pipes.layer_layout, &[out, &m.state[m.cur]], &pipes.sampler, buf);
    fullscreen_pass(encoder, "layer down", &m.down, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.layer_pipelines.down, &g);
    if let Some(glow) = glow {
        let g = scene::tex_group(device, &pipes.layer_layout, &[glow, &m.state[m.cur]], &pipes.sampler, buf);
        fullscreen_pass(encoder, "layer down glow", &m.down, wgpu::LoadOp::Load, &pipes.layer_pipelines.down_add, &g);
    }
}

/// Meter a picture (when levelling is on; `down` when `meter_down` already
/// ran) and blend it onto the composite by the uniform already in `buf`.
#[allow(clippy::too_many_arguments)]
fn meter_and_blend(device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, pipes: &Pipes, level: bool, down: bool, m: &mut Meter, buf: &wgpu::Buffer, out: &wgpu::TextureView, dst_read: &wgpu::TextureView, dst_write: &wgpu::TextureView) {
    let (prev_s, cur_s) = (m.cur, 1 - m.cur);
    if level {
        if !down {
            meter_down(device, encoder, pipes, m, buf, out, None);
        }
        let g = scene::tex_group(device, &pipes.layer_layout, &[&m.down, &m.state[prev_s]], &pipes.sampler, buf);
        fullscreen_pass(encoder, "layer meter", &m.state[cur_s], wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.layer_pipelines.meter, &g);
    }
    let group = scene::tex_group(device, &pipes.blend_layout, &[out, &m.state[cur_s], dst_read], &pipes.sampler, buf);
    fullscreen_pass(encoder, "layer", dst_write, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.layer_pipelines.blend, &group);
    if level {
        m.cur = cur_s;
    }
}

/// One layer's frame: filter its audio, slew its tint, shift its palette,
/// render the visual, run the modifier chain, meter it, and blend it onto
/// the composite at `level` opacity.
#[allow(clippy::too_many_arguments)]
fn render_layer(device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, pipes: &Pipes, fr: &Frame, visual: &mut dyn Scene, slot: &mut Slot, layer: &Layer, level: f32, first: bool, dst_read: &wgpu::TextureView, dst_write: &wgpu::TextureView, timer: &mut GpuTimer) {
    let (dt, time) = (fr.dt, fr.time);
    let si = slot.chain.index as u8;
    let input = layer_input(fr, slot, layer);
    timer.mark(encoder, Part::Visual(si));
    visual.render(queue, encoder, &input.ctx(fr, slot, layer), layer);
    let feats: &FeatureFrame = input.features.as_ref().unwrap_or(fr.features);
    let (pal_table, pal_shift) = (&slot.palette.table, slot.palette.shift);
    queue.write_buffer(&slot.buf, 0, bytemuck::bytes_of(&LayerParams { opacity: level * layer.brightness.exp2(), dt, flags: fr.level as u32 | if first { 2 } else { 0 } | if visual.level_bright() { 4 } else { 0 }, blend: layer.blend as u32 }));

    // Effects director: which pool effects are live, and a gate per slot
    // that slews toward live or resting; pinned effects stay at 1.
    {
        let fx = &mut slot.fx;
        let dir = &layer.director;
        let pool: Vec<usize> = layer.modifiers.iter().enumerate().take(MAX_MODS).filter(|(_, m)| m.enabled() && !m.pinned()).map(|(j, _)| j).collect();
        if dir.mode == DirectorMode::Auto && !pool.is_empty() {
            let beat = feats.beat_pulse * feats.beat_confidence;
            let hit = beat > fx.last_beat + 0.4;
            fx.last_beat = beat;
            let mut roll = !fx.started || fr.scene_changed;
            if hit {
                fx.beats = fx.beats.wrapping_add(1);
                if fx.beats % dir.every.max(1) == 0 {
                    roll = true;
                }
            }
            if roll {
                let first = !fx.started;
                fx.started = true;
                fx.rolls = fx.rolls.wrapping_add(1);
                // Seeded by the track and the roll, so a song replays the same way.
                let mut rng = Rng::seeded(&[fr.art_id, slot.chain.index as u64, fx.rolls as u64]);
                if first || rng.next() >= dir.rest {
                    let mut rest: Vec<usize> = pool.clone();
                    fx.live = [false; MAX_MODS];
                    for _ in 0..(dir.live.max(1) as usize).min(rest.len()) {
                        let k = rng.below(rest.len());
                        fx.live[rest.swap_remove(k)] = true;
                    }
                }
            }
            let k = 1.0 - (-dt / dir.fade_s.max(0.02)).exp();
            for j in 0..MAX_MODS {
                let want = if pool.contains(&j) && !fx.live[j] { 0.0 } else { 1.0 };
                fx.gate[j] += (want - fx.gate[j]) * k;
            }
        } else {
            fx.started = false;
            let k = 1.0 - (-dt / 0.3).exp();
            for g in &mut fx.gate {
                *g += (1.0 - *g) * k;
            }
        }
    }

    // Modifier chain: advance every slot's state and write its uniform
    // first (mutable), then run the passes (immutable borrows only).
    let mut passes: Vec<(usize, usize, f32)> = Vec::new(); // (slot, pipeline index, wet/dry)
    {
        let gates = slot.fx.gate;
        let chain = &mut slot.chain;
        for (j, m) in layer.modifiers.iter().enumerate().take(MAX_MODS) {
            // A resting effect is skipped once its gate has closed.
            if !m.enabled() || gates[j] < 0.002 {
                chain.want_history(device, j, false, fr.width, fr.height);
                chain.want_pyramid(device, j, false, fr.width, fr.height);
                chain.want_glow(device, j, false, fr.width, fr.height);
                continue;
            }
            chain.want_history(device, j, uses_history(mod_index(m)), fr.width, fr.height);
            chain.want_pyramid(device, j, uses_pyramid(m), fr.width, fr.height);
            chain.want_glow(device, j, mod_index(m) == MOD_GLOW, fr.width, fr.height);
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
            if let (Modifier::Glow(k), Some(g)) = (m, chain.glow[j].as_ref()) {
                // Blur width in quarter-size texels: size 1 is a sigma of 0.7% of the height per side, ~40 px at 1440p full size.
                let sigma = (k.size.max(0.05) * 0.007 * fr.height as f32).max(0.5);
                let sub = |mode: f32, dir: [f32; 2]| ModParams { p0: [mode, dir[0], dir[1], sigma], p1: [k.threshold, fr.level as u32 as f32, 0.0, 0.0], ..p };
                queue.write_buffer(&g.bufs[0], 0, bytemuck::bytes_of(&sub(0.0, [0.0, 0.0])));
                queue.write_buffer(&g.bufs[1], 0, bytemuck::bytes_of(&sub(1.0, [1.0, 0.0])));
                queue.write_buffer(&g.bufs[2], 0, bytemuck::bytes_of(&sub(1.0, [0.0, 1.0])));
            }
            if let Modifier::Colorify(_) = m {
                p.p0[3] = fr.level as u32 as f32;
            }
            if matches!(m, Modifier::Grade(_) | Modifier::Edge(_) | Modifier::Halftone(_) | Modifier::Colorify(_)) {
                // All four read the palette table; the shift goes where each shader documents.
                match m {
                    Modifier::Grade(_) => p.p0[2] = pal_shift,
                    Modifier::Edge(_) => p.p0[3] = pal_shift,
                    Modifier::Colorify(_) => p.p1[3] = pal_shift,
                    _ => p.p2[0] = pal_shift,
                }
                queue.write_buffer(&chain.bufs[j], 0, bytemuck::bytes_of(&GradeParams { m: p, pal: *pal_table }));
            } else {
                queue.write_buffer(&chain.bufs[j], 0, bytemuck::bytes_of(&p));
            }
            let mix = (m.mix() * gates[j]).clamp(0.0, 1.0);
            if mix < 0.999 {
                queue.write_buffer(&chain.mix_bufs[j], 0, bytemuck::bytes_of(&MixParams { t: mix, _pad: [0.0; 3] }));
            }
            passes.push((j, mod_index(m), mix));
        }
    }
    let chain = &slot.chain;
    // Last frame's levelling gain, for the passes that measure brightness.
    let meter_state = &slot.meter.state[slot.meter.cur];
    let mut src: &wgpu::TextureView = visual.output();
    let mut pp = 0usize;
    let mut flips: Vec<usize> = Vec::new();
    for (j, pi, mix) in passes {
        timer.mark(encoder, Part::Effect(si, j as u8));
        let input = src;
        // Each history user has its own pair, so two in one chain never
        // read the texture the other is writing.
        let hist = chain.hist[j].as_ref();
        let (hist_prev, hist_cur) = (chain.hist_cur[j], 1 - chain.hist_cur[j]);
        let mut second: &wgpu::TextureView = hist.map_or(src, |h| &h[hist_prev]);
        if pi == MOD_COLORIFY {
            second = meter_state;
        }
        // Glow: threshold into the quarter-size pair, blur across then down;
        // the composite reads the result as its second texture.
        if let (MOD_GLOW, Some(gt)) = (pi, chain.glow[j].as_ref()) {
            let g = scene::tex_group(device, &pipes.mod_layout, &[src, meter_state], &pipes.sampler, &gt.bufs[0]);
            fullscreen_pass(encoder, "glow prefilter", &gt.a, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.glow_blur, &g);
            let g = scene::tex_group(device, &pipes.mod_layout, &[&gt.a, &gt.a], &pipes.sampler, &gt.bufs[1]);
            fullscreen_pass(encoder, "glow blur x", &gt.b, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.glow_blur, &g);
            let g = scene::tex_group(device, &pipes.mod_layout, &[&gt.b, &gt.b], &pipes.sampler, &gt.bufs[2]);
            fullscreen_pass(encoder, "glow blur y", &gt.a, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &pipes.glow_blur, &g);
            second = &gt.a;
        }
        // A fractal reads a mip pyramid of its input: copy, then halve level
        // by level (the blit's one linear tap at half size is a 2x2 box).
        let mut first_tex = src;
        if let (MOD_FRACTAL | MOD_FOLD, Some(py)) = (pi, chain.pyramid[j].as_ref()) {
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
    timer.mark(encoder, Part::Blend(si));
    meter_and_blend(device, encoder, pipes, fr.level, false, &mut slot.meter, &slot.buf, out, dst_read, dst_write);
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
            down_add: scene::pipeline(device, "layer down added", layout, &d, HDR_FORMAT, Some(scene::ADDITIVE)),
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

fn build_glow_blur(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "glow_blur.wgsl");
        scene::pipeline(device, "glow blur", layout, &m, HDR_FORMAT, None)
    })
}

fn build_light(device: &wgpu::Device, shaders: &ShaderSet, layout: &wgpu::BindGroupLayout) -> Result<wgpu::RenderPipeline, String> {
    scene::guarded(device, || {
        let m = scene::shader(device, shaders, "visual_light.wgsl");
        scene::pipeline(device, "diffusion", layout, &m, HDR_FORMAT, Some(scene::ADDITIVE))
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
