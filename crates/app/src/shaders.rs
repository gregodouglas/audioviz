//! WGSL sources with dev-time hot reload. In a dev build the files under
//! `crates/app/shaders/` are read from disk and polled for changes; the
//! embedded copies are the fallback (and the only source in release).

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

pub const NAMES: [&str; 3] = ["decay.wgsl", "beam.wgsl", "present.wgsl"];
const EMBEDDED: [&str; 3] = [
    include_str!("../shaders/decay.wgsl"),
    include_str!("../shaders/beam.wgsl"),
    include_str!("../shaders/present.wgsl"),
];

pub struct ShaderSet {
    pub sources: Vec<String>,
    dir: Option<PathBuf>,
    mtimes: Vec<Option<SystemTime>>,
    last_poll: Instant,
}

impl ShaderSet {
    pub fn load() -> Self {
        let dir = if cfg!(debug_assertions) {
            let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shaders");
            d.is_dir().then_some(d)
        } else {
            None
        };
        let mut set = Self { sources: EMBEDDED.iter().map(|s| s.to_string()).collect(), dir, mtimes: vec![None; NAMES.len()], last_poll: Instant::now() };
        set.read_all();
        set
    }

    pub fn source(&self, name: &str) -> &str {
        let i = NAMES.iter().position(|n| *n == name).expect("unknown shader");
        &self.sources[i]
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
