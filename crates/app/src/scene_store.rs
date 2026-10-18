//! Scenes: named compositions (layers + post + tint). A few ship built in;
//! user scenes are JSON files in `<config dir>/audioviz/scenes/<name>.json`,
//! which doubles as export/import (copy the file). Files saved by older
//! versions (the `looks/` folder) are read too and migrated on load.

use std::path::PathBuf;

use crate::factory;
use crate::settings::Settings;

#[derive(Clone)]
pub struct SceneDef {
    pub name: String,
    pub settings: Settings,
    /// Shipped with the app (a factory file).
    pub factory: bool,
    /// A factory file the user has changed.
    pub edited: bool,
}

pub fn dir() -> Option<PathBuf> {
    factory::dir("scenes")
}

fn legacy_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("audioviz").join("looks"))
}

fn parse(rel: &str, text: &str) -> Option<SceneDef> {
    let mut settings = crate::config::parse(text).ok()?;
    let name = factory::stem(rel).to_string();
    settings.scene_name = name.clone();
    let (fac, same) = factory::status(rel, text);
    Some(SceneDef { name, settings, factory: fac, edited: fac && !same })
}

/// The factory scenes alone (a dev run, or for tests).
pub fn builtin() -> Vec<SceneDef> {
    factory::embedded("scenes/").filter_map(|(rel, text)| parse(rel, text)).collect()
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
        match std::fs::read_to_string(&p).ok().and_then(|t| parse(&format!("scenes/{name}.json"), &t)) {
            Some(def) => out.push(def),
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

/// Every scene: the factory set alone on a dev run, else the user's folder
/// (with the factory files written into it first if they are missing),
/// factory ones first.
pub fn all() -> Vec<SceneDef> {
    if factory::embedded_only() {
        return builtin();
    }
    if let Err(e) = factory::install("scenes/", false) {
        tracing::warn!("could not install factory scenes: {e}");
    }
    let mut v = load_user();
    v.sort_by(|a, b| b.factory.cmp(&a.factory).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

/// Write the factory scenes back over the folder.
pub fn restore_factory() -> anyhow::Result<usize> {
    factory::install("scenes/", true)
}

fn safe_name(name: &str) -> String {
    name.trim().chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').take(40).collect()
}

pub fn save(name: &str, settings: &Settings) -> anyhow::Result<String> {
    let name = safe_name(name);
    anyhow::ensure!(!name.is_empty(), "name is empty");
    anyhow::ensure!(!factory::embedded_only(), "a dev run does not write scenes");
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
