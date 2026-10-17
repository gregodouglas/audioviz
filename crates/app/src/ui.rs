//! egui debug / settings overlay. Toggle with Tab.

use audioviz_analysis::FeatureFrame;
use audioviz_audio::CaptureMode;

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
    pub audio_delay_ms: f32,
    pub target_process: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            gain: 0.85,
            auto_gain: true,
            rotation_deg: -45.0,
            beam_width: 1.4,
            beam_intensity: 1.0,
            persistence: 0.03,
            exposure: 1.0,
            tint: [0.40, 1.0, 0.55],
            audio_delay_ms: 0.0,
            target_process: "Spotify.exe".into(),
        }
    }
}

pub struct Stats {
    pub fps: f32,
    pub points_per_frame: usize,
    pub capture: CaptureMode,
    pub shader_error: Option<String>,
    pub sample_rate: u32,
}

pub fn draw(ctx: &egui::Context, settings: &mut Settings, features: &FeatureFrame, stats: &Stats) {
    egui::Window::new("audioviz")
        .default_width(300.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!("{:.0} fps · {} pts/frame · {} Hz", stats.fps, stats.points_per_frame, stats.sample_rate));
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
            ui.separator();

            ui.heading("Scope");
            ui.add(egui::Slider::new(&mut settings.gain, 0.05..=3.0).logarithmic(true).text("gain"));
            ui.checkbox(&mut settings.auto_gain, "auto gain");
            ui.add(egui::Slider::new(&mut settings.rotation_deg, -180.0..=180.0).text("rotation °"));
            ui.add(egui::Slider::new(&mut settings.beam_width, 0.3..=6.0).text("beam width px"));
            ui.add(egui::Slider::new(&mut settings.beam_intensity, 0.05..=20.0).logarithmic(true).text("beam intensity"));
            ui.add(egui::Slider::new(&mut settings.persistence, 0.0001..=0.9).logarithmic(true).text("persistence /s"));
            ui.add(egui::Slider::new(&mut settings.exposure, 0.1..=10.0).logarithmic(true).text("exposure"));
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
            ui.separator();

            ui.heading("Sync");
            ui.add(egui::Slider::new(&mut settings.audio_delay_ms, 0.0..=300.0).text("audio delay ms"));
            ui.small("Loopback sees audio before you hear it. Raise this until the beam lands on the beat.");
            ui.separator();

            ui.heading("Features");
            meter(ui, "rms", features.rms * 4.0);
            meter(ui, "bass", features.bass * 0.5);
            meter(ui, "mid", features.mid * 0.5);
            meter(ui, "treb", features.treb * 0.5);
            meter(ui, "width", features.stereo_width);
            ui.label(format!("centroid {:.0} Hz · peak {:.2} · {}", features.centroid_hz, features.peak, if features.silent { "silent" } else { "signal" }));
            let n = features.bands.len().max(1) as f32;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 48.0), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            let w = rect.width() / n;
            for (i, b) in features.bands.iter().enumerate() {
                let h = (b.clamp(0.0, 1.0)) * rect.height();
                let x0 = rect.left() + i as f32 * w;
                painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x0 + 1.0, rect.bottom() - h), egui::pos2(x0 + w - 1.0, rect.bottom())), 0.0, egui::Color32::from_rgb(110, 220, 150));
            }
            ui.separator();
            ui.small("Tab: toggle panel · F11: fullscreen · F5: reload shaders · Esc: quit");
        });
}

fn meter(ui: &mut egui::Ui, label: &str, v: f32) {
    ui.horizontal(|ui| {
        ui.label(format!("{label:>5}"));
        ui.add(egui::ProgressBar::new(v.clamp(0.0, 1.0)).desired_width(180.0));
    });
}
