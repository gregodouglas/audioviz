//! Settings persistence: `<config dir>/audioviz/settings.json`
//! (Windows: %APPDATA%\audioviz\settings.json).

use std::path::PathBuf;

use crate::settings::{PostSettings, Settings};

pub fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("audioviz").join("settings.json"))
}

pub fn load() -> Settings {
    let Some(p) = path() else { return Settings::default() };
    if std::env::var_os("AUDIOVIZ_DEFAULTS").is_some() {
        tracing::info!("AUDIOVIZ_DEFAULTS set; ignoring {}", p.display());
        return Settings::default();
    }
    match std::fs::read_to_string(&p) {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text).and_then(|v| Ok((serde_json::from_value::<Settings>(v.clone())?, v))) {
            Ok((mut s, raw)) => {
                migrate_post(&mut s, &raw);
                tracing::info!("settings loaded from {}", p.display());
                s
            }
            Err(e) => {
                tracing::warn!("settings file unreadable ({e}); using defaults");
                Settings::default()
            }
        },
        Err(_) => Settings::default(),
    }
}

/// Pre-0.1.0 files had one global `exposure`/`auto_exposure`/`bloom`/`grain`;
/// carry those values into every scene's post settings so tuned values survive.
fn migrate_post(s: &mut Settings, raw: &serde_json::Value) {
    if raw.get("post").is_some() {
        return;
    }
    let legacy = ["exposure", "auto_exposure", "bloom", "grain"].iter().any(|k| raw.get(k).is_some());
    if !legacy {
        return;
    }
    let f = |k: &str, d: f32| raw.get(k).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(d);
    let p = PostSettings {
        exposure: f("exposure", 1.0),
        auto_exposure: raw.get("auto_exposure").and_then(|v| v.as_bool()).unwrap_or(true),
        bloom: f("bloom", 0.3),
        grain: f("grain", 0.15),
    };
    for i in 0..crate::settings::SCENE_COUNT {
        *s.post_mut(i) = p;
    }
    tracing::info!("migrated global post settings to per-scene");
}

pub fn save(settings: &Settings) {
    // A dev run with AUDIOVIZ_DEFAULTS must never clobber the real file.
    if std::env::var_os("AUDIOVIZ_DEFAULTS").is_some() {
        return;
    }
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(settings) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&p, text) {
                tracing::warn!("could not save settings to {}: {e}", p.display());
            }
        }
        Err(e) => tracing::warn!("could not serialize settings: {e}"),
    }
}

/// `--set key=value` from the command line (the installer's finish page uses
/// it): patch the JSON on disk before the app loads it. Values parse as JSON
/// first (true, 1, 0.5, [..]), else as a string.
pub fn apply_overrides(pairs: &[(String, String)]) {
    let Some(p) = path() else { return };
    let mut root: serde_json::Value = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    let Some(obj) = root.as_object_mut() else { return };
    for (k, v) in pairs {
        let value = serde_json::from_str::<serde_json::Value>(v).unwrap_or_else(|_| serde_json::Value::String(v.clone()));
        tracing::info!("--set {k} = {value}");
        obj.insert(k.clone(), value);
    }
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(&root) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&p, text) {
                tracing::warn!("could not write {}: {e}", p.display());
            }
        }
        Err(e) => tracing::warn!("--set: {e}"),
    }
}
