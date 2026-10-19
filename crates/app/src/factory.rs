//! Factory presets and scenes: the JSON files under `assets/presets/` and
//! `assets/scenes/` in the repo, embedded at build time (see `build.rs`).
//! On a normal run they are written into the user's audioviz folder once,
//! as ordinary files beside the user's own; from then on the app only reads
//! the folder, so a built-in is edited or replaced like any other file.
//! "Restore factory" writes the embedded copies back.
//!
//! A copy the user never edited is brought up to date when a newer app ships
//! a changed version: `assets/factory-history.txt` lists a fingerprint of
//! every version of every factory file that ever shipped, so a file on disk
//! that matches an older one is unedited by definition, and anything else
//! (edited, saved over) is left alone.
//!
//! A dev run with `AUDIOVIZ_DEFAULTS` never writes into the user's folder;
//! the stores read the embedded set directly instead.

use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/factory_list.rs"));

/// `<relative path> <fingerprint>` for every version of every factory file
/// that ever shipped. Rewrite it after changing a factory file:
/// `cargo test -p audioviz rewrite_factory_history -- --ignored`.
const HISTORY: &str = include_str!("../../../assets/factory-history.txt");

/// FNV-1a over the text, CRLF read as LF (a checkout may convert line endings).
fn fingerprint(text: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let b = text.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c == b'\r' && b.get(i + 1) == Some(&b'\n') {
            continue;
        }
        h = (h ^ c as u64).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Whether `text` is a version of factory file `rel` that shipped.
fn shipped(rel: &str, text: &str) -> bool {
    let want = format!("{:016x}", fingerprint(text));
    HISTORY.lines().any(|l| l.rsplit_once(' ').is_some_and(|(r, h)| r == rel && h == want))
}

/// The user's audioviz folder (`%APPDATA%\audioviz` on Windows).
pub fn root() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("audioviz"))
}

/// Dev runs read the embedded set and never touch the user's folder; nor
/// do tests (the registry they build would otherwise install visuals).
pub fn embedded_only() -> bool {
    cfg!(test) || std::env::var_os("AUDIOVIZ_DEFAULTS").is_some()
}

/// Factory files whose relative path starts with `prefix` ("presets/ribbons", "scenes").
pub fn embedded(prefix: &str) -> impl Iterator<Item = (&'static str, &'static str)> {
    FACTORY.iter().copied().filter(move |(rel, _)| rel.starts_with(prefix))
}

/// Write the factory files under `prefix` into the user's folder. With
/// `force` every file is rewritten (restore); otherwise missing ones are
/// written (first run) and unedited older copies are brought up to date.
/// Returns how many were written.
pub fn install(prefix: &str, force: bool) -> anyhow::Result<usize> {
    if embedded_only() {
        return Ok(0);
    }
    let root = root().ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    let mut n = 0;
    for (rel, text) in embedded(prefix) {
        let path = root.join(rel);
        let write = force
            || match std::fs::read_to_string(&path) {
                Ok(old) => upgrade(rel, &old, text),
                Err(_) => !path.exists(),
            };
        if write {
            if let Some(d) = path.parent() {
                std::fs::create_dir_all(d)?;
            }
            if !force && path.exists() {
                tracing::info!("factory file {rel}: unedited older version, updated");
            }
            std::fs::write(&path, text)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Whether the copy `old` of factory file `rel` should become `new`: it
/// differs, and it is a version that shipped (so nobody edited it).
fn upgrade(rel: &str, old: &str, new: &str) -> bool {
    fingerprint(old) != fingerprint(new) && shipped(rel, old)
}

/// Whether `rel` is a factory file, and whether the file on disk still
/// matches the embedded copy.
pub fn status(rel: &str, on_disk: &str) -> (bool, bool) {
    status_with(rel, on_disk, same_json)
}

/// `status` with the caller's idea of "the same" (scenes compare what they
/// mean, so a file in an older shape is not marked edited).
pub fn status_with(rel: &str, on_disk: &str, same: impl Fn(&str, &str) -> bool) -> (bool, bool) {
    match FACTORY.iter().find(|(r, _)| *r == rel) {
        Some((_, text)) => (true, same(text, on_disk)),
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

    /// Every factory file as it is now is in the history, so the next change
    /// to it can recognise this version as unedited.
    #[test]
    fn factory_history_covers_every_factory_file() {
        for (rel, text) in FACTORY.iter() {
            assert!(shipped(rel, text), "{rel} is not in assets/factory-history.txt: run cargo test -p audioviz rewrite_factory_history -- --ignored");
        }
    }

    #[test]
    fn only_unedited_older_copies_are_upgraded() {
        let (rel, now) = FACTORY.iter().copied().find(|(r, _)| r.ends_with(".json")).unwrap();
        assert!(!upgrade(rel, now, now), "the same file is left alone");
        assert!(!upgrade(rel, &format!("{now} "), now), "an edited file is left alone");
        assert!(!upgrade(rel, &now.replace('\n', "\r\n"), now), "line endings alone are not a difference");
        let old = format!("{now}x");
        let line = format!("{rel} {:016x}", fingerprint(&old));
        assert!(!HISTORY.contains(&line));
        assert_eq!(fingerprint("a\r\nb"), fingerprint("a\nb"));
    }

    /// Rewrites assets/factory-history.txt from git: every version of every
    /// file under assets/presets, assets/scenes and assets/visuals in any
    /// commit, plus the files as they are now. Read-only on git.
    #[test]
    #[ignore]
    fn rewrite_factory_history() {
        use std::collections::BTreeSet;
        use std::io::Write;
        use std::process::{Command, Stdio};
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let git = |args: &[&str]| Command::new("git").current_dir(&repo).args(args).output().expect("git");
        let log = git(&["log", "--all", "--format=", "--raw", "--no-abbrev", "--no-renames", "--", "assets/presets", "assets/scenes", "assets/visuals"]);
        let mut blobs: BTreeSet<(String, String)> = BTreeSet::new();
        for line in String::from_utf8_lossy(&log.stdout).lines().filter(|l| l.starts_with(':')) {
            let (meta, path) = line.split_once('\t').expect("raw line");
            let f: Vec<&str> = meta.split_whitespace().collect();
            let rel = path.trim_start_matches("assets/").to_string();
            for sha in [f[2], f[3]] {
                if sha.chars().any(|c| c != '0') {
                    blobs.insert((rel.clone(), sha.to_string()));
                }
            }
        }
        let mut lines: BTreeSet<String> = FACTORY.iter().map(|(rel, text)| format!("{rel} {:016x}", fingerprint(text))).collect();
        let mut cat = Command::new("git").current_dir(&repo).args(["cat-file", "--batch"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().expect("git cat-file");
        let input: String = blobs.iter().map(|(_, sha)| format!("{sha}\n")).collect();
        cat.stdin.take().expect("stdin").write_all(input.as_bytes()).expect("write");
        let out = cat.wait_with_output().expect("cat-file").stdout;
        let mut at = 0;
        for (rel, _) in &blobs {
            let nl = at + out[at..].iter().position(|&c| c == b'\n').expect("header");
            let header = String::from_utf8_lossy(&out[at..nl]).to_string();
            let size: usize = header.rsplit(' ').next().and_then(|s| s.parse().ok()).expect("size");
            let body = &out[nl + 1..nl + 1 + size];
            at = nl + 1 + size + 1;
            lines.insert(format!("{rel} {:016x}", fingerprint(&String::from_utf8_lossy(body))));
        }
        let text: String = lines.into_iter().map(|l| l + "\n").collect();
        std::fs::write(repo.join("assets/factory-history.txt"), text).expect("write history");
    }

    #[test]
    fn factory_set_is_embedded_and_parses() {
        assert!(FACTORY.iter().any(|(r, _)| *r == "presets/ribbons/Starfield.json"));
        assert!(FACTORY.iter().any(|(r, _)| *r == "scenes/Lantern.json"));
        for (rel, text) in FACTORY.iter().filter(|(r, _)| r.ends_with(".json")) {
            assert!(serde_json::from_str::<serde_json::Value>(text).is_ok(), "{rel}");
        }
    }
}
