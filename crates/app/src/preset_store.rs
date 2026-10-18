//! Visual presets: a named set of one visual's parameters. A few ship built
//! in; user presets are JSON files in `<config dir>/audioviz/presets/<visual>/<name>.json`.
//! A preset only ever sets the visual's own parameters: not the palette, band,
//! blend, opacity or effects, which belong to the layer.

use std::path::PathBuf;

use serde_json::Value;

use crate::settings::{CrystalStyle, Layer, VisualKind, WavesSource};

#[derive(Clone, Debug)]
pub struct PresetDef {
    pub name: String,
    pub visual: VisualKind,
    /// The visual's parameter struct, as JSON.
    pub params: Value,
    pub builtin: bool,
}

/// The JSON field of `Layer` that holds this visual's parameters.
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
    }
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

fn preset(name: &str, visual: VisualKind, f: impl FnOnce(&mut Layer)) -> PresetDef {
    let mut l = Layer::new(visual, true, 1.0);
    f(&mut l);
    PresetDef { name: name.into(), visual, params: params_of(&l), builtin: true }
}

pub fn builtin() -> Vec<PresetDef> {
    vec![
        preset("Starfield", VisualKind::Ribbons, |l| {
            // Greg's: every particle, no trail, slow and small: twinkling distant stars.
            l.ribbons.count = crate::scenes::ribbons::PARTICLES;
            l.ribbons.persistence = 0.001;
            l.ribbons.speed = 0.033;
            l.ribbons.scale = 0.4;
            l.ribbons.intensity = 10.0;
        }),
        preset("Deep field", VisualKind::Ribbons, |l| {
            l.ribbons.speed = 0.08;
            l.ribbons.count = 3000;
            l.ribbons.intensity = 0.5;
            l.ribbons.persistence = 0.6;
            l.ribbons.saturation = 0.2;
        }),
        preset("Sparks", VisualKind::Ribbons, |l| {
            l.ribbons.speed = 0.6;
            l.ribbons.count = 1200;
            l.ribbons.intensity = 1.6;
            l.ribbons.persistence = 0.05;
            l.ribbons.width = 0.6;
        }),
        preset("Fine beam", VisualKind::Scope, |l| {
            l.scope.beam_width = 0.6;
            l.scope.beam_intensity = 2.0;
            l.scope.persistence = 0.01;
        }),
        preset("Long phosphor", VisualKind::Scope, |l| {
            l.scope.beam_width = 1.6;
            l.scope.persistence = 0.5;
        }),
        preset("Quiet sky", VisualKind::Starfield, |l| {
            l.starfield.disk = 0.0;
            l.starfield.density = 1.6;
            l.starfield.twinkle = 0.8;
            l.starfield.streams = 0.0;
        }),
        preset("Spectrum ridge", VisualKind::Waves, |l| {
            l.waves.source = WavesSource::Spectrum;
            l.waves.rows = 64;
            l.waves.rate = 18.0;
            l.waves.height = 0.5;
        }),
        preset("Cut stone", VisualKind::Crystal, |l| {
            l.crystal.style = CrystalStyle::Solid;
            l.crystal.count = 5;
            l.crystal.size = 0.7;
        }),
        preset("Rays only", VisualKind::Wheel, |l| {
            l.wheel.hole = 0.9;
            l.wheel.rays = 1.6;
            l.wheel.radius = 0.5;
        }),
    ]
}

pub fn dir(kind: VisualKind) -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("audioviz").join("presets").join(field(kind)))
}

fn safe_name(name: &str) -> String {
    name.trim().chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').take(48).collect::<String>().trim().to_string()
}

pub fn load_user() -> Vec<PresetDef> {
    let mut out = Vec::new();
    for kind in VisualKind::ALL {
        let Some(d) = dir(kind) else { continue };
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let Ok(params) = serde_json::from_str::<Value>(&text) else { continue };
            let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("preset").to_string();
            out.push(PresetDef { name, visual: kind, params, builtin: false });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

pub fn all() -> Vec<PresetDef> {
    let mut v = builtin();
    v.extend(load_user());
    v
}

/// Save the layer's current visual parameters under `name`.
pub fn save(name: &str, layer: &Layer) -> anyhow::Result<String> {
    let name = safe_name(name);
    anyhow::ensure!(!name.is_empty(), "name is empty");
    let d = dir(layer.visual).ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    std::fs::create_dir_all(&d)?;
    let text = serde_json::to_string_pretty(&params_of(layer))?;
    std::fs::write(d.join(format!("{name}.json")), text)?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_sets_only_the_visuals_parameters() {
        let presets = builtin();
        let star = presets.iter().find(|p| p.name == "Starfield").unwrap();
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
    fn every_builtin_round_trips() {
        for p in builtin() {
            let mut l = Layer::default();
            apply(&mut l, &p).unwrap();
            assert_eq!(params_of(&l), p.params, "{}", p.name);
        }
    }
}
