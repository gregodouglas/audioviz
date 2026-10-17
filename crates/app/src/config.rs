//! Settings persistence: `<config dir>/audioviz/settings.json`
//! (Windows: %APPDATA%\audioviz\settings.json).

use std::path::PathBuf;

use crate::ui::{PostSettings, Settings};

pub fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("audioviz").join("settings.json"))
}

/// Loaded settings plus whether this looks like a first run (no settings file).
pub struct Loaded {
    pub settings: Settings,
    pub first_run: bool,
}

pub fn load() -> Loaded {
    let Some(p) = path() else { return Loaded { settings: Settings::default(), first_run: true } };
    if std::env::var_os("AUDIOVIZ_DEFAULTS").is_some() {
        tracing::info!("AUDIOVIZ_DEFAULTS set; ignoring {}", p.display());
        return Loaded { settings: Settings::default(), first_run: true };
    }
    match std::fs::read_to_string(&p) {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text).and_then(|v| Ok((serde_json::from_value::<Settings>(v.clone())?, v))) {
            Ok((mut s, raw)) => {
                migrate_post(&mut s, &raw);
                tracing::info!("settings loaded from {}", p.display());
                Loaded { settings: s, first_run: false }
            }
            Err(e) => {
                tracing::warn!("settings file unreadable ({e}); using defaults");
                Loaded { settings: Settings::default(), first_run: false }
            }
        },
        Err(_) => Loaded { settings: Settings::default(), first_run: true },
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
    for i in 0..crate::ui::SCENE_COUNT {
        *s.post_mut(i) = p;
    }
    tracing::info!("migrated global post settings to per-scene");
}

pub fn save(settings: &Settings) {
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
