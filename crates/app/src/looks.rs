//! Looks: named presets of the visual settings. Three ship built in; user
//! looks are JSON files in `<config dir>/audioviz/looks/<name>.json`, which
//! doubles as export/import (copy the file).

use std::path::PathBuf;

use crate::settings::Settings;

#[derive(Clone)]
pub struct Look {
    pub name: String,
    pub settings: Settings,
    pub builtin: bool,
}

pub fn dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("audioviz").join("looks"))
}

pub fn builtin() -> Vec<Look> {
    let base = Settings::default();
    let night = {
        let mut s = base.clone();
        s.scene = 2;
        s.star_density = 0.8;
        s.star_twinkle = 0.4;
        s.post_mut(2).bloom = 0.08;
        s.post_mut(2).exposure = 0.8;
        s.post_mut(0).bloom = 0.2;
        s.persistence = 0.08;
        s.ribbons_intensity = 0.7;
        s
    };
    let party = {
        let mut s = base.clone();
        s.scene = 1;
        s.auto_scenes = true;
        s.ribbons_speed = 0.45;
        s.ribbons_saturation = 0.7;
        s.ribbons_intensity = 1.3;
        s.beat_pulse = 0.6;
        s.beam_intensity = 1.4;
        for i in 0..3 {
            s.post_mut(i).bloom = 0.5;
        }
        s.tint_from_art = true;
        s
    };
    let studio = {
        let mut s = base.clone();
        s.scene = 0;
        s.tint = [0.85, 0.9, 1.0];
        s.beam_width = 0.9;
        s.persistence = 0.02;
        s.beat_pulse = 0.0;
        for i in 0..3 {
            s.post_mut(i).bloom = 0.12;
            s.post_mut(i).grain = 0.05;
        }
        s
    };
    vec![
        Look { name: "Night".into(), settings: night, builtin: true },
        Look { name: "Party".into(), settings: party, builtin: true },
        Look { name: "Studio".into(), settings: studio, builtin: true },
    ]
}

pub fn load_user() -> Vec<Look> {
    let Some(d) = dir() else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(&d) else { return Vec::new() };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = p.file_stem().and_then(|x| x.to_str()).map(String::from) else { continue };
        match std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Settings>(&t).ok()) {
            Some(settings) => out.push(Look { name, settings, builtin: false }),
            None => tracing::warn!("look unreadable: {}", p.display()),
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

pub fn all() -> Vec<Look> {
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
    let text = serde_json::to_string_pretty(settings)?;
    std::fs::write(d.join(format!("{name}.json")), text)?;
    Ok(name)
}

pub fn delete(name: &str) -> anyhow::Result<()> {
    let d = dir().ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    std::fs::remove_file(d.join(format!("{}.json", safe_name(name))))?;
    Ok(())
}
