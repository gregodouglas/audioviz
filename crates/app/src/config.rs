//! Settings persistence: `<config dir>/audioviz/settings.json`
//! (Windows: %APPDATA%\audioviz\settings.json), plus migration of older
//! layouts so a tuned file is never thrown away.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::settings::{MAX_LAYERS, Settings};

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
        Ok(text) => match parse(&text) {
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

/// Parse a settings (or scene) file of any version.
pub fn parse(text: &str) -> anyhow::Result<Settings> {
    let mut raw: Value = serde_json::from_str(text)?;
    migrate(&mut raw);
    let mut s: Settings = serde_json::from_value(raw)?;
    s.normalize();
    Ok(s)
}

fn f(obj: &Map<String, Value>, k: &str) -> Option<f64> {
    obj.get(k).and_then(|v| v.as_f64())
}

/// Bring older layouts up to date, in place:
/// - 0.1.0: one global `exposure/bloom/grain/auto_exposure`.
/// - 0.2.0 dev: `post` as an array per scene; flat per-visual fields
///   (`gain`, `ribbons_*`, `star_*`), `scene`, `layer_mode`, `layers` without
///   `visual`, `look`, sections `Scene`/`Looks`.
pub fn migrate(raw: &mut Value) {
    let Some(obj) = raw.as_object_mut() else { return };

    // Which single scene was showing (0.1.0 / early 0.2.0).
    let old_scene = obj.get("scene").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    // post: array -> the old scene's entry; global fields -> object.
    match obj.get("post").cloned() {
        Some(Value::Array(v)) => {
            let pick = v.get(old_scene).or(v.first()).cloned().unwrap_or(json!({}));
            obj.insert("post".into(), pick);
        }
        None => {
            let legacy = ["exposure", "auto_exposure", "bloom", "grain"].iter().filter_map(|k| obj.get(*k).map(|v| ((*k).to_string(), v.clone()))).collect::<Map<_, _>>();
            if !legacy.is_empty() {
                obj.insert("post".into(), Value::Object(legacy));
            }
        }
        _ => {}
    }

    // layers: build from flat fields when the file predates the layer model.
    let has_layers = obj.get("layers").and_then(|v| v.as_array()).is_some_and(|a| a.first().is_some_and(|l| l.get("visual").is_some()));
    if !has_layers {
        let layer_mode = obj.get("layer_mode").and_then(|v| v.as_bool()).unwrap_or(false);
        let old_layers = obj.get("layers").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let scope = json!({
            "gain": f(obj, "gain").unwrap_or(0.85), "auto_gain": obj.get("auto_gain").and_then(|v| v.as_bool()).unwrap_or(true),
            "rotation_deg": f(obj, "rotation_deg").unwrap_or(45.0), "beam_width": f(obj, "beam_width").unwrap_or(1.2),
            "beam_intensity": f(obj, "beam_intensity").unwrap_or(1.0), "persistence": f(obj, "persistence").unwrap_or(0.05),
            "beat_pulse": f(obj, "beat_pulse").unwrap_or(0.3)
        });
        let ribbons = json!({
            "speed": f(obj, "ribbons_speed").unwrap_or(0.3), "scale": f(obj, "ribbons_scale").unwrap_or(1.5), "curl": f(obj, "ribbons_curl").unwrap_or(1.0),
            "vortex": f(obj, "ribbons_vortex").unwrap_or(1.0), "width": f(obj, "ribbons_width").unwrap_or(1.0), "intensity": f(obj, "ribbons_intensity").unwrap_or(1.0),
            "persistence": f(obj, "ribbons_persistence").unwrap_or(0.3), "saturation": f(obj, "ribbons_saturation").unwrap_or(0.45),
            "count": obj.get("ribbons_count").and_then(|v| v.as_u64()).unwrap_or(12_000)
        });
        let starfield = json!({
            "mass": f(obj, "star_mass").unwrap_or(1.0), "density": f(obj, "star_density").unwrap_or(1.0),
            "twinkle": f(obj, "star_twinkle").unwrap_or(0.6), "drift": f(obj, "star_drift").unwrap_or(1.0)
        });
        let visuals = ["scope", "ribbons", "starfield"];
        let mut layers = Vec::new();
        for (i, visual) in visuals.iter().enumerate().take(MAX_LAYERS) {
            let (enabled, opacity) = if layer_mode {
                let l = old_layers.get(i);
                (l.and_then(|l| l.get("enabled")).and_then(|v| v.as_bool()).unwrap_or(true), l.and_then(|l| l.get("opacity")).and_then(|v| v.as_f64()).unwrap_or(1.0))
            } else {
                (i == old_scene, 1.0)
            };
            layers.push(json!({ "visual": visual, "enabled": enabled, "opacity": opacity, "scope": scope, "ribbons": ribbons, "starfield": starfield }));
        }
        obj.insert("layers".into(), Value::Array(layers));
        obj.insert("editing".into(), json!(old_scene.min(MAX_LAYERS - 1)));
        if obj.get("tint").is_none() {
            obj.insert("tint".into(), json!([0.40, 1.0, 0.55]));
        }
        tracing::info!("migrated flat visual settings to layers");
    }

    if let Some(look) = obj.remove("look") {
        obj.entry("scene_name").or_insert(look);
    }
    // Panel section names that no longer exist.
    if let Some(Value::String(s)) = obj.get("section").cloned() {
        let mapped = match s.as_str() {
            "Scene" | "Looks" => Some("Scenes"),
            "Scenes" | "Layers" | "Post" | "Sync" | "Display" | "Meters" | "About" => None,
            _ => Some("Scenes"),
        };
        if let Some(m) = mapped {
            obj.insert("section".into(), json!(m));
        }
    }
    for k in ["scene", "layer_mode", "gain", "auto_gain", "rotation_deg", "beam_width", "beam_intensity", "persistence", "beat_pulse", "exposure", "auto_exposure", "bloom", "grain"] {
        obj.remove(k);
    }
    obj.retain(|k, _| !k.starts_with("ribbons_") && !k.starts_with("star_"));
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
    let mut root: Value = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}));
    let Some(obj) = root.as_object_mut() else { return };
    for (k, v) in pairs {
        let value = serde_json::from_str::<Value>(v).unwrap_or_else(|_| Value::String(v.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::VisualKind;

    #[test]
    fn migrates_0_1_0_file() {
        let text = r#"{"gain":1.5,"ribbons_speed":0.7,"star_mass":2.0,"scene":1,"bloom":0.9,"exposure":1.2,"show_ui":true,"section":"Scene","look":"Night"}"#;
        let s = parse(text).unwrap();
        assert_eq!(s.layers.len(), 3);
        assert_eq!(s.layers[0].visual, VisualKind::Scope);
        assert!(!s.layers[0].enabled && s.layers[1].enabled && !s.layers[2].enabled);
        assert_eq!(s.layers[1].ribbons.speed, 0.7);
        assert_eq!(s.layers[0].scope.gain, 1.5);
        assert_eq!(s.layers[2].starfield.mass, 2.0);
        assert_eq!(s.post.bloom, 0.9);
        assert_eq!(s.post.exposure, 1.2);
        assert_eq!(s.editing, 1);
        assert_eq!(s.scene_name, "Night");
        assert!(s.show_ui);
    }

    #[test]
    fn migrates_post_array_and_layer_mode() {
        let text = r#"{"post":[{"bloom":0.1},{"bloom":0.2},{"bloom":0.3}],"scene":2,"layer_mode":true,"layers":[{"enabled":true,"opacity":1.0},{"enabled":false,"opacity":0.4},{"enabled":true,"opacity":0.7}],"section":"Looks"}"#;
        let s = parse(text).unwrap();
        assert_eq!(s.post.bloom, 0.3);
        assert!(s.layers[0].enabled && !s.layers[1].enabled && s.layers[2].enabled);
        assert_eq!(s.layers[2].opacity, 0.7);
        assert_eq!(s.section, crate::settings::Section::Scenes);
    }

    #[test]
    fn migrates_edge_tinted_flag_and_defaults_modifier_bands() {
        let text = r#"{"layers":[{"visual":"scope","enabled":true,"modifiers":[{"kind":"edge","tinted":true},{"kind":"ripple"}]}]}"#;
        let s = parse(text).unwrap();
        let m = &s.layers[0].modifiers;
        assert!(matches!(m[0], crate::settings::Modifier::Edge(e) if e.colour == crate::settings::EdgeColour::Tint && !e.tinted));
        assert!(m.iter().all(|m| m.band().is_none()), "modifiers follow the layer's band by default");
        assert!(!serde_json::to_string(&s).unwrap().contains("tinted"));
    }

    #[test]
    fn current_format_round_trips() {
        let s = Settings::default();
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(parse(&text).unwrap(), s);
    }
}
