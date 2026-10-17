//! Thin OS integration layer: open a URL or Spotify, read the power state,
//! build a diagnostics line, and the one sanctioned network call in the app -
//! the *manual* "Check for updates" fetch of the latest GitHub release tag.
//!
//! Everything OS-specific lives in `windows.rs` behind `#[cfg(windows)]` and
//! goes through the Windows-native APIs (`ShellExecuteW`, `GetSystemPowerStatus`,
//! the registry, WinHTTP). No Rust HTTP/TLS crate is involved, so `deny.toml`
//! keeps banning them and PRIVACY.md stays verifiable. `other.rs` is a no-op /
//! `None` fallback so the macOS CI build keeps compiling.
//!
//! The pure parts (semver comparison, GitHub JSON parsing) live here so they
//! are unit-tested on every platform.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::{diagnostics_header, latest_release, launch_spotify, on_battery, open_url, spotify_installed};

#[cfg(not(windows))]
mod other;
#[cfg(not(windows))]
pub use self::other::{diagnostics_header, latest_release, launch_spotify, on_battery, open_url, spotify_installed};

/// The newest published GitHub release, as returned by [`latest_release`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    /// The git tag of the release, e.g. `v0.1.0`.
    pub tag: String,
    /// The release page on github.com, suitable for [`open_url`].
    pub url: String,
}

/// Compare two `v0.2.0` / `0.2.0` style tags numerically and report whether
/// `tag` is strictly newer than `current`. A leading `v` or `V` is ignored,
/// so are pre-release / build suffixes (`0.2.0-rc1` compares as `0.2.0`).
/// Anything that does not parse as a `major.minor.patch` triple is treated
/// as not newer, so a garbled API response can never produce an update
/// prompt.
pub fn is_newer(tag: &str, current: &str) -> bool {
    match (parse_semver(tag), parse_semver(current)) {
        (Some(tag), Some(current)) => tag > current,
        _ => false,
    }
}

/// Parse `[v]MAJOR.MINOR.PATCH[-suffix][+build]` into a comparable triple.
fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim();
    let s = s.strip_prefix('v').or_else(|| s.strip_prefix('V')).unwrap_or(s);
    // Drop pre-release and build metadata; they are not part of the triple.
    let core = s.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Pull `tag_name` and `html_url` out of a GitHub `releases/latest` response.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_release_json(body: &[u8]) -> anyhow::Result<ReleaseInfo> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| anyhow::anyhow!("GitHub returned something that is not JSON ({e})"))?;
    let field = |name: &str| -> anyhow::Result<String> {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| anyhow::anyhow!("GitHub response has no `{name}` field"))
    };
    Ok(ReleaseInfo { tag: field("tag_name")?, url: field("html_url")? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_when_any_component_is_greater() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.1.1", "v0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("v0.10.0", "v0.9.0"), "components compare numerically, not lexically");
    }

    #[test]
    fn not_newer_when_equal_or_older() {
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "v0.1.0"));
        assert!(!is_newer("v0.1.0", "v0.2.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
    }

    #[test]
    fn malformed_is_never_newer() {
        assert!(!is_newer("latest", "0.1.0"));
        assert!(!is_newer("", "0.1.0"));
        assert!(!is_newer("v1.2", "0.1.0"));
        assert!(!is_newer("1.2.3.4", "0.1.0"));
        assert!(!is_newer("v9.9.9", "not-a-version"));
        assert!(!is_newer("v0.x.0", "0.1.0"));
    }

    #[test]
    fn suffixes_are_ignored() {
        assert!(is_newer("v0.2.0-rc1", "0.1.0"));
        assert!(!is_newer("v0.1.0-rc1", "0.1.0"));
        assert!(is_newer("0.1.1+build.7", "0.1.0"));
        assert_eq!(parse_semver(" v1.2.3 "), Some((1, 2, 3)));
    }

    #[test]
    fn release_json_round_trip() {
        let body = br#"{"tag_name":"v0.1.0","html_url":"https://github.com/o/r/releases/tag/v0.1.0","name":"x"}"#;
        let info = parse_release_json(body).unwrap();
        assert_eq!(info.tag, "v0.1.0");
        assert_eq!(info.url, "https://github.com/o/r/releases/tag/v0.1.0");
    }

    #[test]
    fn release_json_errors_are_readable() {
        let err = parse_release_json(b"<html>rate limited</html>").unwrap_err().to_string();
        assert!(err.contains("not JSON"), "{err}");
        let err = parse_release_json(br#"{"message":"Not Found"}"#).unwrap_err().to_string();
        assert!(err.contains("tag_name"), "{err}");
    }
}
