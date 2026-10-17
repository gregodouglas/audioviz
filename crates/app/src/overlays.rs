//! Full-window overlays and small notices drawn over the scene: the
//! track-change toast, the empty state, the calibration sheet, the shortcut
//! sheet, the first-run sheet and the bottom-left notice lines.

use egui::{Align2, Color32, Rect, Sense, pos2, vec2};

use crate::panel::MonitorInfo;
use crate::theme::{self, ACCENT, BG, HAIRLINE, TEXT, TEXT_2, TEXT_3};
use crate::widgets;

/// Read-only view of the tap-along calibration.
#[derive(Clone, Default)]
pub struct CalibrationView {
    pub taps: usize,
    pub matched: usize,
    /// Per-tap outcome, oldest first: true = matched an onset.
    pub history: Vec<bool>,
    /// Median (tap - onset) in ms; None until enough taps matched.
    pub offset_ms: Option<f32>,
    /// Median absolute deviation of the matches, ms.
    pub spread_ms: f32,
}

pub const CALIBRATION_MIN_TAPS: usize = 6;

pub fn calibration_status(c: &CalibrationView) -> String {
    match c.offset_ms {
        Some(off) => format!("{} taps · {} matched · offset {:+.0} ms · ±{:.0} ms", c.taps, c.matched, off, c.spread_ms),
        None => format!("{} taps · {} matched · need {} matches", c.taps, c.matched, CALIBRATION_MIN_TAPS),
    }
}

fn dim(ctx: &egui::Context, alpha: u8) {
    let r = ctx.content_rect();
    ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("dim"))).rect_filled(r, 0.0, theme::with_alpha(BG, alpha));
}

fn centre_area(ctx: &egui::Context, id: &str, size: egui::Vec2) -> egui::Area {
    let r = ctx.content_rect();
    egui::Area::new(egui::Id::unique(id)).fixed_pos(pos2(r.center().x - size.x * 0.5, r.center().y - size.y * 0.5)).order(egui::Order::Foreground)
}

// ---- notices ---------------------------------------------------------------

/// Unobtrusive one-liners, bottom-left (capture failure, calibration result).
pub fn notices(ctx: &egui::Context, lines: &[String], bottom_offset: f32) {
    if lines.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::unique("notices"))
        .anchor(Align2::LEFT_BOTTOM, vec2(16.0, -16.0 - bottom_offset))
        .interactable(false)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new().fill(theme::with_alpha(BG, 190)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                for l in lines {
                    ui.label(egui::RichText::new(l).color(Color32::from_rgb(200, 210, 205)).size(13.0));
                }
            });
        });
}

// ---- toast -----------------------------------------------------------------

/// Track-change toast, bottom-left. `t` is the animated visibility.
pub fn toast(ctx: &egui::Context, art: Option<egui::TextureId>, title: &str, artist: &str, t: f32, bottom_offset: f32) {
    if t <= 0.001 || title.is_empty() {
        return;
    }
    let r = ctx.content_rect();
    let h = 46.0;
    let w = 300.0;
    let y = r.max.y - 24.0 - h - bottom_offset + (1.0 - t) * 12.0;
    let rect = Rect::from_min_size(pos2(r.min.x + 24.0, y), vec2(w, h));
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("toast")));
    let a = (t * 255.0) as u8;
    p.rect_filled(rect, 4.0, theme::with_alpha(BG, (190.0 * t) as u8));
    let art_r = Rect::from_min_size(pos2(rect.min.x + 8.0, rect.min.y + 8.0), vec2(30.0, 30.0));
    match art {
        Some(id) => {
            p.image(id, art_r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), theme::with_alpha(Color32::WHITE, a));
        }
        None => {
            p.rect_filled(art_r, 3.0, theme::with_alpha(HAIRLINE, a));
        }
    }
    p.text(pos2(rect.min.x + 48.0, rect.min.y + 15.0), Align2::LEFT_CENTER, title, theme::bold(13.0), theme::with_alpha(TEXT, a));
    p.text(pos2(rect.min.x + 48.0, rect.min.y + 32.0), Align2::LEFT_CENTER, artist, theme::sans(11.0), theme::with_alpha(TEXT_2, a));
}

// ---- empty state -------------------------------------------------------------

#[derive(Default)]
pub struct EmptyActions {
    pub open_spotify: bool,
    pub pick_app: bool,
}

/// Shown when the capture target (Spotify) is not running.
pub fn empty_state(ctx: &egui::Context, target: &str, device: &str, spotify_installed: bool) -> EmptyActions {
    let mut a = EmptyActions::default();
    let size = vec2(520.0, 260.0);
    centre_area(ctx, "empty-state", size).interactable(true).show(ctx, |ui| {
        ui.set_min_size(size);
        ui.vertical_centered(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(90.0, 80.0), Sense::hover());
            widgets::mark(ui.painter(), r.center(), 34.0, TEXT_3, false);
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Nothing to listen to yet").font(theme::bold(19.0)).color(TEXT));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!("Start {} and play something. Until then this shows the default output:", target.trim_end_matches(".exe"))).color(TEXT_2));
            ui.label(egui::RichText::new(device).font(theme::mono(13.0)).color(TEXT_2));
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                let total = 120.0 + 8.0 + 130.0;
                ui.add_space((ui.available_width() - total) * 0.5);
                if spotify_installed && target.to_lowercase().contains("spotify") {
                    if ui.add_sized([120.0, 26.0], egui::Button::new(egui::RichText::new("Open Spotify").color(ACCENT)).fill(Color32::TRANSPARENT).stroke(egui::Stroke::new(1.0, ACCENT))).clicked() {
                        a.open_spotify = true;
                    }
                }
                if ui.add_sized([130.0, 26.0], egui::Button::new("Pick another app").fill(Color32::TRANSPARENT).stroke(egui::Stroke::new(1.0, HAIRLINE))).clicked() {
                    a.pick_app = true;
                }
            });
        });
    });
    a
}

// ---- calibration -------------------------------------------------------------

#[derive(Default)]
pub struct CalActions {
    pub apply: bool,
    pub cancel: bool,
}

pub fn calibration(ctx: &egui::Context, view: &CalibrationView, current_delay: f32, beat_pulse: f32) -> CalActions {
    let mut a = CalActions::default();
    dim(ctx, 180);
    let r = ctx.content_rect();
    let (cx, cy) = (r.center().x, r.min.y + r.height() * 0.44);
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("calib")));
    let pulse = 74.0 + 6.0 * beat_pulse;
    p.circle_stroke(pos2(cx, cy), pulse, egui::Stroke::new(18.0, theme::with_alpha(ACCENT, 45)));
    p.circle_stroke(pos2(cx, cy), 74.0, egui::Stroke::new(2.0, ACCENT));
    p.text(pos2(cx, cy), Align2::CENTER_CENTER, "Tap Space", theme::bold(22.0), TEXT);
    p.text(pos2(cx, cy + 118.0), Align2::CENTER_CENTER, "on each beat you hear. Ten is plenty.", theme::sans(15.0), TEXT_2);
    // Tap dots: last ten.
    let n = 10usize;
    let x0 = cx - (n as f32 - 1.0) * 14.0 * 0.5;
    let hist: Vec<bool> = view.history.iter().rev().take(n).rev().copied().collect();
    for i in 0..n {
        let color = match hist.get(i) {
            Some(true) => ACCENT,
            Some(false) => TEXT_2,
            None => HAIRLINE,
        };
        p.circle_filled(pos2(x0 + i as f32 * 14.0, cy + 150.0), 4.0, color);
    }
    p.text(pos2(cx, cy + 184.0), Align2::CENTER_CENTER, calibration_status(view), theme::mono(13.0), TEXT);
    let verdict = match view.offset_ms {
        Some(_) if view.spread_ms <= 15.0 => ("Looking good. Press Enter to apply, or keep tapping.", ACCENT),
        Some(_) => ("Taps are scattered. Try a track with a clear kick, or keep tapping.", theme::WARN),
        None => ("Keep tapping.", TEXT_3),
    };
    p.text(pos2(cx, cy + 208.0), Align2::CENTER_CENTER, verdict.0, theme::sans(12.0), verdict.1);
    p.text(pos2(cx, r.max.y - 40.0), Align2::CENTER_CENTER, format!("Enter apply · C cancel · current delay {current_delay:.0} ms"), theme::mono(11.0), TEXT_3);
    // Buttons for the mouse.
    let size = vec2(220.0, 30.0);
    egui::Area::new(egui::Id::unique("calib-buttons")).fixed_pos(pos2(cx - size.x * 0.5, cy + 228.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.add_enabled(view.offset_ms.is_some(), egui::Button::new(egui::RichText::new("Apply").color(ACCENT)).fill(Color32::TRANSPARENT).stroke(egui::Stroke::new(1.0, ACCENT))).clicked() {
                a.apply = true;
            }
            if widgets::ghost_button(ui, "Cancel", false).clicked() {
                a.cancel = true;
            }
        });
    });
    a
}

// ---- shortcuts ---------------------------------------------------------------

pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Tab", "settings panel"),
    ("H", "hide everything; H or Tab shows it again"),
    ("1 2 3", "scene (in layer mode: the layer to edit)"),
    ("Space", "next scene (tap while calibrating)"),
    ("A", "auto scene director"),
    ("N", "show what is playing"),
    ("T", "title overlay layout (starfield)"),
    ("C", "calibrate audio delay"),
    ("Enter", "apply calibration"),
    ("L", "next look"),
    ("F11", "fullscreen on this monitor"),
    ("F5", "reload shaders (dev)"),
    ("?", "this sheet"),
    ("Esc", "close sheet · close panel · leave fullscreen · quit"),
];

/// Returns true when the sheet should close.
pub fn shortcuts(ctx: &egui::Context) -> bool {
    dim(ctx, 160);
    let size = vec2(360.0, 24.0 * SHORTCUTS.len() as f32 + 70.0);
    let mut close = false;
    centre_area(ctx, "shortcuts", size).show(ctx, |ui| {
        egui::Frame::new().fill(theme::panel_fill()).stroke(egui::Stroke::new(1.0, HAIRLINE)).corner_radius(6.0).inner_margin(20.0).show(ui, |ui| {
            ui.set_min_width(size.x - 40.0);
            ui.label(egui::RichText::new("Shortcuts").font(theme::bold(15.0)));
            ui.add_space(8.0);
            for (k, d) in SHORTCUTS {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(70.0, 20.0), Sense::hover());
                    ui.painter().rect_filled(r, 3.0, theme::RAISED);
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, *k, theme::mono(11.0), TEXT);
                    ui.label(egui::RichText::new(*d).color(TEXT_2));
                });
            }
            ui.add_space(8.0);
            if widgets::ghost_button(ui, "Close", false).clicked() {
                close = true;
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    close
}

// ---- first run ---------------------------------------------------------------

#[derive(Default)]
pub struct FirstRun {
    pub step: usize,
}

#[derive(Default)]
pub struct FirstRunActions {
    pub monitor: Option<usize>,
    pub start_fullscreen: Option<bool>,
    pub calibrate: bool,
    pub scene: Option<usize>,
    pub done: bool,
}

pub fn first_run(ctx: &egui::Context, state: &mut FirstRun, monitors: &[MonitorInfo], selected_monitor: usize, start_fullscreen: bool, scene_names: &[&str], scene: usize) -> FirstRunActions {
    let mut a = FirstRunActions::default();
    dim(ctx, 170);
    let size = vec2(480.0, 300.0);
    centre_area(ctx, "first-run", size).show(ctx, |ui| {
        egui::Frame::new().fill(theme::panel_fill()).stroke(egui::Stroke::new(1.0, HAIRLINE)).corner_radius(6.0).inner_margin(24.0).show(ui, |ui| {
            ui.set_width(size.x - 48.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                widgets::mark(ui.painter(), r.center(), 9.0, ACCENT, true);
                ui.label(egui::RichText::new("Welcome to audioviz").font(theme::bold(17.0)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{} / 3", state.step + 1)).font(theme::mono(11.0)).color(TEXT_3));
                });
            });
            widgets::hairline(ui);
            ui.add_space(10.0);
            match state.step {
                0 => {
                    ui.label("Which monitor should it live on?");
                    widgets::hint(ui, "Click one. F11 toggles fullscreen there; the window remembers.");
                    ui.add_space(8.0);
                    let mut sel = selected_monitor;
                    if crate::panel::monitor_diagram(ui, monitors, &mut sel) {
                        a.monitor = Some(sel);
                    }
                    let mut sf = start_fullscreen;
                    if widgets::toggle_row(ui, "Start fullscreen on it", &mut sf).changed() {
                        a.start_fullscreen = Some(sf);
                    }
                }
                1 => {
                    ui.label("Line the visuals up with your speakers.");
                    widgets::hint(ui, "Loopback hears the music before you do. Tap along for ten beats and the app measures the gap. You can redo this any time with C.");
                    ui.add_space(12.0);
                    if ui.add(egui::Button::new(egui::RichText::new("Calibrate now").color(ACCENT)).fill(Color32::TRANSPARENT).stroke(egui::Stroke::new(1.0, ACCENT))).clicked() {
                        a.calibrate = true;
                    }
                }
                _ => {
                    ui.label("Pick a starting scene.");
                    widgets::hint(ui, "1, 2, 3 switch any time. A lets the director change scenes on track changes and drops.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        for (i, name) in scene_names.iter().enumerate() {
                            let active = i == scene;
                            let b = egui::Button::new(egui::RichText::new(format!("{}  {}", i + 1, name)).color(if active { ACCENT } else { TEXT })).fill(if active { theme::with_alpha(ACCENT, 30) } else { Color32::TRANSPARENT }).stroke(egui::Stroke::new(1.0, if active { ACCENT } else { HAIRLINE }));
                            if ui.add_sized([120.0, 28.0], b).clicked() {
                                a.scene = Some(i);
                            }
                        }
                    });
                }
            }
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if state.step > 0 && widgets::ghost_button(ui, "Back", false).clicked() {
                    state.step -= 1;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let last = state.step >= 2;
                    let label = if last { "Done" } else { "Next" };
                    if ui.add(egui::Button::new(egui::RichText::new(label).color(BG)).fill(ACCENT)).clicked() {
                        if last {
                            a.done = true;
                        } else {
                            state.step += 1;
                        }
                    }
                    if !last && widgets::ghost_button(ui, "Skip all", false).clicked() {
                        a.done = true;
                    }
                });
            });
        });
    });
    a
}
