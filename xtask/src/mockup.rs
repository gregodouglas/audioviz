//! `cargo xtask mockup`: draws the Phase 6 UI direction (docs/phase6-ui.md) as
//! SVG from the design tokens below and renders PNGs into docs/img/. The point
//! is a picture Greg can react to before the window changes; the tokens here
//! are the ones the egui style will use.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context;

// ---- design tokens -------------------------------------------------------
pub const BG: &str = "#0b0e10";
pub const PANEL: &str = "#0e1215";
pub const HAIRLINE: &str = "#1c2226";
pub const TEXT: &str = "#d9dfdd";
pub const TEXT_2: &str = "#7d8a86";
pub const TEXT_3: &str = "#4a5552";
pub const ACCENT: &str = "#66ff8c";
pub const SANS: &str = "Segoe UI, Helvetica Neue, Arial, sans-serif";
pub const MONO: &str = "Cascadia Mono, Consolas, SF Mono, monospace";
pub const BAR_H: f32 = 28.0;
pub const PANEL_W: f32 = 320.0;
pub const RAIL_W: f32 = 40.0;

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// 3:2 Lissajous path centred at (cx, cy) with radius r.
pub fn lissajous(cx: f32, cy: f32, r: f32) -> String {
    let n = 240;
    let mut d = String::new();
    for i in 0..=n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let x = cx + r * 1.05 * (3.0 * t + std::f32::consts::FRAC_PI_2).sin();
        let y = cy + r * (2.0 * t).sin();
        let _ = write!(d, "{}{:.1} {:.1} ", if i == 0 { "M" } else { "L" }, x, y);
    }
    d + "Z"
}

fn mark(cx: f32, cy: f32, r: f32, mono: bool) -> String {
    let p = lissajous(cx, cy, r);
    let w = r / 9.0;
    if mono {
        format!(r##"<path d="{p}" fill="none" stroke="{TEXT}" stroke-width="{w:.2}" stroke-linejoin="round"/>"##)
    } else {
        format!(
            r##"<path d="{p}" fill="none" stroke="{ACCENT}" stroke-opacity="0.35" stroke-width="{:.2}" stroke-linejoin="round"/><path d="{p}" fill="none" stroke="{ACCENT}" stroke-width="{w:.2}" stroke-linejoin="round"/>"##,
            w * 3.0
        )
    }
}

fn text(x: f32, y: f32, size: f32, color: &str, family: &str, weight: &str, anchor: &str, s: &str) -> String {
    let s = s.replace('&', "&amp;").replace('<', "&lt;");
    format!(r##"<text x="{x:.1}" y="{y:.1}" font-family="{family}" font-size="{size}" font-weight="{weight}" fill="{color}" text-anchor="{anchor}">{s}</text>"##)
}

fn hline(x0: f32, x1: f32, y: f32, color: &str) -> String {
    format!(r##"<line x1="{x0:.1}" y1="{y:.1}" x2="{x1:.1}" y2="{y:.1}" stroke="{color}" stroke-width="1"/>"##)
}

/// A slider row inside the panel: label left, value right (mono), track below.
fn slider(x: f32, y: f32, w: f32, label: &str, value: &str, t: f32, focused: bool) -> String {
    let mut s = String::new();
    s += &text(x, y, 13.0, TEXT, SANS, "400", "start", label);
    s += &text(x + w, y, 12.0, if focused { ACCENT } else { TEXT_2 }, MONO, "400", "end", value);
    let ty = y + 11.0;
    s += &format!(r##"<rect x="{x}" y="{:.1}" width="{w}" height="2" fill="{HAIRLINE}"/>"##, ty - 1.0);
    s += &format!(r##"<rect x="{x}" y="{:.1}" width="{:.1}" height="2" fill="{}"/>"##, ty - 1.0, w * t, if focused { ACCENT } else { TEXT_2 });
    s += &format!(r##"<circle cx="{:.1}" cy="{ty:.1}" r="5" fill="{BG}" stroke="{}" stroke-width="1.5"/>"##, x + w * t, if focused { ACCENT } else { TEXT });
    s
}

fn toggle(x: f32, y: f32, label: &str, on: bool) -> String {
    let mut s = String::new();
    s += &format!(r##"<rect x="{x}" y="{:.1}" width="28" height="16" rx="8" fill="{}"/>"##, y - 11.0, if on { ACCENT } else { HAIRLINE });
    s += &format!(r##"<circle cx="{:.1}" cy="{:.1}" r="6" fill="{}"/>"##, if on { x + 20.0 } else { x + 8.0 }, y - 3.0, if on { BG } else { TEXT_2 });
    s += &text(x + 36.0, y, 13.0, TEXT, SANS, "400", "start", label);
    s
}

fn rail_icon(cx: f32, cy: f32, kind: usize, active: bool) -> String {
    let c = if active { ACCENT } else { TEXT_3 };
    let sw = 1.5;
    match kind {
        0 => format!(r##"<circle cx="{cx}" cy="{cy}" r="6.5" fill="none" stroke="{c}" stroke-width="{sw}"/><circle cx="{cx}" cy="{cy}" r="2" fill="{c}"/>"##),
        1 => format!(r##"<circle cx="{cx}" cy="{cy}" r="7" fill="none" stroke="{c}" stroke-width="{sw}"/><circle cx="{cx}" cy="{cy}" r="3.5" fill="none" stroke="{c}" stroke-width="{sw}"/>"##),
        2 => format!(r##"<circle cx="{cx}" cy="{cy}" r="7" fill="none" stroke="{c}" stroke-width="{sw}"/><path d="M{cx} {:.1} L{cx} {cy} L{:.1} {cy}" fill="none" stroke="{c}" stroke-width="{sw}" stroke-linecap="round"/>"##, cy - 4.5, cx + 3.5),
        3 => format!(r##"<rect x="{:.1}" y="{:.1}" width="15" height="10" rx="1.5" fill="none" stroke="{c}" stroke-width="{sw}"/><line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{c}" stroke-width="{sw}"/>"##, cx - 7.5, cy - 6.5, cx - 4.0, cy + 7.0, cx + 4.0, cy + 7.0),
        4 => format!(r##"<rect x="{:.1}" y="{:.1}" width="3" height="6" fill="{c}"/><rect x="{:.1}" y="{:.1}" width="3" height="13" fill="{c}"/><rect x="{:.1}" y="{:.1}" width="3" height="9" fill="{c}"/>"##, cx - 7.5, cy + 1.0, cx - 1.5, cy - 6.0, cx + 4.5, cy - 2.0),
        _ => format!(r##"<circle cx="{cx}" cy="{cy}" r="7" fill="none" stroke="{c}" stroke-width="{sw}"/><line x1="{cx}" y1="{:.1}" x2="{cx}" y2="{:.1}" stroke="{c}" stroke-width="{sw}" stroke-linecap="round"/><circle cx="{cx}" cy="{:.1}" r="1" fill="{c}"/>"##, cy - 1.0, cy + 3.5, cy - 4.0),
    }
}

/// The main window: chrome bar, starfield scene with title overlay, docked panel on "Post".
pub fn window_svg(w: f32, h: f32) -> String {
    let mut s = String::new();
    let _ = write!(s, r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">"##);
    s += &format!(r##"<clipPath id="win"><rect width="{w}" height="{h}" rx="8"/></clipPath><g clip-path="url(#win)">"##);
    s += &format!(r##"<rect width="{w}" height="{h}" fill="{BG}"/>"##);

    // --- scene: starfield with a lensed ring and the Swiss title overlay ---
    let mut rng = Rng(0x2545_F491);
    for _ in 0..260 {
        let x = rng.next() * w;
        let y = BAR_H + rng.next() * (h - BAR_H);
        let r = 0.4 + rng.next() * rng.next() * 1.6;
        let a = 0.25 + rng.next() * 0.75;
        let warm = rng.next() < 0.3;
        let col = if warm { "#ffe9c8" } else { "#dfe9ff" };
        let _ = write!(s, r##"<circle cx="{x:.1}" cy="{y:.1}" r="{r:.2}" fill="{col}" fill-opacity="{a:.2}"/>"##);
    }
    let (cx, cy) = ((w - PANEL_W) * 0.5, BAR_H + (h - BAR_H) * 0.47);
    let _ = write!(s, r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="92" fill="none" stroke="{ACCENT}" stroke-opacity="0.12" stroke-width="22"/><circle cx="{cx:.1}" cy="{cy:.1}" r="92" fill="none" stroke="{ACCENT}" stroke-opacity="0.55" stroke-width="5"/>"##);
    // title overlay (as in the current starfield scene)
    let ox = 56.0;
    s += &text(ox, BAR_H + 62.0, 12.0, TEXT_2, SANS, "400", "start", "NOW PLAYING  01:01 / 03:22");
    s += &hline(ox, w - PANEL_W - 56.0, h - 150.0, "#9aa5a1");
    s += &text(ox, h - 86.0, 54.0, TEXT, SANS, "700", "start", "Levitate");
    s += &text(ox, h - 54.0, 17.0, "#b7c0bc", SANS, "400", "start", "THE PALMS   ·   Mulholland Dr.");

    // --- docked panel ---
    let px = w - PANEL_W;
    let _ = write!(s, r##"<rect x="{px}" y="{BAR_H}" width="{PANEL_W}" height="{:.1}" fill="{PANEL}" fill-opacity="0.92"/>"##, h - BAR_H);
    let _ = write!(s, r##"<line x1="{px}" y1="{BAR_H}" x2="{px}" y2="{h}" stroke="{HAIRLINE}"/>"##);
    let _ = write!(s, r##"<line x1="{:.1}" y1="{BAR_H}" x2="{:.1}" y2="{h}" stroke="{HAIRLINE}"/>"##, px + RAIL_W, px + RAIL_W);
    // rail
    let labels = ["Scene", "Post", "Sync", "Display", "Meters", "About"];
    for (i, _) in labels.iter().enumerate() {
        let cy = BAR_H + 28.0 + i as f32 * 40.0;
        let active = i == 1;
        if active {
            let _ = write!(s, r##"<rect x="{px}" y="{:.1}" width="2" height="24" fill="{ACCENT}"/>"##, cy - 12.0);
        }
        s += &rail_icon(px + RAIL_W * 0.5, cy, i, active);
    }
    // section
    let cx0 = px + RAIL_W + 20.0;
    let cw = PANEL_W - RAIL_W - 40.0;
    let mut y = BAR_H + 36.0;
    s += &text(cx0, y, 15.0, TEXT, SANS, "600", "start", "Post");
    s += &text(cx0 + 38.0, y, 13.0, TEXT_2, SANS, "400", "start", "· starfield");
    s += &text(cx0 + cw, y, 12.0, TEXT_2, SANS, "400", "end", "Reset");
    y += 16.0;
    s += &hline(cx0, cx0 + cw, y, HAIRLINE);
    y += 32.0;
    s += &slider(cx0, y, cw, "Exposure", "1.00", 0.33, false);
    y += 32.0;
    s += &toggle(cx0, y, "Auto exposure", true);
    y += 36.0;
    s += &slider(cx0, y, cw, "Bloom", "0.12", 0.04, true);
    y += 36.0;
    s += &slider(cx0, y, cw, "Grain", "0.15", 0.15, false);
    y += 34.0;
    s += &text(cx0, y, 11.5, TEXT_3, SANS, "400", "start", "Per scene. Blended across cross-fades.");
    y += 28.0;
    let _ = write!(s, r##"<rect x="{cx0}" y="{:.1}" width="132" height="26" rx="4" fill="none" stroke="{HAIRLINE}"/>"##, y - 17.0);
    s += &text(cx0 + 66.0, y, 12.0, TEXT, SANS, "400", "middle", "Copy to all scenes");
    // footer
    s += &hline(cx0, cx0 + cw, h - 44.0, HAIRLINE);
    s += &text(cx0, h - 26.0, 10.5, TEXT_3, MONO, "400", "start", "Tab panel   1-3 scene   C calibrate");
    s += &text(cx0, h - 12.0, 10.5, TEXT_3, MONO, "400", "start", "F11 fullscreen   Esc quit");
    s += &text(cx0 + cw, h - 12.0, 10.0, TEXT_3, MONO, "400", "end", "0.2.0");

    // --- chrome bar (drawn last, over everything) ---
    let _ = write!(s, r##"<rect width="{w}" height="{BAR_H}" fill="{BG}"/>"##);
    s += &hline(0.0, w, BAR_H - 0.5, HAIRLINE);
    s += &mark(14.0 + 8.0, BAR_H * 0.5, 7.0, false);
    s += &text(36.0, 18.5, 13.0, TEXT, SANS, "600", "start", "audioviz");
    // scene pips, centred on the window
    let pips = ["1  scope", "2  ribbons", "3  starfield"];
    let pw = 84.0;
    let x0 = w * 0.5 - pw * 1.5;
    for (i, p) in pips.iter().enumerate() {
        let active = i == 2;
        let x = x0 + i as f32 * pw;
        if active {
            let _ = write!(s, r##"<rect x="{:.1}" y="{:.1}" width="{pw}" height="2" fill="{ACCENT}"/>"##, x, BAR_H - 2.0);
        }
        s += &text(x + pw * 0.5, 18.5, 12.0, if active { TEXT } else { TEXT_2 }, SANS, if active { "600" } else { "400" }, "middle", p);
    }
    // now playing + window controls
    let ctrl_w = 46.0;
    let right = w - ctrl_w * 3.0 - 16.0;
    s += &text(right, 18.5, 12.0, TEXT_2, SANS, "400", "end", "The Palms — Levitate");
    for i in 0..3 {
        let cx = w - ctrl_w * (3.0 - i as f32) + ctrl_w * 0.5;
        let cy = BAR_H * 0.5;
        let g = match i {
            0 => format!(r##"<line x1="{:.1}" y1="{cy}" x2="{:.1}" y2="{cy}" stroke="{TEXT_2}" stroke-width="1"/>"##, cx - 5.0, cx + 5.0),
            1 => format!(r##"<rect x="{:.1}" y="{:.1}" width="10" height="10" fill="none" stroke="{TEXT_2}" stroke-width="1"/>"##, cx - 5.0, cy - 5.0),
            _ => format!(r##"<path d="M{:.1} {:.1} L{:.1} {:.1} M{:.1} {:.1} L{:.1} {:.1}" stroke="{TEXT_2}" stroke-width="1"/>"##, cx - 5.0, cy - 5.0, cx + 5.0, cy + 5.0, cx + 5.0, cy - 5.0, cx - 5.0, cy + 5.0),
        };
        s += &g;
    }
    s += "</g>";
    let _ = write!(s, r##"<rect x="0.5" y="0.5" width="{:.1}" height="{:.1}" rx="8" fill="none" stroke="{HAIRLINE}"/>"##, w - 1.0, h - 1.0);
    s += "</svg>";
    s
}

/// Installer page as Inno Setup's modern wizard can be made to look: dark
/// image strip with the mark, a single quiet page, one accent button.
pub fn installer_svg() -> String {
    let (w, h) = (620.0, 440.0);
    let strip = 180.0;
    let mut s = String::new();
    let _ = write!(s, r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">"##);
    let _ = write!(s, r##"<rect width="{w}" height="{h}" rx="8" fill="#ffffff"/>"##);
    let _ = write!(s, r##"<clipPath id="strip"><rect width="{strip}" height="{h}" rx="8"/><rect x="{:.1}" width="{:.1}" height="{h}"/></clipPath>"##, strip * 0.5, strip * 0.5);
    let _ = write!(s, r##"<rect width="{strip}" height="{h}" fill="{BG}" clip-path="url(#strip)"/>"##);
    s += &mark(strip * 0.5, 150.0, 42.0, false);
    s += &text(strip * 0.5, 228.0, 15.0, TEXT, SANS, "600", "middle", "audioviz");
    s += &text(strip * 0.5, 248.0, 11.0, TEXT_2, SANS, "400", "middle", "0.2.0");
    s += &text(strip * 0.5, h - 22.0, 10.0, TEXT_3, MONO, "400", "middle", "MIT · no telemetry");
    let x = strip + 40.0;
    let cw = w - strip - 80.0;
    s += &text(x, 72.0, 22.0, "#111517", SANS, "600", "start", "Install audioviz");
    s += &text(x, 98.0, 13.0, "#5d6966", SANS, "400", "start", "For this user only. No administrator rights needed.");
    s += &text(x, 150.0, 11.0, "#8a9491", SANS, "600", "start", "LOCATION");
    s += &text(x, 172.0, 13.0, "#111517", MONO, "400", "start", "%LOCALAPPDATA%\\Programs\\audioviz");
    s += &format!(r##"<line x1="{x}" y1="182" x2="{:.1}" y2="182" stroke="#e3e7e6"/>"##, x + cw);
    s += &text(x, 222.0, 11.0, "#8a9491", SANS, "600", "start", "OPTIONS");
    for (i, (label, on)) in [("Start Menu shortcut", true), ("Desktop shortcut", false), ("Start audioviz when I sign in", false)].iter().enumerate() {
        let y = 248.0 + i as f32 * 30.0;
        let _ = write!(s, r##"<rect x="{x}" y="{:.1}" width="16" height="16" rx="3" fill="{}" stroke="{}"/>"##, y - 12.0, if *on { ACCENT } else { "#ffffff" }, if *on { ACCENT } else { "#c9d0ce" });
        if *on {
            let _ = write!(s, r##"<path d="M{:.1} {:.1} L{:.1} {:.1} L{:.1} {:.1}" fill="none" stroke="{BG}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"##, x + 3.5, y - 4.0, x + 7.0, y - 0.5, x + 12.5, y - 8.0);
        }
        s += &text(x + 26.0, y, 13.0, "#111517", SANS, "400", "start", label);
    }
    s += &text(x, h - 62.0, 11.0, "#8a9491", SANS, "400", "start", "6 MB · Windows 10 2004 or later · unsigned: SmartScreen may ask once");
    let _ = write!(s, r##"<rect x="{:.1}" y="{:.1}" width="96" height="32" rx="4" fill="{BG}"/>"##, w - 40.0 - 96.0, h - 36.0 - 10.0);
    s += &text(w - 40.0 - 48.0, h - 36.0 + 10.0, 13.0, ACCENT, SANS, "600", "middle", "Install");
    s += &text(w - 40.0 - 96.0 - 24.0, h - 36.0 + 10.0, 13.0, "#5d6966", SANS, "400", "end", "Cancel");
    let _ = write!(s, r##"<rect x="0.5" y="0.5" width="{:.1}" height="{:.1}" rx="8" fill="none" stroke="#d5dbd9"/>"##, w - 1.0, h - 1.0);
    s += "</svg>";
    s
}

pub fn render_png(svg: &str, scale: f32, out: &Path) -> anyhow::Result<()> {
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    opt.font_family = "Segoe UI".into();
    let tree = resvg::usvg::Tree::from_str(svg, &opt).context("parse svg")?;
    let (w, h) = ((tree.size().width() * scale) as u32, (tree.size().height() * scale) as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).context("pixmap")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.save_png(out).context("save png")?;
    Ok(())
}
