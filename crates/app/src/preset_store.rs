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
use crate::settings::{Layer, VisualKind};

#[derive(Clone, Debug)]
pub struct PresetDef {
    pub name: String,
    pub visual: VisualKind,
    /// The visual's parameter struct, as JSON.
    pub params: Value,
    /// Shipped with the app (a factory file).
    pub factory: bool,
    /// A factory file the user has changed.
    pub edited: bool,
}

/// The JSON field of `Layer` that holds this visual's parameters, and the
/// folder name of its presets.
pub fn field(kind: VisualKind) -> &'static str {
    match kind {
        VisualKind::Scope => "scope",
        VisualKind::Ribbons => "ribbons",
        VisualKind::Starfield => "starfield",
        VisualKind::Kaleido => "kaleido",
        VisualKind::Sigil => "sigil",
        VisualKind::Waves => "waves",
        VisualKind::Crystal => "crystal",
        VisualKind::Wheel => "wheel",
        VisualKind::Loop => "loop",
        VisualKind::Eclipse => "eclipse",
        VisualKind::Ripples => "ripples",
    }
}

fn kind_of(folder: &str) -> Option<VisualKind> {
    VisualKind::ALL.into_iter().find(|k| field(*k) == folder)
}

/// This layer's current parameters for its visual, as JSON.
pub fn params_of(layer: &Layer) -> Value {
    let mut v = serde_json::to_value(layer).unwrap_or(Value::Null);
    v.get_mut(field(layer.visual)).map(Value::take).unwrap_or(Value::Null)
}

/// Apply a preset's parameters to a layer (switching it to the preset's visual).
pub fn apply(layer: &mut Layer, preset: &PresetDef) -> anyhow::Result<()> {
    let mut v = serde_json::to_value(&*layer)?;
    v[field(preset.visual)] = preset.params.clone();
    let mut next: Layer = serde_json::from_value(v)?;
    next.visual = preset.visual;
    next.normalize();
    *layer = next;
    Ok(())
}

/// The visual's own defaults as a preset.
pub fn defaults(kind: VisualKind) -> PresetDef {
    let fresh = Layer::new(kind, true, 1.0);
    PresetDef { name: String::new(), visual: kind, params: params_of(&fresh), factory: true, edited: false }
}

pub fn dir(kind: VisualKind) -> Option<PathBuf> {
    factory::dir("presets").map(|d| d.join(field(kind)))
}

fn safe_name(name: &str) -> String {
    name.trim().chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').take(48).collect::<String>().trim().to_string()
}

fn parse(rel: &str, text: &str) -> Option<PresetDef> {
    let mut parts = rel.split('/');
    let (_, folder, file) = (parts.next()?, parts.next()?, parts.next()?);
    let kind = kind_of(folder)?;
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
        for kind in VisualKind::ALL {
            let Some(d) = dir(kind) else { continue };
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&p) else { continue };
                let file = p.file_name().and_then(|s| s.to_str()).unwrap_or("preset.json");
                if let Some(def) = parse(&format!("presets/{}/{}", field(kind), file), &text) {
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
    let d = dir(layer.visual).ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    std::fs::create_dir_all(&d)?;
    let text = serde_json::to_string_pretty(&params_of(layer))?;
    std::fs::write(d.join(format!("{name}.json")), text)?;
    Ok(name)
}

/// Write the factory presets of this visual back over the folder.
pub fn restore_factory(kind: VisualKind) -> anyhow::Result<usize> {
    factory::install(&format!("presets/{}/", field(kind)), true)
}

/// Delete one user preset.
pub fn delete(kind: VisualKind, name: &str) -> anyhow::Result<()> {
    if let Some(d) = dir(kind) {
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
        let star = presets.iter().find(|p| p.name == "Starfield" && p.visual == VisualKind::Ribbons).expect("factory Starfield");
        let mut l = Layer::new(VisualKind::Scope, true, 0.4);
        l.palette.name = "neon".into();
        l.opacity = 0.4;
        apply(&mut l, star).unwrap();
        assert_eq!(l.visual, VisualKind::Ribbons);
        assert_eq!(l.ribbons.count, crate::scenes::ribbons::PARTICLES);
        assert!((l.ribbons.persistence - 0.001).abs() < 1e-6);
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
