//! Scenes: named compositions (layers + Picture + tint). A few ship built
//! in; user scenes are JSON files in `<config dir>/audioviz/scenes/<name>.json`,
//! which doubles as export/import (copy the file). Files saved by older
//! versions (the `looks/` folder) are read too and migrated on load.
//!
//! A scene file holds the composition only, small enough to edit by hand:
//!
//! ```json
//! { "name": "Lantern", "tint": [1, 0.4, 0.8], "tint_from_art": false,
//!   "picture": { "exposure": 1.0, "bloom": 0.4, ... },
//!   "layers": [ { "visual": "crystal", "opacity": 1.0, ..., "crystal": { ... } } ] }
//! ```
//!
//! Each layer carries its own visual's parameters and not the other
//! visuals' (they take their defaults on load). Files from before this
//! format were a whole settings object; they still load (the scene fields
//! are taken, the rest ignored) and are written in this form when saved.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

use crate::factory;
use crate::settings::{Layer, PostSettings, Settings};

/// The on-disk shape of a scene, in the order a person reads it.
#[derive(Serialize)]
struct SceneFile<'a> {
    name: &'a str,
    tint: [f32; 3],
    tint_from_art: bool,
    picture: &'a PostSettings,
    layers: Vec<Value>,
}

/// A layer as a scene stores it: everything but the other visuals' parameters.
fn layer_json(l: &Layer) -> Value {
    // Through text, not `to_value`: that widens every f32 to f64 and a 0.4
    // would be written 0.4000000059604645.
    let mut v: Value = serde_json::to_string(l).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        for k in l.params.keys() {
            if *k != l.visual {
                o.shift_remove(k.as_str());
            }
        }
    }
    v
}

/// The scene part of `settings` as the text of a scene file.
pub fn to_text(settings: &Settings) -> anyhow::Result<String> {
    let f = SceneFile { name: &settings.scene_name, tint: settings.tint, tint_from_art: settings.tint_from_art, picture: &settings.post, layers: settings.layers.iter().map(layer_json).collect() };
    Ok(serde_json::to_string_pretty(&f)? + "\n")
}

/// Read a scene file, either shape, into settings (only the scene fields
/// mean anything). The scene shape is renamed to the settings keys so the
/// settings migrations (old layer layouts, legacy fields) apply to both.
pub fn from_text(text: &str) -> anyhow::Result<Settings> {
    let mut raw: Value = serde_json::from_str(text)?;
    if let Some(o) = raw.as_object_mut() {
        if let Some(p) = o.shift_remove("picture") {
            o.insert("post".into(), p);
        }
        if let Some(n) = o.shift_remove("name") {
            o.insert("scene_name".into(), n);
        }
    }
    crate::config::parse(&raw.to_string())
}

/// Two scene texts that mean the same composition (whatever their shape).
pub fn same_scene(a: &str, b: &str) -> bool {
    match (from_text(a).and_then(|s| to_text(&s)), from_text(b).and_then(|s| to_text(&s))) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

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
    let mut settings = from_text(text).ok()?;
    let name = factory::stem(rel).to_string();
    settings.scene_name = name.clone();
    let (fac, same) = factory::status_with(rel, text, same_scene);
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
    std::fs::write(d.join(format!("{name}.json")), to_text(&s)?)?;
    Ok(name)
}

/// Give a saved scene a new name (a new file; the old one is removed).
pub fn rename(old: &str, new: &str, settings: &Settings) -> anyhow::Result<String> {
    anyhow::ensure!(safe_name(old) != safe_name(new), "same name");
    anyhow::ensure!(dir().is_some_and(|d| !d.join(format!("{}.json", safe_name(new))).exists()), "a scene called '{}' already exists", safe_name(new));
    let n = save(new, settings)?;
    delete(old)?;
    Ok(n)
}

/// A free name for a copy of `name`: "Lantern 2", "Lantern 3"...
pub fn copy_name(name: &str, taken: &[&str]) -> String {
    let base = safe_name(name);
    let base = base.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end().to_string();
    (2..1000).map(|n| format!("{base} {n}")).find(|c| !taken.iter().any(|t| t.eq_ignore_ascii_case(c))).unwrap_or_else(|| format!("{base} copy"))
}

/// Write one factory scene back over its file.
pub fn restore_one(name: &str) -> anyhow::Result<usize> {
    factory::install(&format!("scenes/{}.json", safe_name(name)), true)
}

/// Whether the current settings differ from the saved scene they came from.
pub fn changed(settings: &Settings, saved: &Settings) -> bool {
    match (to_text(settings), to_text(saved)) {
        (Ok(a), Ok(b)) => a != b,
        _ => false,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_scenes_are_in_the_scene_shape() {
        for (rel, text) in factory::embedded("scenes/") {
            let v: Value = serde_json::from_str(text).unwrap();
            let o = v.as_object().unwrap();
            let keys: Vec<&str> = o.keys().map(|k| k.as_str()).collect();
            assert_eq!(keys, ["name", "tint", "tint_from_art", "picture", "layers"], "{rel}");
            assert_eq!(o["name"].as_str(), Some(factory::stem(rel)), "{rel}: name matches the file");
            // Written exactly as the app writes it, so a factory file is never "(edited)" on arrival.
            assert_eq!(to_text(&from_text(text).unwrap()).unwrap(), *text, "{rel}");
        }
    }

    /// Rewrites `assets/scenes/` in the current scene shape. Run by hand when
    /// the shape changes: `cargo test -p audioviz rewrite_factory_scene_files -- --ignored`.
    #[test]
    #[ignore]
    fn rewrite_factory_scene_files() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/scenes");
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let mut s = from_text(&std::fs::read_to_string(&p).unwrap()).unwrap();
            s.scene_name = p.file_stem().unwrap().to_string_lossy().into_owned();
            std::fs::write(&p, to_text(&s).unwrap()).unwrap();
        }
    }

    #[test]
    fn copies_get_free_names() {
        assert_eq!(copy_name("Lantern", &["Lantern"]), "Lantern 2");
        assert_eq!(copy_name("Lantern 2", &["Lantern", "Lantern 2"]), "Lantern 3");
        assert_eq!(copy_name("Night", &["night 2"]), "Night 3");
    }

    #[test]
    fn an_applied_scene_is_unchanged_until_edited() {
        let def = &builtin()[0];
        let mut s = Settings::default();
        s.apply_scene(&def.settings);
        assert!(!changed(&s, &def.settings));
        s.layers[0].opacity *= 0.5;
        assert!(changed(&s, &def.settings));
    }

    #[test]
    fn a_layer_keeps_only_its_own_visual() {
        let mut s = Settings::default();
        s.scene_name = "Test".into();
        s.layers[0].visual = "crystal".into();
        s.layers[0].put("crystal", &crate::settings::CrystalParams { count: 33, ..Default::default() });
        s.layers[0].put("scope", &crate::settings::ScopeParams { gain: 3.0, ..Default::default() });
        let text = to_text(&s).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        let l0 = v["layers"][0].as_object().unwrap();
        assert!(l0.contains_key("crystal") && !l0.contains_key("scope") && !l0.contains_key("ribbons"));
        let back = from_text(&text).unwrap();
        assert_eq!(back.layers[0].get::<crate::settings::CrystalParams>("crystal").count, 33);
        assert_eq!(back.layers[0].get::<crate::settings::ScopeParams>("scope"), crate::settings::ScopeParams::default(), "other visuals take their defaults");
        assert_eq!(back.scene_name, "Test");
    }

    #[test]
    fn an_old_whole_settings_file_still_loads_and_matches_its_new_form() {
        let mut s = Settings::default();
        s.scene_name = "Old".into();
        s.post.bloom = 0.33;
        s.tint = [0.1, 0.2, 0.3];
        let old = serde_json::to_string_pretty(&s).unwrap();
        let back = from_text(&old).unwrap();
        assert_eq!(back.post.bloom, 0.33);
        assert_eq!(back.tint, [0.1, 0.2, 0.3]);
        assert_eq!(back.scene_name, "Old");
        assert!(same_scene(&old, &to_text(&s).unwrap()));
    }
}
