//! User settings: everything the panel can change, persisted as JSON
//! (`config.rs`). New fields must have a serde default so old files load;
//! removed or renamed fields must be migrated in `config::load`, never dropped.

use serde::{Deserialize, Serialize};

/// Number of scenes the per-scene settings are sized for.
pub const SCENE_COUNT: usize = 3;

/// Post-stack settings, one set per scene (the starfield wants less bloom
/// than the scope). Blended during cross-fades.
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

impl PostSettings {
    pub fn default_for(scene: usize) -> Self {
        match scene {
            2 => Self { bloom: 0.12, ..Self::default() },
            _ => Self::default(),
        }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let l = |a: f32, b: f32| a + (b - a) * t;
        Self {
            exposure: l(self.exposure, other.exposure),
            auto_exposure: if t < 0.5 { self.auto_exposure } else { other.auto_exposure },
            bloom: l(self.bloom, other.bloom),
            grain: l(self.grain, other.grain),
        }
    }
}

/// Panel sections, in rail order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Section {
    Scene,
    Post,
    Sync,
    Display,
    Looks,
    Meters,
    About,
}

impl Section {
    pub const ALL: [Section; 7] = [Section::Scene, Section::Post, Section::Sync, Section::Display, Section::Looks, Section::Meters, Section::About];
    pub fn name(self) -> &'static str {
        match self {
            Section::Scene => "Scene",
            Section::Post => "Post",
            Section::Sync => "Sync",
            Section::Display => "Display",
            Section::Looks => "Looks",
            Section::Meters => "Meters",
            Section::About => "About",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Scope scene
    pub gain: f32,
    pub auto_gain: bool,
    pub rotation_deg: f32,
    pub beam_width: f32,
    pub beam_intensity: f32,
    /// Fraction of brightness remaining after one second.
    pub persistence: f32,
    pub tint: [f32; 3],
    /// How much the beat clock pulses the beam (0 = off).
    pub beat_pulse: f32,
    // Ribbons scene
    pub ribbons_speed: f32,
    pub ribbons_scale: f32,
    pub ribbons_curl: f32,
    pub ribbons_vortex: f32,
    pub ribbons_width: f32,
    pub ribbons_intensity: f32,
    pub ribbons_persistence: f32,
    pub ribbons_saturation: f32,
    pub ribbons_count: u32,
    // Starfield scene
    pub star_mass: f32,
    pub star_density: f32,
    pub star_grid: f32,
    pub star_twinkle: f32,
    pub star_drift: f32,
    pub type_overlay: bool,
    pub type_scale: f32,
    // Scenes
    pub scene: usize,
    pub auto_scenes: bool,
    // Post, indexed by scene.
    pub post: Vec<PostSettings>,
    /// Scene accent follows the album art's dominant colour.
    pub tint_from_art: bool,
    /// Accessibility: cap frame-to-frame brightness rises (photosensitivity).
    pub reduce_flashing: bool,
    /// Allowed fractional brightness rise per second when reducing flashing.
    pub flash_rise: f32,
    // Audio
    pub audio_delay_ms: f32,
    pub target_process: String,
    // Display
    /// 0 = uncapped (VSync only). Ambient use: 60.
    pub fps_cap: u32,
    /// Index into the monitor list for fullscreen.
    pub monitor: usize,
    pub start_fullscreen: bool,
    /// Drop to 30 fps when the window is hidden or the machine is on battery.
    pub low_power: bool,
    /// Bottom-left track-change toast in scenes without a title overlay.
    pub toast: bool,
    /// Hide the chrome bar and strip after a few seconds without mouse movement.
    pub chrome_autohide: bool,
    // Panel
    pub show_ui: bool,
    pub section: Section,
    /// Show the analysis meters section.
    pub developer: bool,
    /// Name of the active look, if any ("" = custom).
    pub look: String,
    /// The first-run sheet has been completed or skipped.
    pub onboarded: bool,
}

impl Settings {
    pub fn post_for(&self, scene: usize) -> PostSettings {
        self.post.get(scene).copied().unwrap_or_else(|| PostSettings::default_for(scene))
    }
    pub fn post_mut(&mut self, scene: usize) -> &mut PostSettings {
        while self.post.len() <= scene {
            let i = self.post.len();
            self.post.push(PostSettings::default_for(i));
        }
        &mut self.post[scene]
    }
    pub fn reset_post(&mut self, scene: usize) {
        *self.post_mut(scene) = PostSettings::default_for(scene);
    }
    pub fn reset_scope(&mut self) {
        let d = Settings::default();
        self.gain = d.gain;
        self.auto_gain = d.auto_gain;
        self.rotation_deg = d.rotation_deg;
        self.beam_width = d.beam_width;
        self.beam_intensity = d.beam_intensity;
        self.persistence = d.persistence;
        self.beat_pulse = d.beat_pulse;
    }
    pub fn reset_ribbons(&mut self) {
        let d = Settings::default();
        self.ribbons_speed = d.ribbons_speed;
        self.ribbons_scale = d.ribbons_scale;
        self.ribbons_curl = d.ribbons_curl;
        self.ribbons_vortex = d.ribbons_vortex;
        self.ribbons_width = d.ribbons_width;
        self.ribbons_intensity = d.ribbons_intensity;
        self.ribbons_persistence = d.ribbons_persistence;
        self.ribbons_saturation = d.ribbons_saturation;
        self.ribbons_count = d.ribbons_count;
    }
    pub fn reset_starfield(&mut self) {
        let d = Settings::default();
        self.star_mass = d.star_mass;
        self.star_density = d.star_density;
        self.star_grid = d.star_grid;
        self.star_twinkle = d.star_twinkle;
        self.star_drift = d.star_drift;
        self.type_scale = d.type_scale;
    }

    /// The fields a "look" captures: everything visual, nothing about this
    /// machine (monitor, capture target, delay, window state).
    pub fn apply_look(&mut self, look: &Settings) {
        let keep = self.clone();
        *self = look.clone();
        self.audio_delay_ms = keep.audio_delay_ms;
        self.target_process = keep.target_process;
        self.fps_cap = keep.fps_cap;
        self.monitor = keep.monitor;
        self.start_fullscreen = keep.start_fullscreen;
        self.low_power = keep.low_power;
        self.toast = keep.toast;
        self.chrome_autohide = keep.chrome_autohide;
        self.show_ui = keep.show_ui;
        self.section = keep.section;
        self.developer = keep.developer;
        self.onboarded = keep.onboarded;
        self.reduce_flashing = keep.reduce_flashing;
        self.flash_rise = keep.flash_rise;
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            gain: 0.85,
            auto_gain: true,
            rotation_deg: 45.0,
            beam_width: 1.2,
            beam_intensity: 1.0,
            persistence: 0.05,
            tint: [0.40, 1.0, 0.55],
            beat_pulse: 0.3,
            ribbons_speed: 0.3,
            ribbons_scale: 1.5,
            ribbons_curl: 1.0,
            ribbons_vortex: 1.0,
            ribbons_width: 1.0,
            ribbons_intensity: 1.0,
            ribbons_persistence: 0.3,
            ribbons_saturation: 0.45,
            ribbons_count: 12_000,
            star_mass: 1.0,
            star_density: 1.0,
            star_grid: 1.0,
            star_twinkle: 0.6,
            star_drift: 1.0,
            type_overlay: true,
            type_scale: 1.0,
            scene: 0,
            auto_scenes: false,
            post: (0..SCENE_COUNT).map(PostSettings::default_for).collect(),
            tint_from_art: false,
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
            section: Section::Scene,
            developer: false,
            look: String::new(),
            onboarded: false,
        }
    }
}
