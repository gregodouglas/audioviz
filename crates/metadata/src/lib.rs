//! Now-playing metadata (track, artist, album, art, position) from the OS
//! media session, no API keys. Windows: System Media Transport Controls.
//! macOS: pending (AppleScript or Web API per docs/research/01-audio-ingest.md).

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[cfg(windows)]
pub mod windows;

/// Tint used when the art has no usable colour (grey, black, missing): the
/// phosphor green the rest of the UI defaults to.
pub const DEFAULT_TINT: [f32; 3] = [0.40, 1.0, 0.55];

#[derive(Debug, Clone)]
pub struct ArtImage {
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major.
    pub rgba: Vec<u8>,
    /// Dominant colour, computed once at decode time. See [`ArtImage::dominant_color`].
    tint: [f32; 3],
}

impl ArtImage {
    /// Build from raw RGBA8 pixels, computing the tint up front.
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        let tint = dominant_color(width, height, &rgba);
        Self { width, height, rgba, tint }
    }

    /// Dominant colour of the art as an RGB triple in 0..1, lifted so it reads
    /// as a tint on a black background (value >= 0.75, saturation 0.45..0.9).
    /// Falls back to [`DEFAULT_TINT`] for art with no saturated pixels.
    /// Cached at decode time, so this is free to call every frame.
    pub fn dominant_color(&self) -> [f32; 3] {
        self.tint
    }
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
    /// The session's source app id (Windows: `SourceAppUserModelId`), lowercased.
    pub source_app: String,
    /// True when `source_app` matches the `app_filter` given to
    /// [`MetadataSource::start`], i.e. we are reading the app we asked for
    /// rather than falling back to whatever session the system considers current.
    pub is_target: bool,
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

/// A transport command sent to the media session via [`MetadataHandle::control`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCommand {
    TogglePlayPause,
    Next,
    Previous,
}

pub struct MetadataHandle {
    latest: Arc<Mutex<Option<NowPlaying>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    commands: Sender<MediaCommand>,
}

impl MetadataHandle {
    pub fn latest(&self) -> Option<NowPlaying> {
        self.latest.lock().ok().and_then(|g| g.clone())
    }

    /// Send a transport command to the currently picked session. Non-blocking:
    /// the polling thread executes it, logs any failure, and re-polls at once
    /// so [`MetadataHandle::latest`] reflects the change quickly. A source with
    /// no transport (e.g. [`NullSource`]) silently ignores it.
    pub fn control(&self, cmd: MediaCommand) {
        // A closed receiver just means the backend has no transport; nothing to do.
        let _ = self.commands.send(cmd);
    }
}

impl Drop for MetadataHandle {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Shared state between a handle and its backend thread, built by [`MetadataSource`] impls.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct Shared {
    pub latest: Arc<Mutex<Option<NowPlaying>>>,
    pub stop: Arc<std::sync::atomic::AtomicBool>,
    pub commands: Receiver<MediaCommand>,
}

impl MetadataHandle {
    /// Create a handle plus the backend's half of the shared state.
    pub(crate) fn new() -> (Self, Shared) {
        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let handle = Self { latest: latest.clone(), stop: stop.clone(), commands: tx };
        (handle, Shared { latest, stop, commands: rx })
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

/// A source that never reports anything and ignores transport commands.
pub struct NullSource;

impl MetadataSource for NullSource {
    fn start(&self, _app_filter: &str) -> anyhow::Result<MetadataHandle> {
        // Dropping `Shared` closes the command receiver; `control` tolerates that.
        let (handle, _shared) = MetadataHandle::new();
        Ok(handle)
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn decode_art(bytes: &[u8]) -> Option<ArtImage> {
    let img = image::load_from_memory(bytes).ok()?;
    // Cap size: this is a small tile in a type grid, not a poster.
    let img = if img.width() > 512 || img.height() > 512 { img.thumbnail(512, 512) } else { img };
    let rgba = img.to_rgba8();
    Some(ArtImage::from_rgba(rgba.width(), rgba.height(), rgba.into_raw()))
}

/// RGB (0..1) -> HSV with h in 0..360.
fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = max;
    let s = if max > 0.0 { delta / max } else { 0.0 };
    let h = if delta <= 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (h, s, v)
}

/// HSV (h in 0..360) -> RGB (0..1).
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let c = v * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// Dominant colour of an RGBA8 image, lifted for use as a tint on black.
///
/// Stride-downsamples to at most ~4096 pixels, drops near-black and near-white
/// pixels, votes hue into 24 bins weighted by saturation * value, averages the
/// members of the winning bin, then clamps value to >= 0.75 and saturation to
/// 0.45..0.9. Returns [`DEFAULT_TINT`] when no pixel qualifies.
fn dominant_color(width: u32, height: u32, rgba: &[u8]) -> [f32; 3] {
    const BINS: usize = 24;
    const MAX_SAMPLES: usize = 4096;
    let n = (width as usize) * (height as usize);
    if n == 0 || rgba.len() < n * 4 {
        return DEFAULT_TINT;
    }
    let stride = n.div_ceil(MAX_SAMPLES).max(1);

    // Per bin: total weight, weighted sums of (h, s, v).
    let mut weight = [0.0f32; BINS];
    let mut sum = [[0.0f32; 3]; BINS];
    for px in rgba.chunks_exact(4).step_by(stride) {
        if px[3] < 128 {
            continue;
        }
        let (r, g, b) = (px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0);
        let (h, s, v) = rgb_to_hsv(r, g, b);
        if v < 0.15 || (s < 0.12 && v > 0.85) {
            continue;
        }
        let w = s * v;
        if w <= 0.0 {
            continue;
        }
        let bin = ((h / 360.0 * BINS as f32) as usize).min(BINS - 1);
        weight[bin] += w;
        sum[bin][0] += h * w;
        sum[bin][1] += s * w;
        sum[bin][2] += v * w;
    }

    let best = (0..BINS).max_by(|&a, &b| weight[a].total_cmp(&weight[b])).unwrap_or(0);
    let w = weight[best];
    if w <= 0.0 {
        return DEFAULT_TINT;
    }
    let h = sum[best][0] / w;
    let s = (sum[best][1] / w).clamp(0.45, 0.9);
    let v = (sum[best][2] / w).max(0.75);
    hsv_to_rgb(h, s, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        rgba.iter().copied().cycle().take((w * h * 4) as usize).collect()
    }

    #[test]
    fn mostly_red_art_gives_red_tint() {
        let (w, h) = (64, 64);
        let mut px = solid(w, h, [200, 30, 30, 255]);
        // Sprinkle a little green noise so it is "mostly", not purely, red.
        for y in 0..8u32 {
            for x in 0..8u32 {
                let i = ((y * w + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&[20, 180, 40, 255]);
            }
        }
        let art = ArtImage::from_rgba(w, h, px);
        let [r, g, b] = art.dominant_color();
        assert!(r > g && r > b, "expected red-dominant tint, got {:?}", [r, g, b]);
        assert!(r >= 0.75, "value should be lifted to >= 0.75, got {r}");
        assert_eq!(art.dominant_color(), art.dominant_color(), "cached tint must be stable");
    }

    #[test]
    fn grey_art_gives_default_tint() {
        let art = ArtImage::from_rgba(32, 32, solid(32, 32, [128, 128, 128, 255]));
        assert_eq!(art.dominant_color(), DEFAULT_TINT);
        let white = ArtImage::from_rgba(32, 32, solid(32, 32, [250, 250, 250, 255]));
        assert_eq!(white.dominant_color(), DEFAULT_TINT);
    }

    #[test]
    fn black_art_with_small_blue_patch_gives_blue_tint() {
        let (w, h) = (128, 128);
        let mut px = solid(w, h, [0, 0, 0, 255]);
        for y in 50..60u32 {
            for x in 50..60u32 {
                let i = ((y * w + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&[10, 20, 220, 255]);
            }
        }
        let art = ArtImage::from_rgba(w, h, px);
        let [r, g, b] = art.dominant_color();
        assert!(b > r && b > g, "expected blue-dominant tint, got {:?}", [r, g, b]);
        assert!(b >= 0.75, "value should be lifted to >= 0.75, got {b}");
    }

    #[test]
    fn empty_art_gives_default_tint() {
        assert_eq!(ArtImage::from_rgba(0, 0, Vec::new()).dominant_color(), DEFAULT_TINT);
    }

    #[test]
    fn null_source_accepts_commands() {
        let handle = NullSource.start("spotify").unwrap();
        handle.control(MediaCommand::TogglePlayPause);
        handle.control(MediaCommand::Next);
        handle.control(MediaCommand::Previous);
        assert!(handle.latest().is_none());
    }
}
