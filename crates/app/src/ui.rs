//! egui debug / settings overlay. Toggle with Tab.

use audioviz_analysis::FeatureFrame;
use audioviz_audio::CaptureMode;

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

#[derive(Debug, Clone)]
pub struct Settings {
    pub gain: f32,
    pub auto_gain: bool,
    pub rotation_deg: f32,
    pub beam_width: f32,
    pub beam_intensity: f32,
    /// Fraction of brightness remaining after one second.
    pub persistence: f32,
    pub exposure: f32,
    pub tint: [f32; 3],
    /// How much the beat clock pulses the exposure (0 = off).
    pub beat_pulse: f32,
    pub audio_delay_ms: f32,
    pub target_process: String,
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
            exposure: 1.0,
            tint: [0.40, 1.0, 0.55],
            beat_pulse: 0.3,
            audio_delay_ms: 0.0,
            target_process: "Spotify.exe".into(),
        }
    }
}

pub struct Stats {
    pub fps: f32,
    pub points_per_sec: f32,
    pub capture: CaptureMode,
    pub shader_error: Option<String>,
    pub sample_rate: u32,
}

pub fn draw(ctx: &egui::Context, settings: &mut Settings, f: &FeatureFrame, stats: &Stats) {
    egui::Window::new("audioviz")
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

            ui.collapsing("Scope", |ui| {
                ui.add(egui::Slider::new(&mut settings.gain, 0.05..=3.0).logarithmic(true).text("gain"));
                ui.checkbox(&mut settings.auto_gain, "auto gain");
                ui.add(egui::Slider::new(&mut settings.rotation_deg, -180.0..=180.0).text("rotation °"));
                ui.add(egui::Slider::new(&mut settings.beam_width, 0.3..=6.0).text("beam width px"));
                ui.add(egui::Slider::new(&mut settings.beam_intensity, 0.05..=20.0).logarithmic(true).text("beam intensity"));
                ui.add(egui::Slider::new(&mut settings.persistence, 0.0001..=0.9).logarithmic(true).text("persistence /s"));
                ui.add(egui::Slider::new(&mut settings.exposure, 0.1..=10.0).logarithmic(true).text("exposure"));
                ui.add(egui::Slider::new(&mut settings.beat_pulse, 0.0..=1.0).text("beat pulse"));
                ui.horizontal(|ui| {
                    ui.label("tint");
                    ui.color_edit_button_rgb(&mut settings.tint);
                    if ui.button("P1 green").clicked() {
                        settings.tint = [0.40, 1.0, 0.55];
                    }
                    if ui.button("P4 white").clicked() {
                        settings.tint = [0.85, 0.9, 1.0];
                    }
                    if ui.button("amber").clicked() {
                        settings.tint = [1.0, 0.65, 0.2];
                    }
                });
            });

            ui.collapsing("Sync", |ui| {
                ui.add(egui::Slider::new(&mut settings.audio_delay_ms, 0.0..=300.0).text("audio delay ms"));
                ui.small("Loopback sees audio before you hear it. Raise this until the beam lands on the beat.");
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

            egui::CollapsingHeader::new("Level").default_open(true).show(ui, |ui| {
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

            egui::CollapsingHeader::new("Chroma").default_open(true).show(ui, |ui| {
                ui.label(format!("dominant {}", NOTE_NAMES[f.key_class as usize % 12]));
                bars(ui, &f.chroma, 40.0, egui::Color32::from_rgb(230, 150, 255), Some(&NOTE_NAMES));
            });

            ui.separator();
            ui.small("Tab: toggle panel · F11: fullscreen · F5: reload shaders · Esc: quit");
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
