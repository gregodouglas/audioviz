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
pub const PANEL_W: f32 = 320.0;
pub const RAIL_W: f32 = 40.0;
pub const RADIUS: u8 = 4;
/// Seconds of mouse rest before the chrome hides.
pub const AUTOHIDE_S: f32 = 3.0;
/// Show/hide animation length.
pub const ANIM_S: f32 = 0.12;

pub const FAMILY_BOLD: &str = "swiss-bold";
pub const FAMILY_REGULAR: &str = "swiss";
pub const FAMILY_MONO: &str = "mono";

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

/// Install system fonts (a grotesk and a monospace) with egui's bundled fonts
/// as the fallback, then the style.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let bold_c = ["C:\\Windows\\Fonts\\segoeuib.ttf", "C:\\Windows\\Fonts\\arialbd.ttf", "/System/Library/Fonts/HelveticaNeue.ttc", "/Library/Fonts/Arial Bold.ttf"];
    let regular_c = ["C:\\Windows\\Fonts\\segoeui.ttf", "C:\\Windows\\Fonts\\arial.ttf", "/System/Library/Fonts/HelveticaNeue.ttc", "/Library/Fonts/Arial.ttf"];
    let mono_c = ["C:\\Windows\\Fonts\\CascadiaMono.ttf", "C:\\Windows\\Fonts\\consola.ttf", "/System/Library/Fonts/SFNSMono.ttf", "/System/Library/Fonts/Menlo.ttc"];
    let mut add = |family: &str, candidates: &[&str], fallback: &str| {
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                fonts.font_data.insert(family.to_string(), Arc::new(egui::FontData::from_owned(bytes)));
                fonts.families.insert(FontFamily::Name(family.into()), vec![family.to_string(), fallback.to_string()]);
                tracing::info!("font {family}: {path}");
                return true;
            }
        }
        fonts.families.insert(FontFamily::Name(family.into()), vec![fallback.to_string()]);
        false
    };
    let have_sans = add(FAMILY_REGULAR, &regular_c, "Ubuntu-Light");
    add(FAMILY_BOLD, &bold_c, "Ubuntu-Light");
    let have_mono = add(FAMILY_MONO, &mono_c, "Hack");
    // Make the system faces the defaults so every widget picks them up.
    if have_sans {
        if let Some(v) = fonts.families.get_mut(&FontFamily::Proportional) {
            v.insert(0, FAMILY_REGULAR.to_string());
        }
    }
    if have_mono {
        if let Some(v) = fonts.families.get_mut(&FontFamily::Monospace) {
            v.insert(0, FAMILY_MONO.to_string());
        }
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
