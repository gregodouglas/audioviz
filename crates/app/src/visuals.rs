//! The visual registry: every visual the app can draw, by id. A visual is a
//! `Scene` (it renders one layer's picture) plus a `VisualDef` that names it,
//! describes it, builds it, and knows its parameters. Everything else a layer
//! has (band, tint, palette, blend, opacity, the effect chain and its
//! director, presets, scenes, the Performance timings) is the engine's and
//! applies to any visual without the visual knowing.
//!
//! Every visual is loaded from a folder (`shader_visual`): the shipped ones
//! from `assets/visuals/`, a person's own from their visuals folder. The id
//! is what layers, scene files and the presets folder use, so it never
//! changes once shipped.

use std::path::PathBuf;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::scene::Scene;
use crate::shader_visual::{self, ShaderDef, ShaderVisual};
use crate::shaders::ShaderSet;

/// The scope: what a layer draws while its visual is not installed.
pub const SCOPE: &str = "scope";

/// Where a visual comes from. Every visual is loaded from a folder today;
/// a compiled-in kind would be another variant here.
pub enum Source {
    /// Loaded from a folder (`shader_visual`).
    Loaded(&'static ShaderDef),
}

pub struct VisualDef {
    /// Stable id: the layer's `visual`, its parameter key, the presets folder.
    pub id: &'static str,
    /// Full name for the Visual list ("Phosphor scope").
    pub name: &'static str,
    /// Short name for the bar, the stack and breadcrumbs ("Scope").
    pub short: &'static str,
    /// One line for tooltips.
    pub summary: &'static str,
    pub source: Source,
    /// Draws the waveform itself, so a layer band filters the waveform (the
    /// scope); other visuals take band-limited features.
    pub band_waveform: bool,
}

/// The registry: every visual loaded from a folder whose id is not already
/// taken. It only grows while the app runs (a folder added
/// later is picked up by `refresh`); entries live for the life of the app.
fn registry() -> &'static RwLock<Vec<&'static VisualDef>> {
    static ALL: std::sync::OnceLock<RwLock<Vec<&'static VisualDef>>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut v: Vec<&'static VisualDef> = Vec::new();
        for d in shader_visual::discover() {
            push_loaded(&mut v, d);
        }
        // A shipped visual whose folder is missing or does not parse still
        // loads from the copy inside the app (the scope is everyone's fallback).
        for d in shader_visual::embedded() {
            if !v.iter().any(|b| b.id == d.id) {
                push_loaded(&mut v, d);
            }
        }
        RwLock::new(v)
    })
}

fn push_loaded(v: &mut Vec<&'static VisualDef>, d: ShaderDef) -> Option<&'static VisualDef> {
    if v.iter().any(|b| b.id == d.id) {
        tracing::warn!("visual {}: the id is taken by another visual; skipped", d.id);
        return None;
    }
    let d: &'static ShaderDef = Box::leak(Box::new(d));
    let def: &'static VisualDef = Box::leak(Box::new(VisualDef { id: &d.id, name: &d.name, short: &d.short, summary: &d.summary, source: Source::Loaded(d), band_waveform: d.band_waveform }));
    v.push(def);
    Some(def)
}

/// Every visual, in the order the Visual list shows them.
pub fn all() -> Vec<&'static VisualDef> {
    registry().read().map(|v| v.clone()).unwrap_or_default()
}

/// Register a visual loaded from a folder after start (a new visual from the
/// panel). None when its id is taken.
pub fn add_loaded(d: ShaderDef) -> Option<&'static VisualDef> {
    registry().write().ok().and_then(|mut v| push_loaded(&mut v, d))
}

/// Pick up visual folders added since start, at most every two seconds.
/// Returns how many were added.
pub fn refresh() -> usize {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let Ok(mut last) = LAST.lock() else { return 0 };
        if last.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
            return 0;
        }
        *last = Some(Instant::now());
    }
    let known: Vec<PathBuf> = all().iter().filter_map(|d| match &d.source {
        Source::Loaded(s) => s.dir.clone(),
    }).collect();
    let added: Vec<&str> = shader_visual::rescan(&known).into_iter().filter_map(add_loaded).map(|d| d.id).collect();
    if !added.is_empty() {
        tracing::info!("visuals added while running: {}", added.join(", "));
    }
    added.len()
}

impl VisualDef {
    /// Full parameters from what a layer stores, defaults filled in.
    pub fn params(&self, stored: Option<&Value>) -> Value {
        match &self.source {
            Source::Loaded(d) => d.params_json(stored),
        }
    }

    pub fn make(&'static self, device: &wgpu::Device, w: u32, h: u32, shaders: &ShaderSet) -> Result<Box<dyn Scene>, String> {
        match &self.source {
            Source::Loaded(d) => Ok(Box::new(ShaderVisual::new(device, w, h, shaders, d)?)),
        }
    }
}

pub fn get(id: &str) -> Option<&'static VisualDef> {
    registry().read().ok().and_then(|v| v.iter().find(|d| d.id == id).copied())
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
    match get(id).or_else(|| get(SCOPE)) {
        Some(def) => def.make(device, w, h, shaders),
        None => Err(format!("no visual \"{id}\" and no scope to stand in")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_every_visual_has_defaults() {
        let mut seen = std::collections::HashSet::new();
        for d in all() {
            assert!(seen.insert(d.id), "{} twice", d.id);
            assert!(d.params(None).is_object(), "{}", d.id);
            assert!(!d.name.is_empty() && !d.short.is_empty() && !d.summary.is_empty());
        }
    }
}
