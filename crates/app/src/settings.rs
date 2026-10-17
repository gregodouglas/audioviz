//! User settings: everything the panel can change, persisted as JSON
//! (`config.rs`). New fields must have a serde default so old files load;
//! removed or renamed fields must be migrated in `config::migrate`, never
//! dropped.
//!
//! Model: a **visual** is a renderer (scope, ribbons, starfield); a **layer**
//! is one instance of a visual with its own parameters, opacity and audio
//! band; a **scene** is the composition of up to `MAX_LAYERS` layers plus the
//! post settings, saved by name (`scene_store.rs`).

use serde::{Deserialize, Serialize};

pub const MAX_LAYERS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VisualKind {
    Scope,
    Ribbons,
    Starfield,
    Kaleido,
    Sigil,
}

impl VisualKind {
    pub const ALL: [VisualKind; 5] = [VisualKind::Scope, VisualKind::Ribbons, VisualKind::Starfield, VisualKind::Kaleido, VisualKind::Sigil];
    pub fn name(self) -> &'static str {
        match self {
            VisualKind::Scope => "Phosphor scope",
            VisualKind::Ribbons => "Flow-field ribbons",
            VisualKind::Starfield => "Black hole",
            VisualKind::Kaleido => "Kaleidoscope",
            VisualKind::Sigil => "Sigil",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            VisualKind::Scope => "scope",
            VisualKind::Ribbons => "ribbons",
            VisualKind::Starfield => "starfield",
            VisualKind::Kaleido => "kaleido",
            VisualKind::Sigil => "sigil",
        }
    }
}

/// Mirror geometry of a kaleidoscope fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FoldMode {
    /// Two mirrors: N-fold symmetry around the apex.
    Radial,
    /// Three mirrors at 60 degrees: a triangular tiling of one cell.
    Tiled,
    /// Rotation only, no mirror: spinning blades.
    Pinwheel,
}

/// Kaleidoscope fold modifier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fold {
    pub enabled: bool,
    pub mode: FoldMode,
    pub segments: u32,
    /// Rotation, radians per second at average energy.
    pub speed: f32,
    pub zoom: f32,
    /// Apex offset from the source's centre (image half-heights).
    pub offset: f32,
    /// How fast the apex wanders.
    pub wander: f32,
    /// 0 = fixed segments; N = step to a new count every N beats.
    pub step_beats: u32,
    /// How much energy, onsets and bass move it.
    pub react: f32,
}

impl Default for Fold {
    fn default() -> Self {
        Self { enabled: true, mode: FoldMode::Radial, segments: 8, speed: 0.15, zoom: 1.0, offset: 0.25, wander: 1.0, step_beats: 0, react: 1.0 }
    }
}

/// The pre-chain `fold` field (one build, 2026-10-17); read for migration only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LegacyFold {
    pub enabled: bool,
    pub mode: FoldMode,
    pub segments: u32,
    pub speed: f32,
    pub zoom: f32,
    pub react: f32,
}

impl Default for LegacyFold {
    fn default() -> Self {
        Self { enabled: false, mode: FoldMode::Radial, segments: 8, speed: 0.15, zoom: 1.0, react: 1.0 }
    }
}

/// Feedback / echo modifier: last frame's result zoomed, spun and faded under this one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Feedback {
    pub enabled: bool,
    pub decay: f32,
    /// Zoom rate per second (positive = outward).
    pub zoom: f32,
    /// Spin, radians per second.
    pub spin: f32,
    pub react: f32,
}

impl Default for Feedback {
    fn default() -> Self {
        Self { enabled: true, decay: 0.9, zoom: 0.25, spin: 0.1, react: 1.0 }
    }
}

/// Polar wrap modifier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Polar {
    pub enabled: bool,
    /// false = wrap (x becomes angle), true = unwrap (the inverse).
    pub unwrap: bool,
    pub radius: f32,
    pub spin: f32,
    pub react: f32,
}

impl Default for Polar {
    fn default() -> Self {
        Self { enabled: true, unwrap: false, radius: 1.0, spin: 0.1, react: 1.0 }
    }
}

/// Ripple modifier: rings on beats, plus a standing sine warp.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ripple {
    pub enabled: bool,
    pub amount: f32,
    pub wavelength: f32,
    pub speed: f32,
    pub sine: f32,
    pub react: f32,
}

impl Default for Ripple {
    fn default() -> Self {
        Self { enabled: true, amount: 0.04, wavelength: 0.15, speed: 0.8, sine: 0.0, react: 1.0 }
    }
}

/// Edge (Sobel outline) modifier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Edge {
    pub enabled: bool,
    pub strength: f32,
    /// Share of the original kept underneath.
    pub keep: f32,
    pub tinted: bool,
    pub react: f32,
}

impl Default for Edge {
    fn default() -> Self {
        Self { enabled: true, strength: 1.5, keep: 0.0, tinted: false, react: 1.0 }
    }
}

/// Chromatic split modifier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChromaSplit {
    pub enabled: bool,
    pub amount: f32,
    pub radial: bool,
    pub react: f32,
}

impl Default for ChromaSplit {
    fn default() -> Self {
        Self { enabled: true, amount: 0.012, radial: true, react: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaskShape {
    Circle,
    Band,
    Hex,
}

/// Mask modifier: keep the picture inside (or outside) a soft shape.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mask {
    pub enabled: bool,
    pub shape: MaskShape,
    pub radius: f32,
    pub soft: f32,
    pub invert: bool,
    pub react: f32,
}

impl Default for Mask {
    fn default() -> Self {
        Self { enabled: true, shape: MaskShape::Circle, radius: 0.7, soft: 0.1, invert: false, react: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FractalMode {
    /// Kaleidoscopic IFS: fold, rotate, scale, shift; the cell recurs at every scale.
    Kifs,
    /// z^2 + c warp: the picture pulled into Julia-set filaments.
    Julia,
}

/// Fractal modifier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fractal {
    pub enabled: bool,
    pub mode: FractalMode,
    pub iterations: u32,
    /// KIFS scale per step (>1 shrinks the cell each level).
    pub scale: f32,
    /// KIFS rotation per step, radians; Julia: speed of c's orbit.
    pub angle: f32,
    /// KIFS shift per step; Julia: radius of c's orbit.
    pub offset: f32,
    pub zoom: f32,
    /// Weight of each deeper level relative to the one above.
    pub falloff: f32,
    pub react: f32,
}

impl Default for Fractal {
    fn default() -> Self {
        Self { enabled: true, mode: FractalMode::Kifs, iterations: 4, scale: 1.9, angle: 0.35, offset: 0.8, zoom: 1.0, falloff: 0.8, react: 1.0 }
    }
}

/// Luminescence modifier: fresh glow, hue-shifted afterglow, UV saturation, sparkle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Luminescence {
    pub enabled: bool,
    /// Fast pool release, seconds.
    pub fresh_s: f32,
    /// Slow pool release, seconds (the afterglow).
    pub afterglow_s: f32,
    pub fresh_gain: f32,
    pub afterglow_gain: f32,
    /// Afterglow hue, degrees away from the layer tint.
    pub hue_shift: f32,
    /// Extra excitation on onsets.
    pub excite: f32,
    /// Fluorescent saturation of the excited picture.
    pub uv: f32,
    pub sparkle: f32,
    pub react: f32,
}

impl Default for Luminescence {
    fn default() -> Self {
        Self { enabled: true, fresh_s: 0.35, afterglow_s: 4.0, fresh_gain: 0.6, afterglow_gain: 0.5, hue_shift: -140.0, excite: 1.5, uv: 0.6, sparkle: 1.0, react: 1.0 }
    }
}

/// Palette grade modifier: recolour the picture through the layer's palette.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    pub enabled: bool,
    /// false = by brightness (gradient map), true = by the picture's hue.
    pub by_hue: bool,
    pub amount: f32,
    pub react: f32,
}

impl Default for Grade {
    fn default() -> Self {
        Self { enabled: true, by_hue: false, amount: 1.0, react: 1.0 }
    }
}

/// A layer's palette: a preset by name, or its own stops.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaletteSel {
    /// "none" = the layer colours itself as before; a preset name; or "custom".
    pub name: String,
    pub custom: Vec<crate::color::Stop>,
    /// How far visuals lean on the palette instead of their own colouring.
    pub amount: f32,
    /// Cycles per second the palette shifts along itself (0 = still).
    pub cycle: f32,
    pub cyclic: bool,
    pub react: f32,
}

impl Default for PaletteSel {
    fn default() -> Self {
        Self { name: "none".into(), custom: crate::color::default_custom(), amount: 1.0, cycle: 0.0, cyclic: true, react: 1.0 }
    }
}

impl PaletteSel {
    pub fn active(&self) -> bool {
        self.name != "none"
    }
    /// The stops this selection means (a preset's, or the custom list).
    pub fn stops(&self) -> Vec<crate::color::Stop> {
        if self.name == "custom" {
            self.custom.clone()
        } else {
            crate::color::preset(&self.name).map(|p| p.stops.to_vec()).unwrap_or_else(|| crate::color::preset("phosphor").unwrap().stops.to_vec())
        }
    }
}

/// A modifier: one full-screen pass over a layer's picture before the mix.
/// A layer runs an ordered chain of up to `MAX_MODS`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Modifier {
    Fold(Fold),
    Feedback(Feedback),
    Polar(Polar),
    Ripple(Ripple),
    Edge(Edge),
    Chroma(ChromaSplit),
    Mask(Mask),
    Fractal(Fractal),
    Lumi(Luminescence),
    Grade(Grade),
}

pub const MAX_MODS: usize = 6;

impl Modifier {
    /// Every kind with a constructor, in the order the panel offers them.
    pub const KINDS: [(&'static str, fn() -> Modifier); 10] = [
        ("palette", || Modifier::Grade(Grade::default())),
        ("kaleidoscope", || Modifier::Fold(Fold::default())),
        ("fractal", || Modifier::Fractal(Fractal::default())),
        ("luminescence", || Modifier::Lumi(Luminescence::default())),
        ("feedback", || Modifier::Feedback(Feedback::default())),
        ("polar", || Modifier::Polar(Polar::default())),
        ("ripple", || Modifier::Ripple(Ripple::default())),
        ("edge", || Modifier::Edge(Edge::default())),
        ("chroma split", || Modifier::Chroma(ChromaSplit::default())),
        ("mask", || Modifier::Mask(Mask::default())),
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Modifier::Fold(_) => "kaleidoscope",
            Modifier::Feedback(_) => "feedback",
            Modifier::Polar(_) => "polar",
            Modifier::Ripple(_) => "ripple",
            Modifier::Edge(_) => "edge",
            Modifier::Chroma(_) => "chroma split",
            Modifier::Mask(_) => "mask",
            Modifier::Fractal(_) => "fractal",
            Modifier::Lumi(_) => "luminescence",
            Modifier::Grade(_) => "palette",
        }
    }

    pub fn enabled(&self) -> bool {
        *self.enabled_ref()
    }

    fn enabled_ref(&self) -> &bool {
        match self {
            Modifier::Fold(m) => &m.enabled,
            Modifier::Feedback(m) => &m.enabled,
            Modifier::Polar(m) => &m.enabled,
            Modifier::Ripple(m) => &m.enabled,
            Modifier::Edge(m) => &m.enabled,
            Modifier::Chroma(m) => &m.enabled,
            Modifier::Mask(m) => &m.enabled,
            Modifier::Fractal(m) => &m.enabled,
            Modifier::Lumi(m) => &m.enabled,
            Modifier::Grade(m) => &m.enabled,
        }
    }

    pub fn enabled_mut(&mut self) -> &mut bool {
        match self {
            Modifier::Fold(m) => &mut m.enabled,
            Modifier::Feedback(m) => &mut m.enabled,
            Modifier::Polar(m) => &mut m.enabled,
            Modifier::Ripple(m) => &mut m.enabled,
            Modifier::Edge(m) => &mut m.enabled,
            Modifier::Chroma(m) => &mut m.enabled,
            Modifier::Mask(m) => &mut m.enabled,
            Modifier::Fractal(m) => &mut m.enabled,
            Modifier::Lumi(m) => &mut m.enabled,
            Modifier::Grade(m) => &mut m.enabled,
        }
    }
}

/// Where the kaleidoscope's object cell takes its picture from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageSource {
    Art,
    Photos,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KaleidoParams {
    pub source: ImageSource,
    /// Photos: seconds each picture stays before the crossfade.
    pub dwell_s: f32,
    /// Fraction of the picture's height the window shows.
    pub zoom: f32,
    pub drift: f32,
    /// Spin kick on onsets.
    pub tumble: f32,
    /// 0 = the picture's own colours, 1 = fully pulled to the layer tint.
    pub tint_mix: f32,
}

impl Default for KaleidoParams {
    fn default() -> Self {
        Self { source: ImageSource::Art, dwell_s: 15.0, zoom: 0.6, drift: 1.0, tumble: 1.0, tint_mix: 0.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScopeParams {
    pub gain: f32,
    pub auto_gain: bool,
    pub rotation_deg: f32,
    pub beam_width: f32,
    pub beam_intensity: f32,
    /// Fraction of brightness remaining after one second.
    pub persistence: f32,
    /// How much the beat clock pulses the beam (0 = off).
    pub beat_pulse: f32,
}

impl Default for ScopeParams {
    fn default() -> Self {
        Self { gain: 0.85, auto_gain: true, rotation_deg: 45.0, beam_width: 1.2, beam_intensity: 1.0, persistence: 0.05, beat_pulse: 0.3 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RibbonsParams {
    pub speed: f32,
    pub scale: f32,
    pub curl: f32,
    pub vortex: f32,
    pub width: f32,
    pub intensity: f32,
    pub persistence: f32,
    pub saturation: f32,
    pub count: u32,
}

impl Default for RibbonsParams {
    fn default() -> Self {
        Self { speed: 0.3, scale: 1.5, curl: 1.0, vortex: 1.0, width: 1.0, intensity: 1.0, persistence: 0.3, saturation: 0.45, count: 12_000 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StarfieldParams {
    /// Lens mass: scales the shadow and the disk together.
    pub mass: f32,
    pub density: f32,
    pub twinkle: f32,
    pub drift: f32,
    /// Accretion disk brightness.
    pub disk: f32,
    /// Doppler beaming strength (approaching side brighter and bluer).
    pub doppler: f32,
    /// Camera elevation above the disk plane, degrees.
    pub tilt: f32,
    /// How much the hole's size breathes with the bass (0 = steady).
    pub breathe: f32,
    /// Nebula haze and the Milky Way band behind the stars.
    pub nebula: f32,
    /// Infall: tendrils at the inner edge and streams spiralling in from outside.
    pub streams: f32,
}

impl Default for StarfieldParams {
    fn default() -> Self {
        Self { mass: 1.0, density: 1.0, twinkle: 0.6, drift: 1.0, disk: 1.0, doppler: 1.0, tilt: 5.0, breathe: 0.0, nebula: 1.0, streams: 1.0 }
    }
}

/// Sigil: a neon sacred-geometry glyph (`scenes/sigil.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SigilParams {
    /// Glyph size as a fraction of the short side.
    pub scale: f32,
    /// Coloured halo around every tube.
    pub glow: f32,
    pub rings: f32,
    pub rays: f32,
    pub orbs: f32,
    /// The painterly ground and its splatter.
    pub ground: f32,
    /// Ray afterimage life, seconds.
    pub persistence: f32,
    /// Rotation of the glyph and the orbit.
    pub spin: f32,
    /// How much bands, onsets and beats move it.
    pub react: f32,
    /// Extinguish and relight a new arrangement on a track change.
    pub rebuild: bool,
}

impl Default for SigilParams {
    fn default() -> Self {
        Self { scale: 0.8, glow: 1.0, rings: 1.0, rays: 1.0, orbs: 1.0, ground: 1.0, persistence: 0.7, spin: 1.0, react: 1.0, rebuild: true }
    }
}

/// Where a layer takes its colour from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TintMode {
    /// The scene tint (which itself may follow the album art).
    Scene,
    /// The album art's dominant colour, whatever the scene tint does.
    Art,
    /// This layer's own colour.
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerTint {
    pub mode: TintMode,
    pub color: [f32; 3],
}

impl Default for LayerTint {
    fn default() -> Self {
        Self { mode: TintMode::Scene, color: [0.40, 1.0, 0.55] }
    }
}

/// Which part of the spectrum a layer reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioBand {
    /// false = the full mix (what every layer did before bands existed).
    pub enabled: bool,
    pub lo_hz: f32,
    pub hi_hz: f32,
}

impl Default for AudioBand {
    fn default() -> Self {
        Self { enabled: false, lo_hz: 20.0, hi_hz: 400.0 }
    }
}

impl AudioBand {
    /// Named presets: (label, lo, hi).
    pub const PRESETS: [(&'static str, f32, f32); 6] = [("sub", 20.0, 60.0), ("bass", 60.0, 250.0), ("low mid", 250.0, 500.0), ("mid", 500.0, 2000.0), ("presence", 2000.0, 4000.0), ("air", 4000.0, 16000.0)];
    pub fn label(&self) -> String {
        if !self.enabled {
            return "full mix".into();
        }
        for (name, lo, hi) in Self::PRESETS {
            if (self.lo_hz - lo).abs() < 1.0 && (self.hi_hz - hi).abs() < 1.0 {
                return name.into();
            }
        }
        format!("{:.0}-{:.0} Hz", self.lo_hz, self.hi_hz)
    }
}

/// One instance of a visual in the scene. All three parameter sets are kept
/// so switching the visual back and forth loses nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub visual: VisualKind,
    pub enabled: bool,
    /// 0 = off, 1 = as rendered, up to 2 = boosted.
    pub opacity: f32,
    pub band: AudioBand,
    pub tint: LayerTint,
    /// Colour palette the visual and the palette modifier draw from.
    pub palette: PaletteSel,
    /// Ordered chain of modifiers run over the visual's output.
    pub modifiers: Vec<Modifier>,
    /// Pre-chain field, migrated into `modifiers` on load; never written.
    #[serde(skip_serializing)]
    pub fold: LegacyFold,
    pub scope: ScopeParams,
    pub ribbons: RibbonsParams,
    pub starfield: StarfieldParams,
    pub kaleido: KaleidoParams,
    pub sigil: SigilParams,
}

impl Default for Layer {
    fn default() -> Self {
        Self { visual: VisualKind::Scope, enabled: false, opacity: 1.0, band: AudioBand::default(), tint: LayerTint::default(), palette: PaletteSel::default(), modifiers: Vec::new(), fold: LegacyFold::default(), scope: ScopeParams::default(), ribbons: RibbonsParams::default(), starfield: StarfieldParams::default(), kaleido: KaleidoParams::default(), sigil: SigilParams::default() }
    }
}

impl Layer {
    pub fn new(visual: VisualKind, enabled: bool, opacity: f32) -> Self {
        Self { visual, enabled, opacity, ..Self::default() }
    }

    /// Bring a loaded layer up to date: the one-build `fold` field becomes the
    /// first modifier; chains are capped.
    pub fn normalize(&mut self) {
        if self.fold.enabled {
            let f = self.fold;
            self.modifiers.insert(0, Modifier::Fold(Fold { enabled: true, mode: f.mode, segments: f.segments, speed: f.speed, zoom: f.zoom, react: f.react, ..Fold::default() }));
            self.fold.enabled = false;
        }
        self.modifiers.truncate(MAX_MODS);
    }

    pub fn has_fold(&self) -> bool {
        self.modifiers.iter().any(|m| matches!(m, Modifier::Fold(_)))
    }
}

/// Post-stack settings for the whole picture.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostSettings {
    pub exposure: f32,
    pub auto_exposure: bool,
    pub bloom: f32,
    pub grain: f32,
}

impl Default for PostSettings {
    fn default() -> Self {
        Self { exposure: 1.0, auto_exposure: true, bloom: 0.3, grain: 0.15 }
    }
}

/// Panel sections, in rail order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Section {
    Scenes,
    Layers,
    Post,
    Sync,
    Display,
    Meters,
    About,
}

impl Section {
    pub const ALL: [Section; 7] = [Section::Scenes, Section::Layers, Section::Post, Section::Sync, Section::Display, Section::Meters, Section::About];
    pub fn name(self) -> &'static str {
        match self {
            Section::Scenes => "Scenes",
            Section::Layers => "Layers",
            Section::Post => "Post",
            Section::Sync => "Sync",
            Section::Display => "Display",
            Section::Meters => "Meters",
            Section::About => "About",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // ---- the scene (saved by scene_store) ----
    pub layers: Vec<Layer>,
    /// Which layer the panel and keys 1-3 edit.
    pub editing: usize,
    /// Name of the active scene ("" = unsaved).
    pub scene_name: String,
    pub post: PostSettings,
    /// Base colour for every visual.
    pub tint: [f32; 3],
    /// Scene accent follows the album art's dominant colour.
    pub tint_from_art: bool,
    /// Show what is playing: toast and track identity on the strip.
    pub show_now_playing: bool,
    /// Folder of photos for the kaleidoscope's "my photos" picture source.
    pub photos_folder: String,
    // ---- director ----
    pub auto_scenes: bool,
    // ---- accessibility ----
    pub reduce_flashing: bool,
    pub flash_rise: f32,
    // ---- audio ----
    pub audio_delay_ms: f32,
    pub target_process: String,
    // ---- display ----
    pub fps_cap: u32,
    pub monitor: usize,
    pub start_fullscreen: bool,
    pub low_power: bool,
    pub toast: bool,
    pub chrome_autohide: bool,
    // ---- panel ----
    pub show_ui: bool,
    pub section: Section,
    pub developer: bool,
    pub onboarded: bool,
}

impl Settings {
    pub fn layer(&self, i: usize) -> Layer {
        self.layers.get(i).cloned().unwrap_or_default()
    }
    pub fn layer_mut(&mut self, i: usize) -> &mut Layer {
        while self.layers.len() <= i {
            self.layers.push(Layer::default());
        }
        &mut self.layers[i]
    }
    /// Post-load fix-ups for every layer (see `Layer::normalize`).
    pub fn normalize(&mut self) {
        for l in &mut self.layers {
            l.normalize();
        }
    }
    pub fn reset_post(&mut self) {
        self.post = PostSettings::default();
    }

    /// Take the scene part of `other` (layers, post, tint, overlay) and keep
    /// everything about this machine and window.
    pub fn apply_scene(&mut self, other: &Settings) {
        self.layers = other.layers.clone();
        self.editing = other.editing.min(MAX_LAYERS - 1);
        self.post = other.post;
        self.tint = other.tint;
        self.tint_from_art = other.tint_from_art;
        self.scene_name = other.scene_name.clone();
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            layers: vec![Layer::new(VisualKind::Scope, true, 1.0), Layer::new(VisualKind::Ribbons, false, 0.5), Layer::new(VisualKind::Starfield, false, 0.5)],
            editing: 0,
            scene_name: String::new(),
            post: PostSettings::default(),
            tint: [0.40, 1.0, 0.55],
            tint_from_art: false,
            show_now_playing: false,
            photos_folder: String::new(),
            auto_scenes: false,
            reduce_flashing: true,
            flash_rise: 2.0,
            audio_delay_ms: 0.0,
            target_process: "Spotify.exe".into(),
            fps_cap: 60,
            monitor: 0,
            start_fullscreen: false,
            low_power: true,
            toast: true,
            chrome_autohide: true,
            show_ui: false,
            section: Section::Scenes,
            developer: false,
            onboarded: false,
        }
    }
}
