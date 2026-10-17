//! Design tokens and the egui style built from them. Single source of truth
//! for colours, type scale and spacing; `xtask/src/mockup.rs` carries the
//! same values for the mockups.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(0x0b, 0x0e, 0x10);
pub const PANEL: Color32 = Color32::from_rgb(0x0e, 0x12, 0x15);
pub const HAIRLINE: Color32 = Color32::from_rgb(0x1c, 0x22, 0x26);
pub const RAISED: Color32 = Color32::from_rgb(0x15, 0x1b, 0x1f);
pub const TEXT: Color32 = Color32::from_rgb(0xd9, 0xdf, 0xdd);
pub const TEXT_2: Color32 = Color32::from_rgb(0x7d, 0x8a, 0x86);
pub const TEXT_3: Color32 = Color32::from_rgb(0x4a, 0x55, 0x52);
pub const ACCENT: Color32 = Color32::from_rgb(0x66, 0xff, 0x8c);
pub const WARN: Color32 = Color32::from_rgb(0xf2, 0xb2, 0x6b);
pub const ERROR: Color32 = Color32::from_rgb(0xff, 0x78, 0x78);

pub const BAR_H: f32 = 28.0;
pub const STRIP_H: f32 = 72.0;
pub const PANEL_W: f32 = 352.0;
pub const RAIL_W: f32 = 40.0;
pub const RADIUS: u8 = 4;
/// Seconds of mouse rest before the chrome hides.
pub const AUTOHIDE_S: f32 = 3.0;
/// Show/hide animation length.
pub const ANIM_S: f32 = 0.12;

pub const FAMILY_BOLD: &str = "swiss-bold";
pub const FAMILY_REGULAR: &str = "swiss";
pub const FAMILY_MONO: &str = "mono";

/// One fixed hue per layer slot, used on its stack row, its bar pip and the
/// inspector header so the layer being edited is known without reading.
pub const LAYER_HUES: [Color32; 3] = [ACCENT, Color32::from_rgb(0x6f, 0xb3, 0xff), Color32::from_rgb(0xff, 0xc4, 0x6b)];

pub fn layer_hue(i: usize) -> Color32 {
    LAYER_HUES[i % LAYER_HUES.len()]
}

pub fn panel_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(PANEL.r(), PANEL.g(), PANEL.b(), 235)
}

pub fn bar_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(BG.r(), BG.g(), BG.b(), 225)
}

pub fn accent_from(tint: [f32; 3]) -> Color32 {
    Color32::from_rgb((tint[0].clamp(0.0, 1.0) * 255.0) as u8, (tint[1].clamp(0.0, 1.0) * 255.0) as u8, (tint[2].clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_MONO.into()))
}
pub fn sans(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_REGULAR.into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_BOLD.into()))
}

/// Bundled fonts (SIL Open Font License, see `assets/fonts/`): Inter for the
/// interface and the title overlay, JetBrains Mono for numbers. Embedded so
/// the app looks the same on every machine; egui's bundled fonts stay as the
/// glyph fallback (symbols, emoji).
pub const FAMILY_MEDIUM: &str = "swiss-medium";
const INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
const MONO_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");

pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut add = |family: &str, bytes: &'static [u8], fallback: &str| {
        fonts.font_data.insert(family.to_string(), Arc::new(egui::FontData::from_static(bytes)));
        fonts.families.insert(FontFamily::Name(family.into()), vec![family.to_string(), fallback.to_string()]);
    };
    add(FAMILY_REGULAR, INTER_REGULAR, "Ubuntu-Light");
    add(FAMILY_MEDIUM, INTER_MEDIUM, "Ubuntu-Light");
    add(FAMILY_BOLD, INTER_SEMIBOLD, "Ubuntu-Light");
    add(FAMILY_MONO, MONO_REGULAR, "Hack");
    // Make them the defaults so every widget picks them up.
    if let Some(v) = fonts.families.get_mut(&FontFamily::Proportional) {
        v.insert(0, FAMILY_REGULAR.to_string());
    }
    if let Some(v) = fonts.families.get_mut(&FontFamily::Monospace) {
        v.insert(0, FAMILY_MONO.to_string());
    }
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style());
    ctx.set_style_of(egui::Theme::Light, style());
}

pub fn style() -> egui::Style {
    let mut s = egui::Style::default();
    s.text_styles = [
        (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(12.5, FontFamily::Proportional)),
        (TextStyle::Heading, FontId::new(15.0, FontFamily::Name(FAMILY_BOLD.into()))),
        (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
    ]
    .into();
    s.spacing.item_spacing = egui::vec2(8.0, 8.0);
    s.spacing.button_padding = egui::vec2(10.0, 4.0);
    s.spacing.slider_width = 180.0;
    s.spacing.slider_rail_height = 3.0;
    s.spacing.interact_size = egui::vec2(32.0, 22.0);
    s.spacing.indent = 12.0;
    s.spacing.combo_width = 160.0;
    s.animation_time = ANIM_S;

    let r = CornerRadius::same(RADIUS);
    let v = &mut s.visuals;
    *v = egui::Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = panel_fill();
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, HAIRLINE);
    v.window_corner_radius = r;
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    v.extreme_bg_color = BG;
    v.faint_bg_color = RAISED;
    v.code_bg_color = RAISED;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = with_alpha(ACCENT, 60);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, HAIRLINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_2);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = RAISED;
    w.inactive.weak_bg_fill = RAISED;
    w.inactive.bg_stroke = Stroke::new(1.0, HAIRLINE);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = Color32::from_rgb(0x1c, 0x24, 0x29);
    w.hovered.weak_bg_fill = Color32::from_rgb(0x1c, 0x24, 0x29);
    w.hovered.bg_stroke = Stroke::new(1.0, TEXT_3);
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = Color32::from_rgb(0x12, 0x2a, 0x1c);
    w.active.weak_bg_fill = Color32::from_rgb(0x12, 0x2a, 0x1c);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, ACCENT);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open.bg_fill = RAISED;
    w.open.weak_bg_fill = RAISED;
    w.open.bg_stroke = Stroke::new(1.0, HAIRLINE);
    w.open.fg_stroke = Stroke::new(1.0, TEXT);
    w.open.corner_radius = r;
    s
}
