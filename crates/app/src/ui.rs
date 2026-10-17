//! egui debug / settings overlay. Toggle with Tab.

use audioviz_analysis::FeatureFrame;
use audioviz_audio::CaptureMode;
use serde::{Deserialize, Serialize};

use crate::scene::Scene;

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

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
    // Audio
    pub audio_delay_ms: f32,
    pub target_process: String,
    // Display
    /// 0 = uncapped (VSync only). Ambient use: 60.
    pub fps_cap: u32,
    /// Index into the monitor list for fullscreen.
    pub monitor: usize,
    pub start_fullscreen: bool,
    pub show_ui: bool,
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
            audio_delay_ms: 0.0,
            target_process: "Spotify.exe".into(),
            fps_cap: 60,
            monitor: 0,
            start_fullscreen: false,
            // Hidden on first run; a hint says "Tab for settings".
            show_ui: false,
        }
    }
}

/// Read-only view of the tap-along calibration for the panel.
#[derive(Clone, Default)]
pub struct CalibrationView {
    pub taps: usize,
    pub matched: usize,
    /// Median (tap - onset) in ms; None until enough taps matched.
    pub offset_ms: Option<f32>,
    /// Median absolute deviation of the matches, ms.
    pub spread_ms: f32,
}

pub const CALIBRATION_MIN_TAPS: usize = 6;

pub struct Stats {
    pub fps: f32,
    pub points_per_sec: f32,
    pub capture: CaptureMode,
    pub shader_error: Option<String>,
    pub sample_rate: u32,
    pub monitors: Vec<String>,
    pub fullscreen: bool,
    pub settings_path: String,
    pub scene_names: Vec<&'static str>,
    pub scene_current: usize,
    pub scene_fading: bool,
    pub last_cue: &'static str,
    pub now_playing: Option<String>,
    pub calibration: Option<CalibrationView>,
}

#[derive(Default)]
pub struct Actions {
    pub toggle_fullscreen: bool,
    pub switch_scene: Option<usize>,
    pub calibrate_toggle: bool,
    pub calibrate_apply: bool,
}

pub fn draw(ctx: &egui::Context, settings: &mut Settings, f: &FeatureFrame, stats: &Stats, scene: &mut dyn Scene) -> Actions {
    let mut actions = Actions::default();
    egui::Window::new(format!("audioviz {}", crate::VERSION))
        .default_width(320.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!("{:.0} fps · {:.0}k pts/s · {} Hz · t={:.0}s", stats.fps, stats.points_per_sec / 1000.0, stats.sample_rate, f.time_s));
            let cap = match &stats.capture {
                CaptureMode::Starting => "capture: starting".to_string(),
                CaptureMode::Process { name, pid } => format!("capture: {name} (pid {pid})"),
                CaptureMode::Endpoint { device } => format!("capture: endpoint '{device}'"),
                CaptureMode::Failed(e) => format!("capture FAILED: {e}"),
            };
            ui.label(cap);
            if let Some(err) = &stats.shader_error {
                ui.colored_label(egui::Color32::from_rgb(255, 120, 120), format!("shader error:\n{err}"));
            }

            egui::CollapsingHeader::new("Scene").default_open(true).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, name) in stats.scene_names.iter().enumerate() {
                        let selected = i == stats.scene_current;
                        if ui.selectable_label(selected, format!("{} {}", i + 1, name)).clicked() && !selected {
                            actions.switch_scene = Some(i);
                        }
                    }
                    if stats.scene_fading {
                        ui.small("fading…");
                    }
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut settings.auto_scenes, "auto (track change, drops, quiet, dwell)");
                    ui.small(format!("last cue: {}", stats.last_cue));
                });
                if let Some(np) = &stats.now_playing {
                    ui.small(np);
                }
                ui.separator();
                scene.ui(ui, settings);
            });

            let scene_name = stats.scene_names.get(stats.scene_current).copied().unwrap_or("scene");
            ui.collapsing(format!("Post ({scene_name})"), |ui| {
                let current = stats.scene_current;
                let p = settings.post_mut(current);
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut p.exposure, 0.1..=10.0).logarithmic(true).text("exposure"));
                    ui.checkbox(&mut p.auto_exposure, "auto");
                });
                ui.add(egui::Slider::new(&mut p.bloom, 0.0..=3.0).text("bloom"));
                ui.add(egui::Slider::new(&mut p.grain, 0.0..=1.0).text("grain"));
                ui.small("Each scene keeps its own post settings; they blend during cross-fades.");
                ui.horizontal(|ui| {
                    if ui.button("Reset this scene").clicked() {
                        settings.reset_post(current);
                    }
                    if ui.button("Copy to all scenes").clicked() {
                        let p = settings.post_for(current);
                        for i in 0..SCENE_COUNT.max(settings.post.len()) {
                            *settings.post_mut(i) = p;
                        }
                    }
                });
            });

            ui.collapsing("Display", |ui| {
                ui.add(egui::Slider::new(&mut settings.fps_cap, 0..=240).text("fps cap (0 = vsync)"));
                let current = stats.monitors.get(settings.monitor).cloned().unwrap_or_else(|| "(none)".into());
                egui::ComboBox::from_label("monitor").selected_text(current).show_ui(ui, |ui| {
                    for (i, name) in stats.monitors.iter().enumerate() {
                        ui.selectable_value(&mut settings.monitor, i, name);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button(if stats.fullscreen { "Exit fullscreen (F11)" } else { "Fullscreen (F11)" }).clicked() {
                        actions.toggle_fullscreen = true;
                    }
                    ui.checkbox(&mut settings.start_fullscreen, "start fullscreen");
                });
                ui.small(format!("settings: {}", stats.settings_path));
            });

            egui::CollapsingHeader::new("Sync").default_open(stats.calibration.is_some()).show(ui, |ui| {
                ui.add(egui::Slider::new(&mut settings.audio_delay_ms, 0.0..=300.0).text("audio delay ms"));
                ui.small("Loopback sees audio before you hear it. Raise this until the beam lands on the beat, or calibrate by tapping.");
                match &stats.calibration {
                    None => {
                        if ui.button("Calibrate by tapping (C)").clicked() {
                            actions.calibrate_toggle = true;
                        }
                    }
                    Some(c) => {
                        ui.colored_label(egui::Color32::from_rgb(255, 220, 90), "Tap Space on each beat you hear (10 or so).");
                        ui.label(calibration_status(c));
                        ui.horizontal(|ui| {
                            let ready = c.offset_ms.is_some();
                            if ui.add_enabled(ready, egui::Button::new("Apply (Enter)")).clicked() {
                                actions.calibrate_apply = true;
                            }
                            if ui.button("Cancel (C)").clicked() {
                                actions.calibrate_toggle = true;
                            }
                        });
                    }
                }
            });

            egui::CollapsingHeader::new("Beat").default_open(true).show(ui, |ui| {
                ui.horizontal(|ui| {
                    light(ui, f.beat_pulse, egui::Color32::from_rgb(255, 220, 90));
                    ui.label(format!("{:.1} bpm · beat {}", f.bpm, f.beat_count));
                });
                meter(ui, "conf", f.beat_confidence);
                meter(ui, "phase", f.beat_phase);
                ui.horizontal(|ui| {
                    ui.label("onsets");
                    for (g, name) in ["low", "mid", "high", "all"].iter().enumerate() {
                        light(ui, f.onset_pulse[g], egui::Color32::from_rgb(120, 200, 255));
                        ui.small(*name);
                    }
                });
            });

            ui.collapsing("Level", |ui| {
                ui.label(format!("{:.1} LUFS-M · {:.1} LUFS-S{}{}", f.lufs_m, f.lufs_s, if f.quiet { " · quiet" } else { "" }, if f.silent { " · silent" } else { "" }));
                meter(ui, "rms", f.rms * 4.0);
                meter(ui, "bass", f.bass * 0.5);
                meter(ui, "mid", f.mid * 0.5);
                meter(ui, "treb", f.treb * 0.5);
                meter(ui, "width", f.stereo_width);
                meter(ui, "trend", f.energy_trend * 0.5 + 0.5);
                ui.label(format!("centroid {:.0} Hz · peak {:.2}", f.centroid_hz, f.peak));
                bars(ui, &f.bands, 40.0, egui::Color32::from_rgb(110, 220, 150), None);
            });

            ui.collapsing("Chroma", |ui| {
                ui.label(format!("dominant {}", NOTE_NAMES[f.key_class as usize % 12]));
                bars(ui, &f.chroma, 40.0, egui::Color32::from_rgb(230, 150, 255), Some(&NOTE_NAMES));
            });

            ui.separator();
            ui.small("Tab: panel · 1-3/Space: scene · A: auto scenes · T: title overlay · C: calibrate · F11: fullscreen · F5: reload shaders · Esc: quit");
        });
    actions
}

pub fn calibration_status(c: &CalibrationView) -> String {
    match c.offset_ms {
        Some(off) => format!("{} taps, {} matched · offset {:+.0} ms (±{:.0})", c.taps, c.matched, off, c.spread_ms),
        None => format!("{} taps, {} matched · need {} matches", c.taps, c.matched, CALIBRATION_MIN_TAPS),
    }
}

/// Unobtrusive one-liners in the bottom-left corner (first-run hint, capture
/// fallback, calibration progress while the panel is hidden).
pub fn draw_notices(ctx: &egui::Context, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::unique("notices"))
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(16.0, -16.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 140))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    for l in lines {
                        ui.label(egui::RichText::new(l).color(egui::Color32::from_rgb(200, 210, 205)).size(13.0));
                    }
                });
        });
}

fn meter(ui: &mut egui::Ui, label: &str, v: f32) {
    ui.horizontal(|ui| {
        ui.label(format!("{label:>5}"));
        ui.add(egui::ProgressBar::new(v.clamp(0.0, 1.0)).desired_width(200.0));
    });
}

fn light(ui: &mut egui::Ui, v: f32, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    let a = (v.clamp(0.0, 1.0) * 255.0) as u8;
    let c = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a.max(25));
    ui.painter().circle_filled(rect.center(), 6.0, c);
}

fn bars(ui: &mut egui::Ui, values: &[f32], height: f32, color: egui::Color32, labels: Option<&[&str; 12]>) {
    let n = values.len().max(1) as f32;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height + if labels.is_some() { 12.0 } else { 0.0 }), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let w = rect.width() / n;
    let bottom = rect.top() + height;
    for (i, b) in values.iter().enumerate() {
        let h = b.clamp(0.0, 1.0) * height;
        let x0 = rect.left() + i as f32 * w;
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x0 + 1.0, bottom - h), egui::pos2(x0 + w - 1.0, bottom)), 0.0, color);
        if let Some(l) = labels {
            painter.text(egui::pos2(x0 + w * 0.5, bottom + 6.0), egui::Align2::CENTER_CENTER, l[i % 12], egui::FontId::proportional(9.0), ui.visuals().text_color());
        }
    }
}
