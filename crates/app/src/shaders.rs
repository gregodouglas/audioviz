//! WGSL sources with dev-time hot reload. In a dev build the files under
//! `crates/app/shaders/` are read from disk and polled for changes; the
//! embedded copies are the fallback (and the only source in release).
//! Files ending in `_params.wgsl` plus `params.wgsl` form a prelude that is
//! prepended to every other shader.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

macro_rules! shader_table {
    ($($name:literal),* $(,)?) => {
        pub const NAMES: &[&str] = &[$($name),*];
        const EMBEDDED: &[&str] = &[$(include_str!(concat!("../shaders/", $name))),*];
    };
}

shader_table!(
    "params.wgsl",
    "visual_api.wgsl",
    "visual_lines.wgsl",
    "bloom_down.wgsl",
    "bloom_up.wgsl",
    "expo.wgsl",
    "present.wgsl",
    "mix.wgsl",
    "layer.wgsl",
    "layer_blend.wgsl",
    "layer_down.wgsl",
    "layer_meter.wgsl",
    "fold.wgsl",
    "feedback.wgsl",
    "polar.wgsl",
    "ripple.wgsl",
    "edge.wgsl",
    "chroma.wgsl",
    "mask.wgsl",
    "fractal.wgsl",
    "blit.wgsl",
    "lumi.wgsl",
    "grade.wgsl",
    "halftone.wgsl",
    "warp.wgsl",
    "lines.wgsl",
    "mirror.wgsl",
    "glow_blur.wgsl",
    "visual_light.wgsl",
    "glow.wgsl",
    "vignette.wgsl",
    "colorify.wgsl",
    "library/creatures.wgsl",
    "library/luminous.wgsl",
    "library/jelly.wgsl",
);

fn is_prelude(name: &str) -> bool {
    name == "params.wgsl" || name.ends_with("_params.wgsl")
}

pub struct ShaderSet {
    sources: Vec<String>,
    dir: Option<PathBuf>,
    mtimes: Vec<Option<SystemTime>>,
    last_poll: Instant,
}

impl ShaderSet {
    pub fn load() -> Self {
        // AUDIOVIZ_REPO=<checkout> reads that checkout's files in any build.
        let dir = match std::env::var_os("AUDIOVIZ_REPO") {
            Some(r) => Some(PathBuf::from(r).join("crates").join("app").join("shaders")),
            None if cfg!(debug_assertions) => Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shaders")),
            None => None,
        }
        .filter(|d| d.is_dir());
        let mut set = Self { sources: EMBEDDED.iter().map(|s| s.to_string()).collect(), dir, mtimes: vec![None; NAMES.len()], last_poll: Instant::now() };
        set.read_all();
        set
    }

    /// Full source for a shader: the prelude(s) plus the file.
    pub fn source(&self, name: &str) -> String {
        let i = NAMES.iter().position(|n| *n == name).unwrap_or_else(|| panic!("unknown shader {name}"));
        let mut out = String::new();
        for (j, n) in NAMES.iter().enumerate() {
            if is_prelude(n) {
                out.push_str(&self.sources[j]);
                out.push('\n');
            }
        }
        out.push_str(&self.sources[i]);
        out
    }

    /// One file as it is, nothing in front (a library a loaded visual opts in to).
    pub fn raw(&self, name: &str) -> &str {
        let i = NAMES.iter().position(|n| *n == name).unwrap_or_else(|| panic!("unknown shader {name}"));
        &self.sources[i]
    }

    /// The shared `params.wgsl` plus the file, without the visuals' own
    /// preludes: loaded visuals see only the API, so a built-in's helper
    /// names never collide with theirs.
    pub fn source_base(&self, name: &str) -> String {
        let i = NAMES.iter().position(|n| *n == name).unwrap_or_else(|| panic!("unknown shader {name}"));
        format!("{}\n{}", self.sources[0], self.sources[i])
    }

    fn read_all(&mut self) {
        let Some(dir) = &self.dir else { return };
        for (i, name) in NAMES.iter().enumerate() {
            let path = dir.join(name);
            if let Ok(text) = std::fs::read_to_string(&path) {
                self.sources[i] = text;
                self.mtimes[i] = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            }
        }
    }

    /// Returns true when any file changed on disk (and reloads the sources).
    pub fn poll(&mut self) -> bool {
        let Some(dir) = &self.dir else { return false };
        if self.last_poll.elapsed() < Duration::from_millis(300) {
            return false;
        }
        self.last_poll = Instant::now();
        let mut changed = false;
        for (i, name) in NAMES.iter().enumerate() {
            let path = dir.join(name);
            let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            if mtime.is_some() && mtime != self.mtimes[i] {
                changed = true;
            }
        }
        if changed {
            self.read_all();
        }
        changed
    }
}
