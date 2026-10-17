//! The integrated title bar (VS Code style): mark · name · scene pips ·
//! status chips · window controls. Drawn by egui at the top of the window;
//! the window itself is frameless. Auto-hides after a few seconds of mouse
//! rest and in fullscreen. Native drag / snap come from `hittest.rs` on
//! Windows; elsewhere the bar's own drag starts a winit window drag.

use std::time::Instant;

use audioviz_audio::CaptureMode;
use egui::{Align2, Color32, Rect, Sense, pos2, vec2};

use crate::settings::Section;
use crate::theme::{self, ACCENT, BAR_H, ERROR, HAIRLINE, TEXT, TEXT_2, TEXT_3};
use crate::widgets::{self, Glyph};

pub struct ChromeInput<'a> {
    pub scene_names: &'a [&'static str],
    pub scene_current: usize,
    pub auto: bool,
    pub capture: &'a CaptureMode,
    pub target_short: &'a str,
    pub sync_ms: f32,
    pub monitor_index: usize,
    pub fps: f32,
    pub maximized: bool,
    pub accent: Color32,
}

#[derive(Default)]
pub struct ChromeActions {
    pub switch_scene: Option<usize>,
    pub toggle_auto: bool,
    pub open_section: Option<Section>,
    pub minimize: bool,
    pub toggle_maximize: bool,
    pub close: bool,
    pub start_drag: bool,
}

/// Regions the native hit test needs, in logical points.
#[derive(Clone)]
pub struct HitRegions {
    pub bar: Rect,
    /// Rects inside the bar that egui handles (pips, chips, min/close).
    pub interactive: Vec<Rect>,
    /// The maximize button (Snap Layouts hover target).
    pub max_button: Rect,
}

impl Default for HitRegions {
    fn default() -> Self {
        Self { bar: Rect::NOTHING, interactive: Vec::new(), max_button: Rect::NOTHING }
    }
}

pub struct Chrome {
    pub last_mouse: Instant,
    pub regions: HitRegions,
    /// Set by the native hit test when the pointer is over the maximize button.
    pub max_hover: bool,
}

impl Chrome {
    pub fn new() -> Self {
        Self { last_mouse: Instant::now(), regions: HitRegions::default(), max_hover: false }
    }

    pub fn note_mouse(&mut self) {
        self.last_mouse = Instant::now();
    }

    /// Whether the chrome wants to be visible, before animation.
    /// `pinned`: a sheet or the panel is open, or the app just started.
    pub fn wants_visible(&self, ctx: &egui::Context, fullscreen: bool, autohide: bool, pinned: bool) -> bool {
        if fullscreen && !pinned {
            // Fullscreen: only on pointer near the top edge.
            return ctx.pointer_latest_pos().is_some_and(|p| p.y < BAR_H * 1.5) && self.last_mouse.elapsed().as_secs_f32() < theme::AUTOHIDE_S;
        }
        if !autohide || pinned {
            return true;
        }
        let recent = self.last_mouse.elapsed().as_secs_f32() < theme::AUTOHIDE_S;
        let over = ctx.pointer_latest_pos().is_some_and(|p| p.y < BAR_H + 8.0);
        recent || over
    }

    /// Draw the bar. `t` is the animated visibility (0 hidden .. 1 shown).
    pub fn draw(&mut self, ctx: &egui::Context, input: &ChromeInput, t: f32) -> ChromeActions {
        let mut actions = ChromeActions::default();
        self.regions = HitRegions::default();
        if t <= 0.001 {
            return actions;
        }
        let screen = ctx.content_rect();
        let y_off = -(1.0 - t) * BAR_H;
        let bar = Rect::from_min_size(pos2(screen.min.x, screen.min.y + y_off), vec2(screen.width(), BAR_H));
        self.regions.bar = bar;

        egui::Area::new(egui::Id::unique("chrome-bar")).fixed_pos(bar.min).order(egui::Order::Foreground).interactable(true).show(ctx, |ui| {
            ui.set_min_size(bar.size());
            let painter = ui.painter().clone();
            painter.rect_filled(bar, 0.0, theme::bar_fill());
            painter.hline(bar.x_range(), bar.max.y - 0.5, egui::Stroke::new(1.0, HAIRLINE));

            // Whole-bar drag surface (fallback when there is no native hit test).
            let bg = ui.interact(bar, egui::Id::unique("chrome-drag"), Sense::click_and_drag());
            if bg.drag_started() {
                actions.start_drag = true;
            }
            if bg.double_clicked() {
                actions.toggle_maximize = true;
            }

            // Mark + name.
            widgets::mark(&painter, pos2(bar.min.x + 22.0, bar.center().y), 7.0, input.accent, true);
            painter.text(pos2(bar.min.x + 36.0, bar.center().y), Align2::LEFT_CENTER, "audioviz", theme::bold(13.0), TEXT);

            // Scene pips, centred, sized to their text (short name = last word).
            let labels: Vec<String> = input.scene_names.iter().enumerate().map(|(i, n)| format!("{}  {}", i + 1, n.rsplit(' ').next().unwrap_or(n).to_lowercase())).collect();
            let widths: Vec<f32> = labels.iter().map(|l| painter.layout_no_wrap(l.clone(), theme::bold(12.0), TEXT).size().x + 28.0).collect();
            let total: f32 = widths.iter().sum();
            let mut x = bar.center().x - total * 0.5;
            for (i, label) in labels.iter().enumerate() {
                let pw = widths[i];
                let r = Rect::from_min_size(pos2(x, bar.min.y), vec2(pw, BAR_H));
                let resp = ui.interact(r, egui::Id::unique(("pip", i)), Sense::click());
                let active = i == input.scene_current;
                if resp.clicked() && !active {
                    actions.switch_scene = Some(i);
                }
                let color = if active { TEXT } else if resp.hovered() { TEXT } else { TEXT_2 };
                let font = if active { theme::bold(12.0) } else { theme::sans(12.0) };
                painter.text(r.center(), Align2::CENTER_CENTER, label, font, color);
                if active {
                    painter.rect_filled(Rect::from_min_size(pos2(r.min.x + 6.0, r.max.y - 2.0), vec2(pw - 12.0, 2.0)), 0.0, input.accent);
                }
                self.regions.interactive.push(r);
                x += pw;
            }
            let ra = Rect::from_min_size(pos2(x, bar.min.y), vec2(64.0, BAR_H));
            let resp = ui.interact(ra, egui::Id::unique("pip-auto"), Sense::click());
            if resp.clicked() {
                actions.toggle_auto = true;
            }
            painter.text(ra.center(), Align2::CENTER_CENTER, "A  auto", theme::sans(12.0), if input.auto { input.accent } else if resp.hovered() { TEXT_2 } else { TEXT_3 });
            self.regions.interactive.push(ra);

            // Window controls, right.
            let cw = 46.0;
            let controls = [(Glyph::Minimize, "min"), (if input.maximized { Glyph::Restore } else { Glyph::Maximize }, "max"), (Glyph::Close, "close")];
            for (i, (g, id)) in controls.into_iter().enumerate() {
                let r = Rect::from_min_size(pos2(bar.max.x - cw * (3.0 - i as f32), bar.min.y), vec2(cw, BAR_H));
                let resp = ui.interact(r, egui::Id::unique(("winctl", id)), Sense::click());
                let hovered = resp.hovered() || (id == "max" && self.max_hover);
                if hovered {
                    let fill = if id == "close" { Color32::from_rgb(0xc4, 0x2b, 0x1c) } else { theme::RAISED };
                    painter.rect_filled(r, 0.0, fill);
                }
                widgets::glyph(&painter, r.center(), g, if hovered { TEXT } else { TEXT_2 });
                if resp.clicked() {
                    match id {
                        "min" => actions.minimize = true,
                        "max" => actions.toggle_maximize = true,
                        _ => actions.close = true,
                    }
                }
                if id == "max" {
                    self.regions.max_button = r;
                } else {
                    self.regions.interactive.push(r);
                }
            }

            // Status chips, right of centre, left of the controls.
            let chips_right = bar.max.x - cw * 3.0 - 12.0;
            let (cap_text, cap_color, cap_section) = match input.capture {
                CaptureMode::Starting => ("starting".to_string(), TEXT_3, Section::Sync),
                CaptureMode::Process { .. } => (input.target_short.to_string(), input.accent, Section::Sync),
                CaptureMode::Endpoint { .. } => ("system audio".to_string(), TEXT_2, Section::Sync),
                CaptureMode::Failed(_) => ("no audio".to_string(), ERROR, Section::Sync),
            };
            let chips: [(String, Color32, Section); 3] = [
                (format!("M{} {:.0} fps", input.monitor_index + 1, input.fps), TEXT_2, Section::Display),
                (format!("sync {:.0} ms", input.sync_ms), TEXT_2, Section::Sync),
                (cap_text, cap_color, cap_section),
            ];
            let mut x = chips_right;
            for (i, (text, color, section)) in chips.iter().enumerate() {
                let galley = painter.layout_no_wrap(text.clone(), theme::mono(11.0), *color);
                let w = galley.size().x + 18.0;
                let r = Rect::from_min_size(pos2(x - w, bar.center().y - 10.0), vec2(w, 20.0));
                let resp = ui.interact(r, egui::Id::unique(("chip", i)), Sense::click());
                painter.rect_filled(r, 10.0, theme::with_alpha(*color, if resp.hovered() { 55 } else { 30 }));
                painter.galley(pos2(r.min.x + 9.0, r.center().y - galley.size().y * 0.5), galley, *color);
                if resp.clicked() {
                    actions.open_section = Some(*section);
                }
                self.regions.interactive.push(r);
                x -= w + 8.0;
            }
        });
        actions
    }
}

/// Accent for the chrome: the scene tint.
pub fn accent(tint: [f32; 3]) -> Color32 {
    let c = theme::accent_from(tint);
    // Keep it readable on the dark bar.
    if (c.r() as u32 + c.g() as u32 + c.b() as u32) < 240 { ACCENT } else { c }
}
