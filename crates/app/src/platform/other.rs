//! Non-Windows fallback: every entry point is a no-op or `None` so the macOS
//! CI build compiles. The shipping target is Windows; see `windows.rs`.

use super::ReleaseInfo;

/// Not supported off Windows; always returns an error.
pub fn open_url(url: &str) -> anyhow::Result<()> {
    anyhow::bail!("opening {url} is only supported on Windows")
}

/// Not supported off Windows; always returns an error.
pub fn launch_spotify() -> anyhow::Result<()> {
    anyhow::bail!("launching Spotify is only supported on Windows")
}

/// Always `false` off Windows.
pub fn spotify_installed() -> bool {
    false
}

/// Always `None` off Windows (power state unknown).
pub fn on_battery() -> Option<bool> {
    None
}

/// The update check is Windows-only (it uses WinHTTP); always returns an error.
pub fn latest_release(owner_repo: &str) -> anyhow::Result<ReleaseInfo> {
    anyhow::bail!("checking {owner_repo} for updates is only supported on Windows")
}

/// One line naming the app version, OS and architecture.
pub fn diagnostics_header() -> String {
    format!("audioviz {} on {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH)
}
