//! Visual presets: a named set of one visual's parameters, as a JSON file
//! in `<config>/audioviz/presets/<visual>/<name>.json`. The factory presets
//! (`assets/presets/` in the repo) are written there on first run and are
//! ordinary files from then on: edit, replace or delete them like any other.
//! "Restore factory presets" writes the embedded copies back.
//!
//! A preset only ever sets the visual's own parameters: not the palette,
//! band, blend, opacity or effects, which belong to the layer.

use std::path::PathBuf;

use serde_json::Value;

use crate::factory;
use crate::settings::Layer;
use crate::visuals;

#[derive(Clone, Debug)]
pub struct PresetDef {
    pub name: String,
    /// The visual's registry id (also the presets folder).
    pub visual: String,
    /// The visual's parameters, as JSON.
    pub params: Value,
    /// Shipped with the app (a factory file).
    pub factory: bool,
    /// A factory file the user has changed.
    pub edited: bool,
}

/// This layer's current parameters for its visual, as JSON (defaults filled in).
pub fn params_of(layer: &Layer) -> Value {
    match visuals::get(&layer.visual) {
        Some(d) => d.params(layer.params.get(&layer.visual)),
        None => layer.params.get(&layer.visual).cloned().unwrap_or(Value::Null),
    }
}

/// Apply a preset's parameters to a layer (switching it to the preset's visual).
pub fn apply(layer: &mut Layer, preset: &PresetDef) -> anyhow::Result<()> {
    layer.visual = preset.visual.clone();
    layer.params.insert(preset.visual.clone(), preset.params.clone());
    Ok(())
}

/// The visual's own defaults as a preset.
pub fn defaults(visual: &str) -> PresetDef {
    let params = visuals::get(visual).map_or(Value::Null, |d| d.params(None));
    PresetDef { name: String::new(), visual: visual.to_string(), params, factory: true, edited: false }
}

pub fn dir(visual: &str) -> Option<PathBuf> {
    factory::dir("presets").map(|d| d.join(visual))
}

fn safe_name(name: &str) -> String {
    name.trim().chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').take(48).collect::<String>().trim().to_string()
}

fn parse(rel: &str, text: &str) -> Option<PresetDef> {
    let mut parts = rel.split('/');
    let (_, folder, file) = (parts.next()?, parts.next()?, parts.next()?);
    let kind = visuals::get(folder)?.id.to_string();
    let params = serde_json::from_str::<Value>(text).ok()?;
    let (factory, same) = factory::status(rel, text);
    Some(PresetDef { name: factory::stem(file).to_string(), visual: kind, params, factory, edited: factory && !same })
}

/// Every preset: the factory set alone on a dev run, else the user's folder
/// (with the factory files written into it first if they are missing).
pub fn all() -> Vec<PresetDef> {
    let mut out = Vec::new();
    if factory::embedded_only() {
        out.extend(factory::embedded("presets/").filter_map(|(rel, text)| parse(rel, text)));
    } else {
        if let Err(e) = factory::install("presets/", false) {
            tracing::warn!("could not install factory presets: {e}");
        }
        for def in visuals::all() {
            let Some(d) = dir(def.id) else { continue };
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&p) else { continue };
                let file = p.file_name().and_then(|s| s.to_str()).unwrap_or("preset.json");
                if let Some(def) = parse(&format!("presets/{}/{}", def.id, file), &text) {
                    out.push(def);
                }
            }
        }
    }
    // Factory first, then the user's, each by name.
    out.sort_by(|a, b| b.factory.cmp(&a.factory).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

/// Save the layer's current visual parameters under `name`.
pub fn save(name: &str, layer: &Layer) -> anyhow::Result<String> {
    let name = safe_name(name);
    anyhow::ensure!(!name.is_empty(), "name is empty");
    anyhow::ensure!(!factory::embedded_only(), "a dev run does not write presets");
    let d = dir(&layer.visual).ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    std::fs::create_dir_all(&d)?;
    let text = serde_json::to_string_pretty(&params_of(layer))?;
    std::fs::write(d.join(format!("{name}.json")), text)?;
    Ok(name)
}

/// Write the factory presets of this visual back over the folder.
pub fn restore_factory(visual: &str) -> anyhow::Result<usize> {
    factory::install(&format!("presets/{visual}/"), true)
}

/// Delete one user preset.
pub fn delete(visual: &str, name: &str) -> anyhow::Result<()> {
    if let Some(d) = dir(visual) {
        let path = d.join(format!("{}.json", safe_name(name)));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factory_presets() -> Vec<PresetDef> {
        factory::embedded("presets/").filter_map(|(rel, text)| parse(rel, text)).collect()
    }

    #[test]
    fn apply_sets_only_the_visuals_parameters() {
        let presets = factory_presets();
        let star = presets.iter().find(|p| p.name == "Starfield" && p.visual == "ribbons").expect("factory Starfield");
        let mut l = Layer::new("scope", true, 0.4);
        l.palette.name = "neon".into();
        l.opacity = 0.4;
        apply(&mut l, star).unwrap();
        assert_eq!(l.visual, "ribbons");
        let r: crate::settings::RibbonsParams = l.get("ribbons");
        assert_eq!(r.count, crate::scenes::ribbons::PARTICLES);
        assert!((r.persistence - 0.001).abs() < 1e-6);
        assert_eq!(l.palette.name, "neon", "palette is the layer's, not the preset's");
        assert_eq!(l.opacity, 0.4);
    }

    #[test]
    fn every_factory_preset_round_trips() {
        let presets = factory_presets();
        assert!(presets.len() >= 9);
        for p in presets {
            assert!(p.factory && !p.edited, "{}", p.name);
            let mut l = Layer::default();
            apply(&mut l, &p).unwrap();
            assert_eq!(params_of(&l), p.params, "{}", p.name);
        }
    }
}
