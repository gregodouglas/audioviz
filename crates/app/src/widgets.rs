//! Small drawing helpers shared by the chrome, panel, strip and overlays.

use std::collections::BTreeMap;
use std::sync::Mutex;

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

/// Label left, editable monospace value right (drag for fine steps, click
/// to type), full-width slider underneath.
pub fn slider_row<Num: egui::emath::Numeric>(ui: &mut Ui, label: &str, value: &mut Num, range: std::ops::RangeInclusive<Num>, fmt: impl Fn(f64) -> String, log: bool) -> Response {
    slider_def(ui, label, value, None, range, fmt, log)
}

/// `slider_row` that knows the control's default: a tick marks it on the rail,
/// the label is dimmed while the value sits at it, and a double-click on the
/// slider puts it back. One line: label · slider · editable value.
pub fn slider_def<Num: egui::emath::Numeric>(ui: &mut Ui, label: &str, value: &mut Num, default: Option<Num>, range: std::ops::RangeInclusive<Num>, fmt: impl Fn(f64) -> String, log: bool) -> Response {
    let at_default = default.is_some_and(|d| (value.to_f64() - d.to_f64()).abs() <= 1e-6 * (1.0 + d.to_f64().abs()));
    let (lo, hi) = (range.start().to_f64(), range.end().to_f64());
    let span = (hi - lo).abs().max(1e-9);
    let id = ui.scope_id().with(("slider", label));
    let hovered_last = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let mut out: Option<Response> = None;
    let frame = egui::Frame::new().fill(if hovered_last { theme::with_alpha(theme::RAISED, 160) } else { Color32::TRANSPARENT }).corner_radius(4.0).inner_margin(egui::Margin::symmetric(4, 1)).outer_margin(egui::Margin::symmetric(-4, 0));
    let fr = frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let label_w = 108.0;
            let value_w = 48.0;
            let lc = if hovered_last { TEXT } else if at_default { TEXT_2 } else { TEXT };
            let r = ui.allocate_ui_with_layout(vec2(label_w, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(label_w);
                ui.add(egui::Label::new(egui::RichText::new(label).size(12.5).color(lc)).truncate())
            }).inner;
            if r.hovered() && ui.painter().layout_no_wrap(label.to_string(), theme::sans(12.5), TEXT).size().x > label_w {
                r.on_hover_text(label);
            }
            let sw = (ui.available_width() - value_w - 8.0).max(40.0);
            ui.spacing_mut().slider_width = sw;
            ui.spacing_mut().interact_size.y = 16.0;
            // The rail: visible on the panel, the travelled part in the accent.
            let v = ui.visuals_mut();
            v.widgets.inactive.bg_fill = theme::with_alpha(TEXT_3, 110);
            v.widgets.hovered.bg_fill = theme::with_alpha(TEXT_3, 150);
            v.widgets.active.bg_fill = theme::with_alpha(TEXT_3, 150);
            let mut sl = egui::Slider::new(value, range.clone()).show_value(false);
            if log {
                sl = sl.logarithmic(true);
            }
            let r = ui.add(sl);
            // Default tick on the rail.
            if let Some(d) = default {
                let d = d.to_f64();
                let t = if log && lo > 0.0 && hi > 0.0 { (d.ln() - lo.ln()) / (hi.ln() - lo.ln()) } else if log { -1.0 } else { (d - lo) / span };
                if (0.0..=1.0).contains(&t) {
                    let rail = r.rect;
                    let x = rail.min.x + 8.0 + (rail.width() - 16.0) * t as f32;
                    ui.painter().vline(x, (rail.center().y - 5.0)..=(rail.center().y + 5.0), Stroke::new(1.0, TEXT_3));
                }
                if r.double_clicked() {
                    *value = d_as(d, default);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let v = ui.visuals_mut();
                v.widgets.inactive.bg_fill = Color32::TRANSPARENT;
                v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                v.widgets.inactive.bg_stroke = Stroke::NONE;
                v.widgets.hovered.bg_fill = theme::RAISED;
                v.widgets.hovered.weak_bg_fill = theme::RAISED;
                v.widgets.hovered.bg_stroke = Stroke::new(1.0, HAIRLINE);
                v.widgets.active.bg_fill = theme::RAISED;
                v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_2);
                ui.style_mut().drag_value_text_style = egui::TextStyle::Small;
                ui.style_mut().text_styles.insert(egui::TextStyle::Small, theme::mono(11.5));
                ui.spacing_mut().button_padding = vec2(3.0, 1.0);
                let speed = if log { value.to_f64().abs().max(span * 1e-3) * 0.01 } else { span / 400.0 };
                let dv = egui::DragValue::new(value).range(range.clone()).speed(speed).custom_formatter(|n, _| fmt(n)).update_while_editing(false);
                ui.add_sized(vec2(value_w, 18.0), dv).on_hover_text("drag for fine steps, click to type");
            });
            out = Some(r);
        });
    });
    let hovered = fr.response.hovered() || fr.response.contains_pointer();
    ui.data_mut(|d| d.insert_temp(id, hovered));
    out.expect("slider row")
}

fn d_as<Num: egui::emath::Numeric>(_d: f64, default: Option<Num>) -> Num {
    default.expect("default")
}

/// A segmented control: one cell per label, the selected one filled. Returns
/// the newly chosen index when it changes.
pub fn segmented(ui: &mut Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let n = labels.len().max(1) as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 5.0, theme::RAISED);
    p.rect_stroke(rect, 5.0, Stroke::new(1.0, HAIRLINE), egui::StrokeKind::Inside);
    let cw = rect.width() / n;
    let mut picked = None;
    for (i, label) in labels.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(rect.min.x + cw * i as f32, rect.min.y), vec2(cw, rect.height()));
        let resp = ui.interact(cell, ui.scope_id().with(("seg", label, i)), Sense::click());
        let on = i == selected;
        let t = ui.ctx().animate_bool_with_time(resp.id, on, theme::ANIM_S);
        if t > 0.01 {
            ui.painter().rect_filled(cell.shrink(2.0), 4.0, theme::with_alpha(ACCENT, (40.0 * t) as u8));
        }
        let color = if on { ACCENT } else if resp.hovered() { TEXT } else { TEXT_2 };
        ui.painter().text(cell.center(), egui::Align2::CENTER_CENTER, *label, theme::sans(12.0), color);
        if i > 0 && !on && i != selected + 1 {
            ui.painter().vline(cell.min.x, (cell.min.y + 6.0)..=(cell.max.y - 6.0), Stroke::new(1.0, HAIRLINE));
        }
        if resp.clicked() && !on {
            picked = Some(i);
        }
    }
    ui.add_space(2.0);
    picked
}

/// A labelled dropdown filling the row: "Visual   [ Sigil ▾ ]". Returns the
/// newly chosen index when it changes.
pub fn dropdown(ui: &mut Ui, label: &str, items: &[&str], selected: Option<usize>) -> Option<usize> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.add_sized(vec2(104.0, 22.0), egui::Label::new(egui::RichText::new(label).color(TEXT_2)).halign(egui::Align::LEFT));
        let text = selected.and_then(|i| items.get(i)).copied().unwrap_or("—");
        let w = ui.available_width();
        egui::ComboBox::from_id_salt(ui.scope_id().with(("dropdown", label))).selected_text(egui::RichText::new(text).color(TEXT)).width(w).show_ui(ui, |ui| {
            ui.set_min_width(w - 16.0);
            for (i, item) in items.iter().enumerate() {
                let on = Some(i) == selected;
                if ui.add(egui::Button::selectable(on, egui::RichText::new(*item).color(if on { ACCENT } else { TEXT }))).clicked() && !on {
                    picked = Some(i);
                }
            }
        });
    });
    picked
}

/// Collapsible control group with a remembered open state (see
/// `groups_load` / `groups_store`). The header shows the control count while
/// the group is closed.
pub fn group(ui: &mut Ui, key: &str, title: &str, default_open: bool, count: usize, body: impl FnOnce(&mut Ui)) {
    let open = groups().get(key).copied().unwrap_or(default_open);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let p = ui.painter();
    let c = if resp.hovered() { TEXT } else { TEXT_2 };
    let cx = rect.min.x + 6.0;
    let cy = rect.center().y;
    let tri = if open { vec![pos2(cx - 4.0, cy - 2.0), pos2(cx + 4.0, cy - 2.0), pos2(cx, cy + 3.0)] } else { vec![pos2(cx - 2.0, cy - 4.0), pos2(cx - 2.0, cy + 4.0), pos2(cx + 3.0, cy)] };
    p.add(egui::Shape::convex_polygon(tri, c, Stroke::NONE));
    p.text(pos2(rect.min.x + 18.0, cy), egui::Align2::LEFT_CENTER, title.to_uppercase(), theme::bold(10.5), c);
    if !open && count > 0 {
        p.text(pos2(rect.max.x - 2.0, cy), egui::Align2::RIGHT_CENTER, count.to_string(), theme::mono(10.5), TEXT_3);
    }
    if resp.clicked() {
        groups().insert(key.to_string(), !open);
    }
    if open {
        ui.add_space(2.0);
        ui.indent(("group", key), body);
        ui.add_space(4.0);
    }
}

static GROUPS: Mutex<BTreeMap<String, bool>> = Mutex::new(BTreeMap::new());

fn groups() -> std::sync::MutexGuard<'static, BTreeMap<String, bool>> {
    GROUPS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Seed the group states from the saved settings (once, at startup).
pub fn groups_load(saved: &BTreeMap<String, bool>) {
    let mut g = groups();
    if g.is_empty() {
        *g = saved.clone();
    }
}

/// Copy the group states back into the settings; true when they changed.
pub fn groups_store(saved: &mut BTreeMap<String, bool>) -> bool {
    let g = groups();
    if *g != *saved {
        *saved = g.clone();
        true
    } else {
        false
    }
}

/// First letter up, for keys that double as display names ("neon" → "Neon").
pub fn title(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A small rounded tag in a colour; returns the response (clickable).
pub fn chip(ui: &mut Ui, text: &str, color: Color32, clickable: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), theme::mono(10.5), color);
    let size = vec2(galley.size().x + 10.0, 16.0);
    let (rect, resp) = ui.allocate_exact_size(size, if clickable { Sense::click() } else { Sense::hover() });
    let fill = if clickable && resp.hovered() { theme::with_alpha(color, 50) } else { theme::with_alpha(color, 22) };
    ui.painter().rect_filled(rect, 8.0, fill);
    ui.painter().galley(pos2(rect.min.x + 5.0, rect.center().y - galley.size().y * 0.5), galley, color);
    if clickable {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// The inspector header: a hue bar, a breadcrumb ("LAYER 1 · SIGIL › LUMINESCENCE")
/// and an optional right-aligned action. Returns true when the action is clicked.
pub fn inspector_header(ui: &mut Ui, hue: Color32, crumbs: &[&str], action: Option<&str>) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 1.5, hue);
    let mut x = rect.min.x + 12.0;
    for (i, c) in crumbs.iter().enumerate() {
        if i > 0 {
            let g = p.layout_no_wrap("›".into(), theme::sans(13.0), TEXT_3);
            p.galley(pos2(x, rect.center().y - g.size().y * 0.5), g.clone(), TEXT_3);
            x += g.size().x + 6.0;
        }
        let last = i + 1 == crumbs.len();
        let g = p.layout_no_wrap(c.to_uppercase(), theme::bold(11.0), if last { TEXT } else { TEXT_2 });
        p.galley(pos2(x, rect.center().y - g.size().y * 0.5), g.clone(), if last { TEXT } else { TEXT_2 });
        x += g.size().x + 6.0;
    }
    let mut clicked = false;
    if let Some(a) = action {
        let g = ui.painter().layout_no_wrap(a.to_string(), theme::sans(12.0), TEXT_2);
        let r = Rect::from_min_size(pos2(rect.max.x - g.size().x - 8.0, rect.min.y), vec2(g.size().x + 8.0, rect.height()));
        let resp = ui.interact(r, ui.scope_id().with(("inspector-action", a)), Sense::click());
        ui.painter().galley(pos2(r.min.x + 4.0, rect.center().y - g.size().y * 0.5), g, if resp.hovered() { TEXT } else { TEXT_2 });
        clicked = resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    }
    ui.painter().hline(rect.x_range(), rect.max.y + 0.5, Stroke::new(1.0, HAIRLINE));
    ui.add_space(6.0);
    clicked
}

/// A switch with a label, mockup style.
pub fn toggle_row(ui: &mut Ui, label: &str, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect.expand2(vec2(4.0, 0.0)), 4.0, theme::with_alpha(theme::RAISED, 160));
    }
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
        Section::Scenes => {
            painter.circle_stroke(c, 6.5, st);
            painter.circle_filled(c, 2.0, color);
        }
        Section::Picture => {
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
        Section::Layers => {
            // three stacked tiles
            painter.rect_stroke(Rect::from_min_size(pos2(c.x - 7.0, c.y - 7.0), vec2(10.0, 10.0)), 1.5, st, egui::StrokeKind::Middle);
            painter.rect_stroke(Rect::from_min_size(pos2(c.x - 4.0, c.y - 4.0), vec2(10.0, 10.0)), 1.5, st, egui::StrokeKind::Middle);
            painter.rect_filled(Rect::from_min_size(pos2(c.x - 1.0, c.y - 1.0), vec2(9.0, 9.0)), 1.5, color);
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

