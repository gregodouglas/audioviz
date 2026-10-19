//! The visual registry: every visual the app can draw, by id. A visual is a
//! `Scene` (it renders one layer's picture) plus a `VisualDef` that names it,
//! describes it, builds it, and knows its parameters. Everything else a layer
//! has (band, tint, palette, blend, opacity, the effect chain and its
//! director, presets, scenes, the Performance timings) is the engine's and
//! applies to any visual without the visual knowing.
//!
//! Adding a built-in visual: one module under `scenes/` with its `Scene`,
//! its parameter struct and a `def()`, and one line in `builtin()` below.
//! The id is what layers, scene files and the presets folder use, so it
//! never changes once shipped.

use std::sync::OnceLock;

use serde_json::Value;

use crate::scene::Scene;
use crate::scenes;
use crate::shaders::ShaderSet;

pub type Maker = fn(&wgpu::Device, u32, u32, &ShaderSet) -> Result<Box<dyn Scene>, String>;

pub struct VisualDef {
    /// Stable id: the layer's `visual`, its parameter key, the presets folder.
    pub id: &'static str,
    /// Full name for the Visual list ("Phosphor scope").
    pub name: &'static str,
    /// Short name for the bar, the stack and breadcrumbs ("Scope").
    pub short: &'static str,
    /// One line for tooltips.
    pub summary: &'static str,
    pub make: Maker,
    /// This visual's full parameters from what a layer stores (its defaults
    /// filled in where nothing is stored), as JSON.
    pub params: fn(Option<&Value>) -> Value,
    /// Draws the waveform itself, so a layer band filters the waveform (the
    /// scope); other visuals take band-limited features.
    pub band_waveform: bool,
}

/// `params` for a visual whose parameters are the struct `T`.
pub fn typed<T: serde::Serialize + serde::de::DeserializeOwned + Default>(stored: Option<&Value>) -> Value {
    let t: T = stored.and_then(|v| T::deserialize(v).ok()).unwrap_or_default();
    serde_json::to_value(t).unwrap_or(Value::Null)
}

fn builtin() -> Vec<VisualDef> {
    vec![
        scenes::scope::def(),
        scenes::ribbons::def(),
        scenes::starfield::def(),
        scenes::kaleido::def(),
        scenes::sigil::def(),
        scenes::waves::def(),
        scenes::crystal::def(),
        scenes::wheel::def(),
        scenes::loops::def(),
        scenes::eclipse::def(),
        scenes::ripples::def(),
        scenes::spiral::def(),
    ]
}

/// Every visual, in the order the Visual list shows them.
pub fn all() -> &'static [VisualDef] {
    static ALL: OnceLock<Vec<VisualDef>> = OnceLock::new();
    ALL.get_or_init(builtin)
}

pub fn get(id: &str) -> Option<&'static VisualDef> {
    all().iter().find(|d| d.id == id)
}

/// The full name, or the id itself for a visual that is not installed.
pub fn name(id: &str) -> &str {
    get(id).map_or(id, |d| d.name)
}

pub fn short(id: &str) -> &str {
    get(id).map_or(id, |d| d.short)
}

pub fn summary(id: &str) -> &'static str {
    get(id).map_or("This visual is not installed; the layer draws the scope until it comes back.", |d| d.summary)
}

/// Build visual `id`, or the scope in its place when it is not installed.
pub fn make(id: &str, device: &wgpu::Device, w: u32, h: u32, shaders: &ShaderSet) -> Result<Box<dyn Scene>, String> {
    let def = get(id).or_else(|| get(scenes::scope::ID)).expect("the scope is built in");
    (def.make)(device, w, h, shaders)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_every_visual_has_defaults() {
        let mut seen = std::collections::HashSet::new();
        for d in all() {
            assert!(seen.insert(d.id), "{} twice", d.id);
            assert!((d.params)(None).is_object(), "{}", d.id);
            assert!(!d.name.is_empty() && !d.short.is_empty() && !d.summary.is_empty());
        }
    }
}
