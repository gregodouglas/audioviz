//! Scenes: named compositions (layers + post + tint). A few ship built in;
//! user scenes are JSON files in `<config dir>/audioviz/scenes/<name>.json`,
//! which doubles as export/import (copy the file). Files saved by older
//! versions (the `looks/` folder) are read too and migrated on load.

use std::path::PathBuf;

use crate::settings::{DirectorMode, Fold, Halftone, Layer, Lines, Mirror, Modifier, Settings, VisualKind, Warp};

#[derive(Clone)]
pub struct SceneDef {
    pub name: String,
    pub settings: Settings,
    pub builtin: bool,
}

pub fn dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("audioviz").join("scenes"))
}

fn legacy_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("audioviz").join("looks"))
}

fn scene(name: &str, layers: Vec<Layer>, f: impl FnOnce(&mut Settings)) -> SceneDef {
    let mut s = Settings::default();
    s.layers = layers;
    s.scene_name = name.into();
    f(&mut s);
    SceneDef { name: name.into(), settings: s, builtin: true }
}

pub fn builtin() -> Vec<SceneDef> {
    let mut v = vec![
        scene("Scope", vec![Layer::new(VisualKind::Scope, true, 1.0), Layer::new(VisualKind::Ribbons, false, 0.5), Layer::new(VisualKind::Starfield, false, 0.5)], |_| {}),
        scene("Ribbons", vec![Layer::new(VisualKind::Ribbons, true, 1.0), Layer::new(VisualKind::Scope, false, 0.5), Layer::new(VisualKind::Starfield, false, 0.5)], |_| {}),
        scene("Starfield", vec![Layer::new(VisualKind::Starfield, true, 1.0), Layer::new(VisualKind::Ribbons, false, 0.5), Layer::new(VisualKind::Scope, false, 0.5)], |s| s.post.bloom = 0.12),
        scene(
            "Deep field",
            vec![
                {
                    let mut l = Layer::new(VisualKind::Ribbons, true, 1.0);
                    l.ribbons.speed = 0.08;
                    l.ribbons.count = 3000;
                    l.ribbons.intensity = 0.5;
                    l.ribbons.persistence = 0.6;
                    l.ribbons.saturation = 0.2;
                    l
                },
                {
                    let mut l = Layer::new(VisualKind::Ribbons, true, 0.35);
                    l.ribbons.speed = 0.6;
                    l.ribbons.count = 1200;
                    l.ribbons.intensity = 1.6;
                    l.ribbons.persistence = 0.05;
                    l.ribbons.width = 0.6;
                    l.band.enabled = true;
                    l.band.lo_hz = 2000.0;
                    l.band.hi_hz = 16000.0;
                    l
                },
                Layer::new(VisualKind::Starfield, false, 0.3),
            ],
            |s| {
                s.post.bloom = 0.2;
                s.tint = [0.75, 0.85, 1.0];
            },
        ),
        scene(
            "Night",
            vec![
                {
                    let mut l = Layer::new(VisualKind::Starfield, true, 0.8);
                    l.starfield.density = 0.8;
                    l.starfield.twinkle = 0.4;
                    l
                },
                {
                    let mut l = Layer::new(VisualKind::Ribbons, true, 0.3);
                    l.ribbons.intensity = 0.7;
                    l.ribbons.speed = 0.2;
                    l
                },
                Layer::new(VisualKind::Scope, false, 0.5),
            ],
            |s| {
                s.post.bloom = 0.08;
                s.post.exposure = 0.8;
            },
        ),
        scene(
            "Party",
            vec![
                {
                    let mut l = Layer::new(VisualKind::Ribbons, true, 1.0);
                    l.ribbons.speed = 0.45;
                    l.ribbons.saturation = 0.7;
                    l.ribbons.intensity = 1.3;
                    l
                },
                {
                    let mut l = Layer::new(VisualKind::Scope, true, 0.8);
                    l.scope.beat_pulse = 0.6;
                    l.scope.beam_intensity = 1.4;
                    l.band.enabled = true;
                    l.band.lo_hz = 20.0;
                    l.band.hi_hz = 250.0;
                    l
                },
                Layer::new(VisualKind::Starfield, false, 0.5),
            ],
            |s| {
                s.post.bloom = 0.5;
                s.tint_from_art = true;
            },
        ),
        scene(
            "Studio",
            vec![
                {
                    let mut l = Layer::new(VisualKind::Scope, true, 1.0);
                    l.scope.beam_width = 0.9;
                    l.scope.persistence = 0.02;
                    l.scope.beat_pulse = 0.0;
                    l
                },
                Layer::new(VisualKind::Ribbons, false, 0.5),
                Layer::new(VisualKind::Starfield, false, 0.5),
            ],
            |s| {
                s.tint = [0.85, 0.9, 1.0];
                s.post.bloom = 0.12;
                s.post.grain = 0.05;
            },
        ),
    ];
    v.push(scene(
        "Kaleidoscope",
        vec![
            {
                let mut l = Layer::new(VisualKind::Kaleido, true, 1.0);
                l.modifiers.push(Modifier::Fold(Fold::default()));
                l
            },
            Layer::new(VisualKind::Ribbons, false, 0.4),
            Layer::new(VisualKind::Scope, false, 0.5),
        ],
        |s| s.post.bloom = 0.1,
    ));
    v.push(scene(
        "Sigil",
        vec![
            {
                let mut l = Layer::new(VisualKind::Sigil, true, 1.0);
                l.palette.name = "neon".into();
                l
            },
            Layer::new(VisualKind::Ribbons, false, 0.4),
            Layer::new(VisualKind::Scope, false, 0.5),
        ],
        |s| {
            s.tint = [0.55, 0.35, 1.0];
            s.post.bloom = 0.16;
            s.post.look = crate::settings::Look::Neon;
        },
    ));
    v.push(scene(
        "Waves",
        vec![
            {
                let mut l = Layer::new(VisualKind::Waves, true, 1.0);
                l.palette.name = "ice".into();
                l
            },
            Layer::new(VisualKind::Ribbons, false, 0.4),
            Layer::new(VisualKind::Scope, false, 0.5),
        ],
        |s| {
            s.tint = [0.75, 0.85, 1.0];
            s.post.bloom = 0.12;
        },
    ));
    v.push(scene(
        "Crystal",
        vec![
            {
                let mut l = Layer::new(VisualKind::Crystal, true, 1.0);
                l.palette.name = "neon".into();
                l
            },
            Layer::new(VisualKind::Ribbons, false, 0.4),
            Layer::new(VisualKind::Scope, false, 0.5),
        ],
        |s| {
            s.tint = [0.55, 0.35, 1.0];
            s.post.bloom = 0.18;
            s.post.look = crate::settings::Look::Neon;
        },
    ));
    v.push(scene(
        "Lantern",
        vec![
            {
                let mut l = Layer::new(VisualKind::Crystal, true, 1.0);
                l.palette.name = "neon".into();
                l.modifiers.push(Modifier::Mirror(Mirror { flip: 0.3, pinned: true, ..Default::default() }));
                l.modifiers.push(Modifier::Halftone(Halftone { mono: 0.8, ..Default::default() }));
                l.modifiers.push(Modifier::Warp(Warp { amount: 30.0, ..Default::default() }));
                l.modifiers.push(Modifier::Lines(Lines { pinned: true, ..Default::default() }));
                l.modifiers.push(Modifier::Chroma(crate::settings::ChromaSplit { amount: 0.004, pinned: true, ..Default::default() }));
                l.director.mode = DirectorMode::Auto;
                l
            },
            {
                let mut l = Layer::new(VisualKind::Wheel, true, 0.8);
                l.palette.name = "neon".into();
                l.palette.cycle = 0.02;
                l.wheel.gain = 1.6;
                l.wheel.curve = 1.5;
                l.modifiers.push(Modifier::Lines(Lines::default()));
                l
            },
            Layer::new(VisualKind::Scope, false, 0.5),
        ],
        |s| {
            s.tint = [1.0, 0.35, 0.8];
            s.post.bloom = 0.4;
            s.post.look = crate::settings::Look::Film;
        },
    ));
    v
}

fn read_dir(d: Option<PathBuf>, out: &mut Vec<SceneDef>) {
    let Some(d) = d else { return };
    let Ok(rd) = std::fs::read_dir(&d) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = p.file_stem().and_then(|x| x.to_str()).map(String::from) else { continue };
        if out.iter().any(|s| s.name == name) {
            continue;
        }
        match std::fs::read_to_string(&p).ok().and_then(|t| crate::config::parse(&t).ok()) {
            Some(mut settings) => {
                settings.scene_name = name.clone();
                out.push(SceneDef { name, settings, builtin: false });
            }
            None => tracing::warn!("scene unreadable: {}", p.display()),
        }
    }
}

pub fn load_user() -> Vec<SceneDef> {
    let mut out = Vec::new();
    read_dir(dir(), &mut out);
    read_dir(legacy_dir(), &mut out);
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

pub fn all() -> Vec<SceneDef> {
    let mut v = builtin();
    v.extend(load_user());
    v
}

fn safe_name(name: &str) -> String {
    name.trim().chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').take(40).collect()
}

pub fn save(name: &str, settings: &Settings) -> anyhow::Result<String> {
    let name = safe_name(name);
    anyhow::ensure!(!name.is_empty(), "name is empty");
    let d = dir().ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    std::fs::create_dir_all(&d)?;
    let mut s = settings.clone();
    s.scene_name = name.clone();
    let text = serde_json::to_string_pretty(&s)?;
    std::fs::write(d.join(format!("{name}.json")), text)?;
    Ok(name)
}

pub fn delete(name: &str) -> anyhow::Result<()> {
    let n = safe_name(name);
    for d in [dir(), legacy_dir()].into_iter().flatten() {
        let p = d.join(format!("{n}.json"));
        if p.exists() {
            std::fs::remove_file(p)?;
        }
    }
    Ok(())
}
