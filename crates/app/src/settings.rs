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
}

impl VisualKind {
    pub const ALL: [VisualKind; 3] = [VisualKind::Scope, VisualKind::Ribbons, VisualKind::Starfield];
    pub fn name(self) -> &'static str {
        match self {
            VisualKind::Scope => "Phosphor scope",
            VisualKind::Ribbons => "Flow-field ribbons",
            VisualKind::Starfield => "Black hole",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            VisualKind::Scope => "scope",
            VisualKind::Ribbons => "ribbons",
            VisualKind::Starfield => "starfield",
        }
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
}

impl Default for StarfieldParams {
    fn default() -> Self {
        Self { mass: 1.0, density: 1.0, twinkle: 0.6, drift: 1.0, disk: 1.0, doppler: 1.0, tilt: 5.0 }
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub visual: VisualKind,
    pub enabled: bool,
    /// 0 = off, 1 = as rendered, up to 2 = boosted.
    pub opacity: f32,
    pub band: AudioBand,
    pub scope: ScopeParams,
    pub ribbons: RibbonsParams,
    pub starfield: StarfieldParams,
}

impl Default for Layer {
    fn default() -> Self {
        Self { visual: VisualKind::Scope, enabled: false, opacity: 1.0, band: AudioBand::default(), scope: ScopeParams::default(), ribbons: RibbonsParams::default(), starfield: StarfieldParams::default() }
    }
}

impl Layer {
    pub fn new(visual: VisualKind, enabled: bool, opacity: f32) -> Self {
        Self { visual, enabled, opacity, ..Self::default() }
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
    // ---- title overlay ----
    pub type_overlay: bool,
    pub type_scale: f32,
    /// Show what is playing: title overlay, toast, track identity on the strip.
    pub show_now_playing: bool,
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
        self.layers.get(i).copied().unwrap_or_default()
    }
    pub fn layer_mut(&mut self, i: usize) -> &mut Layer {
        while self.layers.len() <= i {
            self.layers.push(Layer::default());
        }
        &mut self.layers[i]
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
        self.type_overlay = other.type_overlay;
        self.type_scale = other.type_scale;
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
            type_overlay: true,
            type_scale: 1.0,
            show_now_playing: false,
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
