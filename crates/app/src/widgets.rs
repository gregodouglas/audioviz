//! Small drawing helpers shared by the chrome, panel, strip and overlays.

use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::settings::Section;
use crate::theme::{self, ACCENT, HAIRLINE, TEXT, TEXT_2, TEXT_3};

/// The Lissajous mark, stroked, centred at `c` with radius `r`.
pub fn mark(painter: &egui::Painter, c: Pos2, r: f32, color: Color32, glow: bool) {
    let n = 160;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            pos2(c.x + r * 1.05 * (3.0 * t + std::f32::consts::FRAC_PI_2).sin(), c.y + r * (2.0 * t).sin())
        })
        .collect();
    let w = (r / 9.0).max(1.0);
    if glow {
        painter.add(egui::Shape::line(pts.clone(), Stroke::new(w * 3.0, theme::with_alpha(color, 70))));
    }
    painter.add(egui::Shape::line(pts, Stroke::new(w, color)));
}

/// Label left, monospace value right, full-width slider underneath.
pub fn slider_row<Num: egui::emath::Numeric>(ui: &mut Ui, label: &str, value: &mut Num, range: std::ops::RangeInclusive<Num>, fmt: impl Fn(f64) -> String, log: bool) -> Response {
    let text = fmt(value.to_f64());
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(text).font(theme::mono(12.0)).color(TEXT_2));
        });
    });
    let w = ui.available_width();
    ui.spacing_mut().slider_width = w;
    let mut s = egui::Slider::new(value, range).show_value(false);
    if log {
        s = s.logarithmic(true);
    }
    let r = ui.add(s);
    ui.add_space(4.0);
    r
}

/// A switch with a label, mockup style.
pub fn toggle_row(ui: &mut Ui, label: &str, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, theme::ANIM_S);
    let painter = ui.painter();
    let sw = Rect::from_min_size(pos2(rect.min.x, rect.center().y - 8.0), vec2(28.0, 16.0));
    let bg = lerp_color(HAIRLINE, ACCENT, t);
    painter.rect_filled(sw, 8.0, bg);
    let kx = sw.min.x + 8.0 + 12.0 * t;
    painter.circle_filled(pos2(kx, sw.center().y), 6.0, lerp_color(TEXT_2, theme::BG, t));
    painter.text(pos2(sw.max.x + 10.0, rect.center().y), egui::Align2::LEFT_CENTER, label, theme::sans(13.0), TEXT);
    resp
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Section header with an optional right-aligned action ("Reset").
pub fn section_header(ui: &mut Ui, title: &str, subtitle: Option<&str>, action: Option<&str>) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).font(theme::bold(15.0)).color(TEXT));
        if let Some(s) = subtitle {
            ui.label(egui::RichText::new(s).color(TEXT_2));
        }
        if let Some(a) = action {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(egui::RichText::new(a).size(12.0).color(TEXT_2)).frame(false)).clicked() {
                    clicked = true;
                }
            });
        }
    });
    hairline(ui);
    ui.add_space(8.0);
    clicked
}

pub fn hairline(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, HAIRLINE);
}

pub fn small_caps(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text.to_uppercase()).size(10.5).color(TEXT_2).strong());
}

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(11.5).color(TEXT_3));
}

pub fn ghost_button(ui: &mut Ui, label: &str, accent: bool) -> Response {
    let color = if accent { ACCENT } else { TEXT };
    ui.add(egui::Button::new(egui::RichText::new(label).color(color)).fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, if accent { ACCENT } else { HAIRLINE })))
}

/// Rail icon for a section, drawn with the painter so it matches the mockup.
pub fn rail_icon(painter: &egui::Painter, c: Pos2, section: Section, color: Color32) {
    let sw = 1.5;
    let st = Stroke::new(sw, color);
    match section {
        Section::Scene => {
            painter.circle_stroke(c, 6.5, st);
            painter.circle_filled(c, 2.0, color);
        }
        Section::Post => {
            painter.circle_stroke(c, 7.0, st);
            painter.circle_stroke(c, 3.5, st);
        }
        Section::Sync => {
            painter.circle_stroke(c, 7.0, st);
            painter.line_segment([pos2(c.x, c.y - 4.5), c], st);
            painter.line_segment([c, pos2(c.x + 3.5, c.y)], st);
        }
        Section::Display => {
            painter.rect_stroke(Rect::from_min_size(pos2(c.x - 7.5, c.y - 6.5), vec2(15.0, 10.0)), 1.5, st, egui::StrokeKind::Middle);
            painter.line_segment([pos2(c.x - 4.0, c.y + 7.0), pos2(c.x + 4.0, c.y + 7.0)], st);
        }
        Section::Looks => {
            painter.rect_stroke(Rect::from_min_size(pos2(c.x - 7.0, c.y - 7.0), vec2(14.0, 14.0)), 2.0, st, egui::StrokeKind::Middle);
            painter.line_segment([pos2(c.x - 3.0, c.y), pos2(c.x + 3.0, c.y)], st);
        }
        Section::Meters => {
            painter.rect_filled(Rect::from_min_size(pos2(c.x - 7.5, c.y + 1.0), vec2(3.0, 6.0)), 0.0, color);
            painter.rect_filled(Rect::from_min_size(pos2(c.x - 1.5, c.y - 6.0), vec2(3.0, 13.0)), 0.0, color);
            painter.rect_filled(Rect::from_min_size(pos2(c.x + 4.5, c.y - 2.0), vec2(3.0, 9.0)), 0.0, color);
        }
        Section::About => {
            painter.circle_stroke(c, 7.0, st);
            painter.line_segment([pos2(c.x, c.y - 1.0), pos2(c.x, c.y + 3.5)], st);
            painter.circle_filled(pos2(c.x, c.y - 4.0), 1.0, color);
        }
    }
}

/// Window-control glyphs.
pub enum Glyph {
    Minimize,
    Maximize,
    Restore,
    Close,
}

pub fn glyph(painter: &egui::Painter, c: Pos2, g: Glyph, color: Color32) {
    let st = Stroke::new(1.0, color);
    match g {
        Glyph::Minimize => {
            painter.line_segment([pos2(c.x - 5.0, c.y), pos2(c.x + 5.0, c.y)], st);
        }
        Glyph::Maximize => {
            painter.rect_stroke(Rect::from_center_size(c, Vec2::splat(10.0)), 0.0, st, egui::StrokeKind::Middle);
        }
        Glyph::Restore => {
            painter.rect_stroke(Rect::from_center_size(pos2(c.x - 1.0, c.y + 1.0), Vec2::splat(8.0)), 0.0, st, egui::StrokeKind::Middle);
            painter.line_segment([pos2(c.x - 3.0, c.y - 3.0), pos2(c.x - 3.0, c.y - 3.0)], st);
            painter.line_segment([pos2(c.x - 3.0, c.y - 3.0), pos2(c.x + 5.0, c.y - 3.0)], st);
            painter.line_segment([pos2(c.x + 5.0, c.y - 3.0), pos2(c.x + 5.0, c.y + 5.0)], st);
        }
        Glyph::Close => {
            painter.line_segment([pos2(c.x - 5.0, c.y - 5.0), pos2(c.x + 5.0, c.y + 5.0)], st);
            painter.line_segment([pos2(c.x + 5.0, c.y - 5.0), pos2(c.x - 5.0, c.y + 5.0)], st);
        }
    }
}

