//! Settings persistence: `<config dir>/audioviz/settings.json`
//! (Windows: %APPDATA%\audioviz\settings.json).

use std::path::PathBuf;

use crate::ui::Settings;

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
        Ok(text) => match serde_json::from_str::<Settings>(&text) {
            Ok(s) => {
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
