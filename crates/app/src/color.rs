//! Colour engine: palettes as colour stops, baked into a 32-entry table in
//! linear RGB with OKLab interpolation (so a gradient between two saturated
//! colours never dips through grey). Presets ship here; a layer picks one or
//! edits its own. Consumers sample the table by a 0..1 parameter plus a
//! cyclic shift that the director moves with the music.

use serde::{Deserialize, Serialize};

/// One colour stop: position 0..1 and sRGB colour 0..1 (what the user sees in a picker).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    pub t: f32,
    pub rgb: [f32; 3],
}

impl Stop {
    pub const fn hex(t: f32, hex: u32) -> Self {
        Self { t, rgb: [((hex >> 16) & 0xff) as f32 / 255.0, ((hex >> 8) & 0xff) as f32 / 255.0, (hex & 0xff) as f32 / 255.0] }
    }
}

pub const ENTRIES: usize = 32;
pub type Table = [[f32; 4]; ENTRIES];

pub struct Preset {
    pub name: &'static str,
    pub stops: &'static [Stop],
}

/// Shipped palettes, in panel order. Positions run 0..1; cyclic palettes wrap.
pub const PRESETS: &[Preset] = &[
    Preset { name: "phosphor", stops: &[Stop::hex(0.0, 0x00331a), Stop::hex(0.45, 0x66ff8c), Stop::hex(0.8, 0xc8ffd8), Stop::hex(1.0, 0xffffff)] },
    Preset { name: "techno", stops: &[Stop::hex(0.0, 0x0a1a3f), Stop::hex(0.35, 0x00e5ff), Stop::hex(0.6, 0xffffff), Stop::hex(0.85, 0xff00aa), Stop::hex(1.0, 0x0a1a3f)] },
    Preset { name: "neon", stops: &[Stop::hex(0.0, 0xff1f8f), Stop::hex(0.25, 0x7b2bff), Stop::hex(0.5, 0x1fd6ff), Stop::hex(0.75, 0xb4ff1f), Stop::hex(1.0, 0xff8f1f)] },
    Preset { name: "rave", stops: &[Stop::hex(0.0, 0x5a00ff), Stop::hex(0.3, 0x39ff14), Stop::hex(0.55, 0xff6a00), Stop::hex(0.8, 0x00f0ff), Stop::hex(1.0, 0xffffff)] },
    Preset { name: "80s pop", stops: &[Stop::hex(0.0, 0x1de9b6), Stop::hex(0.3, 0xff4da6), Stop::hex(0.55, 0x7a3cff), Stop::hex(0.8, 0xffe14d), Stop::hex(1.0, 0xff8a65)] },
    Preset { name: "galaxy", stops: &[Stop::hex(0.0, 0x0b1035), Stop::hex(0.35, 0x4b2a9d), Stop::hex(0.6, 0xc33bb0), Stop::hex(0.82, 0xff6fa0), Stop::hex(1.0, 0xfff6e0)] },
    Preset { name: "fire", stops: &[Stop::hex(0.0, 0x2a0400), Stop::hex(0.35, 0xc41b00), Stop::hex(0.6, 0xff7a00), Stop::hex(0.85, 0xffd34d), Stop::hex(1.0, 0xffffff)] },
    Preset { name: "ice", stops: &[Stop::hex(0.0, 0x061a33), Stop::hex(0.4, 0x1e6cff), Stop::hex(0.75, 0x7fd3ff), Stop::hex(1.0, 0xe6f7ff)] },
    Preset { name: "sunset", stops: &[Stop::hex(0.0, 0x2d0a3d), Stop::hex(0.3, 0x7b1e5e), Stop::hex(0.55, 0xe8503c), Stop::hex(0.8, 0xffb347), Stop::hex(1.0, 0xffe8a3)] },
    Preset { name: "mono", stops: &[Stop::hex(0.0, 0x000000), Stop::hex(1.0, 0xffffff)] },
];

pub fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

/// A sensible starting point for a custom palette.
pub fn default_custom() -> Vec<Stop> {
    vec![Stop::hex(0.0, 0x1a0b3d), Stop::hex(0.5, 0x66ff8c), Stop::hex(1.0, 0xffffff)]
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear sRGB -> OKLab (Björn Ottosson).
fn linear_to_oklab(c: [f32; 3]) -> [f32; 3] {
    let l = 0.4122214708 * c[0] + 0.5363325363 * c[1] + 0.0514459929 * c[2];
    let m = 0.2119034982 * c[0] + 0.6806995451 * c[1] + 0.1073969566 * c[2];
    let s = 0.0883024619 * c[0] + 0.2817188376 * c[1] + 0.6299787005 * c[2];
    let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
    [0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s, 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s]
}

fn oklab_to_linear(c: [f32; 3]) -> [f32; 3] {
    let l = c[0] + 0.3963377774 * c[1] + 0.2158037573 * c[2];
    let m = c[0] - 0.1055613458 * c[1] - 0.0638541728 * c[2];
    let s = c[0] - 0.0894841775 * c[1] - 1.2914855480 * c[2];
    let (l, m, s) = (l * l * l, m * m * m, s * s * s);
    [
        (4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s).max(0.0),
        (-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s).max(0.0),
        (-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s).max(0.0),
    ]
}

/// Bake stops into the table. `cyclic` wraps the last entry back toward the
/// first so a shifting palette has no seam.
pub fn bake(stops: &[Stop], cyclic: bool) -> Table {
    let mut out = [[0.0f32, 0.0, 0.0, 1.0]; ENTRIES];
    if stops.is_empty() {
        return out;
    }
    let mut s: Vec<Stop> = stops.to_vec();
    s.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    if cyclic {
        let first = s[0];
        s.push(Stop { t: 1.0 + first.t.max(0.0), rgb: first.rgb });
    }
    let lab: Vec<[f32; 3]> = s.iter().map(|st| linear_to_oklab([srgb_to_linear(st.rgb[0]), srgb_to_linear(st.rgb[1]), srgb_to_linear(st.rgb[2])])).collect();
    let span = if cyclic { ENTRIES as f32 } else { (ENTRIES - 1) as f32 };
    for (i, e) in out.iter_mut().enumerate() {
        let t = i as f32 / span;
        // Find the segment.
        let mut k = 0;
        while k + 1 < s.len() && s[k + 1].t < t {
            k += 1;
        }
        let c = if k + 1 >= s.len() {
            lab[s.len() - 1]
        } else if t <= s[k].t {
            lab[k]
        } else {
            let a = s[k].t;
            let b = s[k + 1].t;
            let f = if b > a { ((t - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 };
            [lab[k][0] + (lab[k + 1][0] - lab[k][0]) * f, lab[k][1] + (lab[k + 1][1] - lab[k][1]) * f, lab[k][2] + (lab[k + 1][2] - lab[k][2]) * f]
        };
        let lin = oklab_to_linear(c);
        *e = [lin[0], lin[1], lin[2], 1.0];
    }
    out
}

/// A stable key for "has this palette changed" checks.
pub fn key(name: &str, custom: &[Stop], cyclic: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut h);
    cyclic.hash(&mut h);
    for s in custom {
        s.t.to_bits().hash(&mut h);
        for c in s.rgb {
            c.to_bits().hash(&mut h);
        }
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklab_round_trips() {
        for c in [[1.0, 0.0, 0.0], [0.2, 0.9, 0.4], [0.5, 0.5, 0.5], [0.0, 0.0, 1.0]] {
            let back = oklab_to_linear(linear_to_oklab(c));
            for i in 0..3 {
                assert!((back[i] - c[i]).abs() < 1e-3, "{c:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn bake_hits_the_endpoints() {
        let t = bake(&[Stop::hex(0.0, 0x000000), Stop::hex(1.0, 0xffffff)], false);
        assert!(t[0][0] < 1e-3);
        assert!((t[ENTRIES - 1][0] - 1.0).abs() < 1e-3);
        // Monotonic brightness for a black-to-white ramp.
        for i in 1..ENTRIES {
            assert!(t[i][0] >= t[i - 1][0] - 1e-6);
        }
    }

    #[test]
    fn every_preset_bakes() {
        for p in PRESETS {
            let t = bake(p.stops, true);
            assert!(t.iter().all(|e| e.iter().all(|v| v.is_finite())), "{}", p.name);
        }
    }
}
