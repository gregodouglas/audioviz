//! `cargo xtask mockup`: draws the Phase 6 UI direction (docs/phase6-ui.md) as
//! SVG from the design tokens below and renders PNGs into docs/img/. The point
//! is a set of pictures Greg can react to before the window changes; the
//! tokens here are the ones the egui style will use.
//!
//! Frames: panel open · hover (bar + transport strip) · ambient (nothing) ·
//! empty state (Spotify not running) · calibration · installer (two pages).

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
/// Accent derived from the album art (the "tint from art" option); used in the hover frame.
pub const ART_ACCENT: &str = "#f2b26b";
pub const SANS: &str = "Segoe UI, Helvetica Neue, Arial, sans-serif";
pub const MONO: &str = "Cascadia Mono, Consolas, SF Mono, monospace";
pub const BAR_H: f32 = 28.0;
pub const STRIP_H: f32 = 72.0;
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

fn mark(cx: f32, cy: f32, r: f32, color: &str, glow: bool) -> String {
    let p = lissajous(cx, cy, r);
    let w = r / 9.0;
    let mut s = String::new();
    if glow {
        let _ = write!(s, r##"<path d="{p}" fill="none" stroke="{color}" stroke-opacity="0.35" stroke-width="{:.2}" stroke-linejoin="round"/>"##, w * 3.0);
    }
    let _ = write!(s, r##"<path d="{p}" fill="none" stroke="{color}" stroke-width="{w:.2}" stroke-linejoin="round"/>"##);
    s
}

fn text(x: f32, y: f32, size: f32, color: &str, family: &str, weight: &str, anchor: &str, s: &str) -> String {
    let s = s.replace('&', "&amp;").replace('<', "&lt;");
    format!(r##"<text x="{x:.1}" y="{y:.1}" font-family="{family}" font-size="{size}" font-weight="{weight}" fill="{color}" text-anchor="{anchor}">{s}</text>"##)
}

fn hline(x0: f32, x1: f32, y: f32, color: &str) -> String {
    format!(r##"<line x1="{x0:.1}" y1="{y:.1}" x2="{x1:.1}" y2="{y:.1}" stroke="{color}" stroke-width="1"/>"##)
}

fn rect(x: f32, y: f32, w: f32, h: f32, rx: f32, fill: &str, opacity: f32) -> String {
    format!(r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="{rx}" fill="{fill}" fill-opacity="{opacity}"/>"##)
}

fn ghost_button(x: f32, y: f32, w: f32, label: &str, accent: bool) -> String {
    let mut s = String::new();
    let _ = write!(s, r##"<rect x="{x}" y="{:.1}" width="{w}" height="26" rx="4" fill="none" stroke="{}"/>"##, y - 17.0, if accent { ACCENT } else { HAIRLINE });
    s += &text(x + w * 0.5, y, 12.0, if accent { ACCENT } else { TEXT }, SANS, "400", "middle", label);
    s
}

fn chip(x: f32, y: f32, label: &str, color: &str) -> (String, f32) {
    let w = 7.0 * label.len() as f32 + 18.0;
    let mut s = String::new();
    let _ = write!(s, r##"<rect x="{x}" y="{:.1}" width="{w:.1}" height="20" rx="10" fill="{color}" fill-opacity="0.12"/>"##, y - 14.0);
    s += &text(x + w * 0.5, y, 11.0, color, MONO, "400", "middle", label);
    (s, w)
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
        5 => format!(r##"<rect x="{:.1}" y="{:.1}" width="14" height="14" rx="2" fill="none" stroke="{c}" stroke-width="{sw}"/><path d="M{:.1} {cy} L{:.1} {cy}" stroke="{c}" stroke-width="{sw}"/>"##, cx - 7.0, cy - 7.0, cx - 3.0, cx + 3.0),
        _ => format!(r##"<circle cx="{cx}" cy="{cy}" r="7" fill="none" stroke="{c}" stroke-width="{sw}"/><line x1="{cx}" y1="{:.1}" x2="{cx}" y2="{:.1}" stroke="{c}" stroke-width="{sw}" stroke-linecap="round"/><circle cx="{cx}" cy="{:.1}" r="1" fill="{c}"/>"##, cy - 1.0, cy + 3.5, cy - 4.0),
    }
}

// ---- scene stand-ins ------------------------------------------------------

fn starfield(s: &mut String, w: f32, h: f32, ring: bool, accent: &str) {
    let mut rng = Rng(0x2545_F491);
    for _ in 0..260 {
        let x = rng.next() * w;
        let y = rng.next() * h;
        let r = 0.4 + rng.next() * rng.next() * 1.6;
        let a = 0.25 + rng.next() * 0.75;
        let col = if rng.next() < 0.3 { "#ffe9c8" } else { "#dfe9ff" };
        let _ = write!(s, r##"<circle cx="{x:.1}" cy="{y:.1}" r="{r:.2}" fill="{col}" fill-opacity="{a:.2}"/>"##);
    }
    if ring {
        let (cx, cy) = (w * 0.5, h * 0.47);
        let _ = write!(s, r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="92" fill="none" stroke="{accent}" stroke-opacity="0.12" stroke-width="22"/><circle cx="{cx:.1}" cy="{cy:.1}" r="92" fill="none" stroke="{accent}" stroke-opacity="0.55" stroke-width="5"/>"##);
    }
}

/// The starfield's Swiss title block. `bottom` is the reserved space under it
/// (the transport strip when it is showing; the overlay slides up with it).
fn title_overlay(s: &mut String, w: f32, h: f32, right_margin: f32, bottom: f32) {
    let ox = 56.0;
    let h = h - bottom;
    *s += &text(ox, 90.0, 12.0, TEXT_2, SANS, "400", "start", "NOW PLAYING  01:01 / 03:22");
    *s += &hline(ox, w - right_margin - 56.0, h - 150.0, "#9aa5a1");
    *s += &text(ox, h - 86.0, 54.0, TEXT, SANS, "700", "start", "Levitate");
    *s += &text(ox, h - 54.0, 17.0, "#b7c0bc", SANS, "400", "start", "THE PALMS   ·   Mulholland Dr.");
}

// ---- chrome ----------------------------------------------------------------

fn chrome_bar(s: &mut String, w: f32, pips: bool, accent: &str) {
    chrome_bar_with(s, w, pips, accent, ("spotify", accent));
}

fn chrome_bar_with(s: &mut String, w: f32, pips: bool, accent: &str, capture: (&str, &str)) {
    let _ = write!(s, r##"<rect width="{w}" height="{BAR_H}" fill="{BG}"/>"##);
    *s += &hline(0.0, w, BAR_H - 0.5, HAIRLINE);
    *s += &mark(22.0, BAR_H * 0.5, 7.0, accent, true);
    *s += &text(36.0, 18.5, 13.0, TEXT, SANS, "600", "start", "audioviz");
    if pips {
        let names = ["1  scope", "2  ribbons", "3  starfield"];
        let pw = 84.0;
        let x0 = w * 0.5 - pw * 1.5;
        for (i, p) in names.iter().enumerate() {
            let active = i == 2;
            let x = x0 + i as f32 * pw;
            if active {
                let _ = write!(s, r##"<rect x="{:.1}" y="{:.1}" width="{pw}" height="2" fill="{accent}"/>"##, x, BAR_H - 2.0);
            }
            *s += &text(x + pw * 0.5, 18.5, 12.0, if active { TEXT } else { TEXT_2 }, SANS, if active { "600" } else { "400" }, "middle", p);
        }
        // "A" auto pip after the scenes
        *s += &text(x0 + 3.0 * pw + 22.0, 18.5, 12.0, TEXT_3, SANS, "400", "middle", "A  auto");
    }
    let ctrl_w = 46.0;
    for i in 0..3 {
        let cx = w - ctrl_w * (3.0 - i as f32) + ctrl_w * 0.5;
        let cy = BAR_H * 0.5;
        let g = match i {
            0 => format!(r##"<line x1="{:.1}" y1="{cy}" x2="{:.1}" y2="{cy}" stroke="{TEXT_2}" stroke-width="1"/>"##, cx - 5.0, cx + 5.0),
            1 => format!(r##"<rect x="{:.1}" y="{:.1}" width="10" height="10" fill="none" stroke="{TEXT_2}" stroke-width="1"/>"##, cx - 5.0, cy - 5.0),
            _ => format!(r##"<path d="M{:.1} {:.1} L{:.1} {:.1} M{:.1} {:.1} L{:.1} {:.1}" stroke="{TEXT_2}" stroke-width="1"/>"##, cx - 5.0, cy - 5.0, cx + 5.0, cy + 5.0, cx + 5.0, cy - 5.0, cx - 5.0, cy + 5.0),
        };
        *s += &g;
    }
    // status chips left of the controls: monitor, sync, capture
    let mut x = w - ctrl_w * 3.0 - 16.0;
    for (label, color) in [capture, ("sync 38 ms", TEXT_2), ("M2 60 fps", TEXT_2)].iter().rev() {
        let (c, cw) = chip(x - 7.0 * label.len() as f32 - 18.0, 18.5, label, color);
        *s += &c;
        x -= cw + 8.0;
    }
}

/// Bottom "transport" strip: album art, track, controls, progress. Appears on
/// hover together with the bar; also for 4 s on track change.
fn transport_strip(s: &mut String, w: f32, h: f32, accent: &str) {
    let y0 = h - STRIP_H;
    *s += &rect(0.0, y0, w, STRIP_H, 0.0, BG, 0.88);
    *s += &hline(0.0, w, y0 + 0.5, HAIRLINE);
    // album art
    *s += &rect(16.0, y0 + 12.0, 48.0, 48.0, 4.0, "#2a2320", 1.0);
    *s += &rect(16.0, y0 + 12.0, 48.0, 48.0, 4.0, ART_ACCENT, 0.25);
    *s += &text(40.0, y0 + 42.0, 9.0, "#e8d8c0", SANS, "700", "middle", "ART");
    *s += &text(78.0, y0 + 31.0, 15.0, TEXT, SANS, "600", "start", "Levitate");
    *s += &text(78.0, y0 + 50.0, 12.0, TEXT_2, SANS, "400", "start", "The Palms  ·  Mulholland Dr.");
    // controls, centred
    let cx = w * 0.5;
    let cy = y0 + 30.0;
    let _ = write!(s, r##"<path d="M{:.1} {:.1} L{:.1} {cy} L{:.1} {:.1} Z M{:.1} {:.1} L{:.1} {:.1}" fill="{TEXT_2}" stroke="{TEXT_2}" stroke-width="1.5"/>"##, cx - 34.0, cy - 6.0, cx - 44.0, cx - 34.0, cy + 6.0, cx - 46.0, cy - 6.0, cx - 46.0, cy + 6.0);
    let _ = write!(s, r##"<circle cx="{cx}" cy="{cy}" r="15" fill="{TEXT}"/><rect x="{:.1}" y="{:.1}" width="3" height="11" fill="{BG}"/><rect x="{:.1}" y="{:.1}" width="3" height="11" fill="{BG}"/>"##, cx - 5.0, cy - 5.5, cx + 2.0, cy - 5.5);
    let _ = write!(s, r##"<path d="M{:.1} {:.1} L{:.1} {cy} L{:.1} {:.1} Z M{:.1} {:.1} L{:.1} {:.1}" fill="{TEXT_2}" stroke="{TEXT_2}" stroke-width="1.5"/>"##, cx + 34.0, cy - 6.0, cx + 44.0, cx + 34.0, cy + 6.0, cx + 46.0, cy - 6.0, cx + 46.0, cy + 6.0);
    // progress
    let (px0, px1) = (cx - 220.0, cx + 220.0);
    *s += &text(px0 - 10.0, y0 + 58.0, 11.0, TEXT_2, MONO, "400", "end", "01:01");
    *s += &text(px1 + 10.0, y0 + 58.0, 11.0, TEXT_2, MONO, "400", "start", "03:22");
    let _ = write!(s, r##"<rect x="{px0}" y="{:.1}" width="{:.1}" height="2" fill="{HAIRLINE}"/><rect x="{px0}" y="{:.1}" width="{:.1}" height="2" fill="{accent}"/>"##, y0 + 54.0, px1 - px0, y0 + 54.0, (px1 - px0) * 0.3);
    // right: tint-from-art, like, scene thumbs
    let rx = w - 16.0;
    *s += &text(rx, y0 + 31.0, 11.0, TEXT_2, SANS, "400", "end", "tint from album art");
    let _ = write!(s, r##"<rect x="{:.1}" y="{:.1}" width="28" height="16" rx="8" fill="{accent}"/><circle cx="{:.1}" cy="{:.1}" r="6" fill="{BG}"/>"##, rx - 150.0, y0 + 19.0, rx - 130.0, y0 + 27.0);
    *s += &text(rx, y0 + 52.0, 11.0, TEXT_3, SANS, "400", "end", "?  shortcuts");
}

fn panel(s: &mut String, w: f32, h: f32) {
    let px = w - PANEL_W;
    let _ = write!(s, r##"<rect x="{px}" y="{BAR_H}" width="{PANEL_W}" height="{:.1}" fill="{PANEL}" fill-opacity="0.92"/>"##, h - BAR_H);
    let _ = write!(s, r##"<line x1="{px}" y1="{BAR_H}" x2="{px}" y2="{h}" stroke="{HAIRLINE}"/>"##);
    let _ = write!(s, r##"<line x1="{:.1}" y1="{BAR_H}" x2="{:.1}" y2="{h}" stroke="{HAIRLINE}"/>"##, px + RAIL_W, px + RAIL_W);
    for i in 0..7 {
        let cy = BAR_H + 28.0 + i as f32 * 40.0;
        let active = i == 1;
        if active {
            let _ = write!(s, r##"<rect x="{px}" y="{:.1}" width="2" height="24" fill="{ACCENT}"/>"##, cy - 12.0);
        }
        *s += &rail_icon(px + RAIL_W * 0.5, cy, i, active);
    }
    let cx0 = px + RAIL_W + 20.0;
    let cw = PANEL_W - RAIL_W - 40.0;
    let mut y = BAR_H + 36.0;
    *s += &text(cx0, y, 15.0, TEXT, SANS, "600", "start", "Post");
    *s += &text(cx0 + 38.0, y, 13.0, TEXT_2, SANS, "400", "start", "· starfield");
    *s += &text(cx0 + cw, y, 12.0, TEXT_2, SANS, "400", "end", "Reset");
    y += 16.0;
    *s += &hline(cx0, cx0 + cw, y, HAIRLINE);
    y += 32.0;
    *s += &slider(cx0, y, cw, "Exposure", "1.00", 0.33, false);
    y += 32.0;
    *s += &toggle(cx0, y, "Auto exposure", true);
    y += 36.0;
    *s += &slider(cx0, y, cw, "Bloom", "0.12", 0.04, true);
    y += 36.0;
    *s += &slider(cx0, y, cw, "Grain", "0.15", 0.15, false);
    y += 34.0;
    *s += &text(cx0, y, 11.5, TEXT_3, SANS, "400", "start", "Per scene. Blended across cross-fades.");
    y += 28.0;
    *s += &ghost_button(cx0, y, 132.0, "Copy to all scenes", false);
    y += 44.0;
    *s += &hline(cx0, cx0 + cw, y - 20.0, HAIRLINE);
    *s += &text(cx0, y, 11.0, TEXT_2, SANS, "600", "start", "LOOKS");
    y += 26.0;
    for (i, name) in ["Night", "Party", "Studio"].iter().enumerate() {
        let x = cx0 + i as f32 * 84.0;
        let active = i == 0;
        let _ = write!(s, r##"<rect x="{x}" y="{:.1}" width="76" height="26" rx="4" fill="{}" fill-opacity="{}" stroke="{}"/>"##, y - 17.0, ACCENT, if active { 0.12 } else { 0.0 }, if active { ACCENT } else { HAIRLINE });
        *s += &text(x + 38.0, y, 12.0, if active { ACCENT } else { TEXT }, SANS, "400", "middle", name);
    }
    y += 26.0;
    *s += &text(cx0, y, 11.0, TEXT_3, SANS, "400", "start", "Save current as a look  ·  Export  ·  Import");
    // footer
    *s += &hline(cx0, cx0 + cw, h - 44.0, HAIRLINE);
    *s += &text(cx0, h - 26.0, 10.5, TEXT_3, MONO, "400", "start", "Tab panel   1-3 scene   C calibrate");
    *s += &text(cx0, h - 12.0, 10.5, TEXT_3, MONO, "400", "start", "F11 fullscreen   ?  all shortcuts");
    *s += &text(cx0 + cw, h - 12.0, 10.0, TEXT_3, MONO, "400", "end", "0.2.0");
}

fn frame_open(w: f32, h: f32) -> String {
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}"><clipPath id="win"><rect width="{w}" height="{h}" rx="8"/></clipPath><g clip-path="url(#win)"><rect width="{w}" height="{h}" fill="{BG}"/>"##)
}

fn frame_close(s: &mut String, w: f32, h: f32) {
    *s += "</g>";
    let _ = write!(s, r##"<rect x="0.5" y="0.5" width="{:.1}" height="{:.1}" rx="8" fill="none" stroke="{HAIRLINE}"/></svg>"##, w - 1.0, h - 1.0);
}

// ---- frames ----------------------------------------------------------------

/// 1. Panel open on Post, bar visible.
pub fn frame_panel(w: f32, h: f32) -> String {
    let mut s = frame_open(w, h);
    starfield(&mut s, w, h, true, ACCENT);
    title_overlay(&mut s, w, h, PANEL_W, 0.0);
    panel(&mut s, w, h);
    chrome_bar(&mut s, w, true, ACCENT);
    frame_close(&mut s, w, h);
    s
}

/// 2. Hover: bar + transport strip, no panel. Tint follows the album art.
pub fn frame_hover(w: f32, h: f32) -> String {
    let mut s = frame_open(w, h);
    starfield(&mut s, w, h, true, ART_ACCENT);
    title_overlay(&mut s, w, h, 0.0, STRIP_H + 8.0);
    transport_strip(&mut s, w, h, ART_ACCENT);
    chrome_bar(&mut s, w, true, ART_ACCENT);
    frame_close(&mut s, w, h);
    s
}

/// 3. Ambient: nothing but the scene (mouse idle or fullscreen). Only a
/// 4-second track-change toast, bottom-left, in the scenes without a title overlay.
pub fn frame_ambient(w: f32, h: f32) -> String {
    let mut s = frame_open(w, h);
    // ribbons stand-in: a few flow lines
    let mut rng = Rng(0x1234_5678);
    for _ in 0..70 {
        let x0 = rng.next() * w;
        let y0 = rng.next() * h;
        let mut d = format!("M{x0:.1} {y0:.1} ");
        let (mut x, mut y) = (x0, y0);
        for k in 0..8 {
            let a = ((x * 0.004).sin() + (y * 0.005).cos()) * 1.6 + k as f32 * 0.1;
            x += a.cos() * 28.0;
            y += a.sin() * 28.0;
            let _ = write!(d, "L{x:.1} {y:.1} ");
        }
        let a = 0.08 + rng.next() * 0.35;
        let _ = write!(s, r##"<path d="{d}" fill="none" stroke="{ACCENT}" stroke-opacity="{a:.2}" stroke-width="{:.1}" stroke-linecap="round" stroke-linejoin="round"/>"##, 0.6 + rng.next() * 1.8);
    }
    // track-change toast
    let (tx, ty) = (24.0, h - 24.0);
    s += &rect(tx, ty - 46.0, 300.0, 46.0, 4.0, BG, 0.75);
    s += &rect(tx + 8.0, ty - 38.0, 30.0, 30.0, 3.0, ART_ACCENT, 0.35);
    s += &text(tx + 48.0, ty - 25.0, 13.0, TEXT, SANS, "600", "start", "Levitate");
    s += &text(tx + 48.0, ty - 10.0, 11.0, TEXT_2, SANS, "400", "start", "The Palms");
    frame_close(&mut s, w, h);
    s
}

/// 4. Empty state: Spotify not running.
pub fn frame_empty(w: f32, h: f32) -> String {
    let mut s = frame_open(w, h);
    starfield(&mut s, w, h, false, ACCENT);
    let (cx, cy) = (w * 0.5, h * 0.44);
    s += &mark(cx, cy - 70.0, 34.0, TEXT_3, false);
    s += &text(cx, cy + 10.0, 19.0, TEXT, SANS, "600", "middle", "Nothing to listen to yet");
    s += &text(cx, cy + 34.0, 13.0, TEXT_2, SANS, "400", "middle", "Start Spotify and play something. Until then this shows the default output:");
    s += &text(cx, cy + 54.0, 13.0, TEXT_2, MONO, "400", "middle", "Speakers (Realtek USB Audio)");
    s += &ghost_button(cx - 128.0, cy + 100.0, 120.0, "Open Spotify", true);
    s += &ghost_button(cx + 8.0, cy + 100.0, 120.0, "Pick another app", false);
    chrome_bar_with(&mut s, w, false, ACCENT, ("endpoint", TEXT_2));
    frame_close(&mut s, w, h);
    s
}

/// 5. Calibration overlay.
pub fn frame_calibrate(w: f32, h: f32) -> String {
    let mut s = frame_open(w, h);
    starfield(&mut s, w, h, false, ACCENT);
    s += &rect(0.0, 0.0, w, h, 0.0, BG, 0.7);
    let (cx, cy) = (w * 0.5, h * 0.44);
    let _ = write!(s, r##"<circle cx="{cx}" cy="{cy:.1}" r="74" fill="none" stroke="{ACCENT}" stroke-opacity="0.18" stroke-width="18"/><circle cx="{cx}" cy="{cy:.1}" r="74" fill="none" stroke="{ACCENT}" stroke-width="2"/>"##);
    s += &text(cx, cy + 7.0, 22.0, TEXT, SANS, "600", "middle", "Tap Space");
    s += &text(cx, cy + 118.0, 15.0, TEXT_2, SANS, "400", "middle", "on each beat you hear. Ten is plenty.");
    // tap dots
    let n = 10;
    let x0 = cx - (n as f32 - 1.0) * 14.0 * 0.5;
    for i in 0..n {
        let (fill, op) = if i < 6 { (ACCENT, 1.0) } else if i == 6 { (TEXT_2, 1.0) } else { (HAIRLINE, 1.0) };
        let _ = write!(s, r##"<circle cx="{:.1}" cy="{:.1}" r="4" fill="{fill}" fill-opacity="{op}"/>"##, x0 + i as f32 * 14.0, cy + 150.0);
    }
    s += &text(cx, cy + 184.0, 13.0, TEXT, MONO, "400", "middle", "7 taps · 6 matched · offset +34 ms · ±5 ms");
    s += &text(cx, cy + 208.0, 12.0, ACCENT, SANS, "400", "middle", "Looking good. Press Enter to apply, or keep tapping.");
    s += &text(cx, h - 40.0, 11.0, TEXT_3, MONO, "400", "middle", "Enter apply · C cancel · current delay 0 ms");
    chrome_bar(&mut s, w, true, ACCENT);
    frame_close(&mut s, w, h);
    s
}

// ---- installer -------------------------------------------------------------

fn installer_frame(title: &str, subtitle: &str, body: &dyn Fn(&mut String, f32, f32), primary: &str, secondary: Option<&str>) -> String {
    let (w, h) = (620.0, 520.0);
    let strip = 180.0;
    let mut s = String::new();
    let _ = write!(s, r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">"##);
    let _ = write!(s, r##"<rect width="{w}" height="{h}" rx="8" fill="#ffffff"/>"##);
    let _ = write!(s, r##"<clipPath id="strip"><rect width="{strip}" height="{h}" rx="8"/><rect x="{:.1}" width="{:.1}" height="{h}"/></clipPath>"##, strip * 0.5, strip * 0.5);
    let _ = write!(s, r##"<rect width="{strip}" height="{h}" fill="{BG}" clip-path="url(#strip)"/>"##);
    s += &mark(strip * 0.5, 150.0, 42.0, ACCENT, true);
    s += &text(strip * 0.5, 228.0, 15.0, TEXT, SANS, "600", "middle", "audioviz");
    s += &text(strip * 0.5, 248.0, 11.0, TEXT_2, SANS, "400", "middle", "0.2.0");
    s += &text(strip * 0.5, h - 22.0, 10.0, TEXT_3, MONO, "400", "middle", "MIT · no telemetry");
    let x = strip + 40.0;
    let cw = w - strip - 80.0;
    s += &text(x, 72.0, 22.0, "#111517", SANS, "600", "start", title);
    s += &text(x, 98.0, 13.0, "#5d6966", SANS, "400", "start", subtitle);
    body(&mut s, x, cw);
    let _ = write!(s, r##"<rect x="{:.1}" y="{:.1}" width="96" height="32" rx="4" fill="{BG}"/>"##, w - 40.0 - 96.0, h - 46.0);
    s += &text(w - 40.0 - 48.0, h - 26.0, 13.0, ACCENT, SANS, "600", "middle", primary);
    if let Some(sec) = secondary {
        s += &text(w - 40.0 - 96.0 - 24.0, h - 26.0, 13.0, "#5d6966", SANS, "400", "end", sec);
    }
    let _ = write!(s, r##"<rect x="0.5" y="0.5" width="{:.1}" height="{:.1}" rx="8" fill="none" stroke="#d5dbd9"/>"##, w - 1.0, h - 1.0);
    s += "</svg>";
    s
}

fn checkbox(s: &mut String, x: f32, y: f32, label: &str, on: bool) {
    let _ = write!(s, r##"<rect x="{x}" y="{:.1}" width="16" height="16" rx="3" fill="{}" stroke="{}"/>"##, y - 12.0, if on { ACCENT } else { "#ffffff" }, if on { ACCENT } else { "#c9d0ce" });
    if on {
        let _ = write!(s, r##"<path d="M{:.1} {:.1} L{:.1} {:.1} L{:.1} {:.1}" fill="none" stroke="{BG}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"##, x + 3.5, y - 4.0, x + 7.0, y - 0.5, x + 12.5, y - 8.0);
    }
    *s += &text(x + 26.0, y, 13.0, "#111517", SANS, "400", "start", label);
}

fn check_row(s: &mut String, x: f32, y: f32, label: &str, ok: bool) {
    let c = if ok { "#1f9d55" } else { "#c2410c" };
    let _ = write!(s, r##"<circle cx="{:.1}" cy="{:.1}" r="7" fill="{c}" fill-opacity="0.15"/>"##, x + 8.0, y - 4.0);
    if ok {
        let _ = write!(s, r##"<path d="M{:.1} {:.1} L{:.1} {:.1} L{:.1} {:.1}" fill="none" stroke="{c}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>"##, x + 4.5, y - 4.0, x + 7.0, y - 1.5, x + 11.5, y - 7.0);
    } else {
        *s += &text(x + 8.0, y, 11.0, c, SANS, "700", "middle", "!");
    }
    *s += &text(x + 24.0, y, 12.5, "#111517", SANS, "400", "start", label);
}

/// Install page: checks, location, options; upgrade note when applicable.
pub fn installer_install_svg() -> String {
    installer_frame(
        "Install audioviz",
        "For this user only. No administrator rights needed.",
        &|s, x, cw| {
            // upgrade banner
            let _ = write!(s, r##"<rect x="{x}" y="112" width="{cw}" height="28" rx="4" fill="{ACCENT}" fill-opacity="0.12"/>"##);
            *s += &text(x + 12.0, 130.0, 12.0, "#1f6b3a", SANS, "400", "start", "Upgrading 0.1.0 → 0.2.0. Your settings are kept.");
            *s += &text(x, 168.0, 11.0, "#8a9491", SANS, "600", "start", "CHECKS");
            check_row(s, x, 190.0, "Windows 11, build 26200", true);
            check_row(s, x, 212.0, "DirectX 12 GPU: NVIDIA GeForce RTX 2070 SUPER", true);
            check_row(s, x, 234.0, "Spotify installed · output: Speakers (Realtek USB Audio)", true);
            *s += &text(x, 276.0, 11.0, "#8a9491", SANS, "600", "start", "LOCATION");
            *s += &text(x, 298.0, 13.0, "#111517", MONO, "400", "start", "%LOCALAPPDATA%\\Programs\\audioviz");
            *s += &text(x + cw, 298.0, 11.0, "#8a9491", SANS, "400", "end", "Change");
            let _ = write!(s, r##"<line x1="{x}" y1="308" x2="{:.1}" y2="308" stroke="#e3e7e6"/>"##, x + cw);
            *s += &text(x, 344.0, 11.0, "#8a9491", SANS, "600", "start", "OPTIONS");
            checkbox(s, x, 370.0, "Start Menu shortcut", true);
            checkbox(s, x + 190.0, 370.0, "Desktop shortcut", false);
            checkbox(s, x, 398.0, "Start when I sign in, minimised to the tray", false);
            *s += &text(x, 436.0, 11.0, "#8a9491", SANS, "400", "start", "6 MB · unsigned, SmartScreen may ask once · SHA-256 on the release page");
        },
        "Install",
        Some("Cancel"),
    )
}

/// Finish page: what happened, launch options, one tip.
pub fn installer_finish_svg() -> String {
    installer_frame(
        "Installed",
        "audioviz 0.2.0 is ready.",
        &|s, x, cw| {
            check_row(s, x, 140.0, "Installed to %LOCALAPPDATA%\\Programs\\audioviz", true);
            check_row(s, x, 162.0, "Start Menu shortcut added", true);
            check_row(s, x, 184.0, "Settings kept from 0.1.0", true);
            *s += &text(x, 232.0, 11.0, "#8a9491", SANS, "600", "start", "NOW");
            checkbox(s, x, 258.0, "Launch audioviz", true);
            checkbox(s, x, 286.0, "Open Spotify too", true);
            checkbox(s, x, 314.0, "Start fullscreen on my second monitor", false);
            let _ = write!(s, r##"<rect x="{x}" y="348" width="{cw}" height="70" rx="4" fill="#f3f6f5"/>"##);
            *s += &text(x + 12.0, 368.0, 11.0, "#8a9491", SANS, "600", "start", "TIP");
            *s += &text(x + 12.0, 388.0, 12.5, "#111517", SANS, "400", "start", "Tab opens settings. C calibrates the beat to your speakers.");
            *s += &text(x + 12.0, 406.0, 12.5, "#111517", SANS, "400", "start", "F11 goes fullscreen. ? shows every shortcut.");
        },
        "Finish",
        None,
    )
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
