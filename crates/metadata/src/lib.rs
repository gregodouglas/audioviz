//! Now-playing metadata (track, artist, album, art, position) from the OS
//! media session, no API keys. Windows: System Media Transport Controls.
//! macOS: pending (AppleScript or Web API per docs/research/01-audio-ingest.md).

use std::sync::{Arc, Mutex};
use std::time::Instant;

#[cfg(windows)]
pub mod windows;

#[derive(Debug, Clone)]
pub struct ArtImage {
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major.
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    /// Seconds into the track, extrapolated from the last report.
    pub position_s: f64,
    pub duration_s: f64,
    pub art: Option<Arc<ArtImage>>,
    /// Changes whenever title/artist/album change.
    pub track_id: u64,
    pub updated: Instant,
}

impl NowPlaying {
    pub fn progress(&self) -> f32 {
        if self.duration_s > 0.0 { (self.position_s / self.duration_s).clamp(0.0, 1.0) as f32 } else { 0.0 }
    }
}

pub fn track_id(title: &str, artist: &str, album: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    title.hash(&mut h);
    artist.hash(&mut h);
    album.hash(&mut h);
    h.finish()
}

pub struct MetadataHandle {
    latest: Arc<Mutex<Option<NowPlaying>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl MetadataHandle {
    pub fn latest(&self) -> Option<NowPlaying> {
        self.latest.lock().ok().and_then(|g| g.clone())
    }
}

impl Drop for MetadataHandle {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub trait MetadataSource: Send + Sync {
    /// Start polling on a background thread. `app_filter` is a case-insensitive
    /// substring of the source app id to prefer (e.g. "spotify"); when no
    /// matching session exists, the system's current session is used.
    fn start(&self, app_filter: &str) -> anyhow::Result<MetadataHandle>;
}

pub fn default_source() -> Box<dyn MetadataSource> {
    #[cfg(windows)]
    {
        Box::new(windows::SmtcSource)
    }
    #[cfg(not(windows))]
    {
        Box::new(NullSource)
    }
}

pub struct NullSource;

impl MetadataSource for NullSource {
    fn start(&self, _app_filter: &str) -> anyhow::Result<MetadataHandle> {
        Ok(MetadataHandle { latest: Arc::new(Mutex::new(None)), stop: Arc::new(std::sync::atomic::AtomicBool::new(false)) })
    }
}

pub(crate) fn decode_art(bytes: &[u8]) -> Option<ArtImage> {
    let img = image::load_from_memory(bytes).ok()?;
    // Cap size: this is a small tile in a type grid, not a poster.
    let img = if img.width() > 512 || img.height() > 512 { img.thumbnail(512, 512) } else { img };
    let rgba = img.to_rgba8();
    Some(ArtImage { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
}
