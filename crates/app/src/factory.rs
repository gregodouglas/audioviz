//! Factory presets and scenes: the JSON files under `assets/presets/` and
//! `assets/scenes/` in the repo, embedded at build time (see `build.rs`).
//! On a normal run they are written into the user's audioviz folder once,
//! as ordinary files beside the user's own; from then on the app only reads
//! the folder, so a built-in is edited or replaced like any other file.
//! "Restore factory" writes the embedded copies back.
//!
//! A dev run with `AUDIOVIZ_DEFAULTS` never writes into the user's folder;
//! the stores read the embedded set directly instead.

use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/factory_list.rs"));

/// The user's audioviz folder (`%APPDATA%\audioviz` on Windows).
pub fn root() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("audioviz"))
}

/// Dev runs read the embedded set and never touch the user's folder.
pub fn embedded_only() -> bool {
    std::env::var_os("AUDIOVIZ_DEFAULTS").is_some()
}

/// Factory files whose relative path starts with `prefix` ("presets/ribbons", "scenes").
pub fn embedded(prefix: &str) -> impl Iterator<Item = (&'static str, &'static str)> {
    FACTORY.iter().copied().filter(move |(rel, _)| rel.starts_with(prefix))
}

/// Write the factory files under `prefix` into the user's folder. With
/// `force` every file is rewritten (restore); otherwise only missing ones
/// are written (first run). Returns how many were written.
pub fn install(prefix: &str, force: bool) -> anyhow::Result<usize> {
    if embedded_only() {
        return Ok(0);
    }
    let root = root().ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    let mut n = 0;
    for (rel, text) in embedded(prefix) {
        let path = root.join(rel);
        if force || !path.exists() {
            if let Some(d) = path.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(&path, text)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Whether `rel` is a factory file, and whether the file on disk still
/// matches the embedded copy.
pub fn status(rel: &str, on_disk: &str) -> (bool, bool) {
    match FACTORY.iter().find(|(r, _)| *r == rel) {
        Some((_, text)) => (true, same_json(text, on_disk)),
        None => (false, false),
    }
}

fn same_json(a: &str, b: &str) -> bool {
    let pa: Option<serde_json::Value> = serde_json::from_str(a).ok();
    let pb: Option<serde_json::Value> = serde_json::from_str(b).ok();
    pa.is_some() && pa == pb
}

/// The folder for `prefix` under the user's audioviz folder.
pub fn dir(prefix: &str) -> Option<PathBuf> {
    root().map(|r| r.join(prefix))
}

/// File stem of a relative factory path.
pub fn stem(rel: &str) -> &str {
    Path::new(rel).file_stem().and_then(|s| s.to_str()).unwrap_or(rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_set_is_embedded_and_parses() {
        assert!(FACTORY.iter().any(|(r, _)| *r == "presets/ribbons/Starfield.json"));
        assert!(FACTORY.iter().any(|(r, _)| *r == "scenes/Lantern.json"));
        for (rel, text) in FACTORY {
            assert!(serde_json::from_str::<serde_json::Value>(text).is_ok(), "{rel}");
        }
    }
}
