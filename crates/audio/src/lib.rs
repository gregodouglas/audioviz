//! Audio capture for audioviz.
//!
//! The product listens to whatever the OS is playing (Spotify first), or to
//! the room through a microphone when the music plays elsewhere. Every
//! platform backend implements [`AudioSource`] and delivers interleaved f32
//! stereo chunks over a channel. The rest of the app never touches a
//! platform API.

use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};

#[cfg(windows)]
pub mod windows;

/// Interleaved stereo audio, always f32, always 2 channels.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// Interleaved L, R, L, R ...
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    /// When the capture thread received this chunk.
    pub captured_at: Instant,
    /// True when the backend produced this chunk to fill a gap (nothing was
    /// playing), so downstream knows it is synthetic silence.
    pub synthetic: bool,
}

impl AudioChunk {
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }
}

/// What to capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureTarget {
    /// Capture only the process tree whose executable has this name
    /// (e.g. `Spotify.exe`). Falls back to [`CaptureTarget::Endpoint`] when
    /// the process is not running or per-process loopback is unavailable.
    Process(String),
    /// Capture everything the default output endpoint is playing.
    Endpoint,
    /// Listen to the room through the default microphone.
    Microphone,
}

/// How the active capture is actually running, for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureMode {
    Starting,
    Process { name: String, pid: u32 },
    Endpoint { device: String },
    Microphone { device: String },
    Failed(String),
}

/// Handle to a running capture. Dropping it stops the capture thread.
pub struct CaptureHandle {
    pub chunks: Receiver<AudioChunk>,
    pub mode: Receiver<CaptureMode>,
    stop: Sender<()>,
}

impl CaptureHandle {
    pub fn stop(self) {
        let _ = self.stop.send(());
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

/// A platform audio capture backend.
pub trait AudioSource: Send + Sync {
    /// Start capturing on a background thread. The returned handle yields
    /// chunks of roughly `chunk_frames` stereo frames.
    fn start(&self, target: CaptureTarget, chunk_frames: usize) -> anyhow::Result<CaptureHandle>;
}

/// The default source for the current platform.
pub fn default_source() -> Box<dyn AudioSource> {
    #[cfg(windows)]
    {
        Box::new(windows::WasapiSource)
    }
    #[cfg(not(windows))]
    {
        Box::new(NullSource)
    }
}

/// Emits silence. Used on platforms without a backend yet (macOS pending).
pub struct NullSource;

impl AudioSource for NullSource {
    fn start(&self, _target: CaptureTarget, chunk_frames: usize) -> anyhow::Result<CaptureHandle> {
        let (tx, rx) = crossbeam_channel::bounded(8);
        let (mode_tx, mode_rx) = crossbeam_channel::bounded(4);
        let (stop_tx, stop_rx) = crossbeam_channel::bounded(1);
        let _ = mode_tx.send(CaptureMode::Failed("no audio backend on this platform".into()));
        std::thread::Builder::new()
            .name("audio-null".into())
            .spawn(move || {
                let sample_rate = 48_000;
                let period = std::time::Duration::from_secs_f64(chunk_frames as f64 / sample_rate as f64);
                loop {
                    if stop_rx.recv_timeout(period).is_ok() {
                        break;
                    }
                    let chunk = AudioChunk {
                        samples: vec![0.0; chunk_frames * 2],
                        sample_rate,
                        captured_at: Instant::now(),
                        synthetic: true,
                    };
                    if tx.send(chunk).is_err() {
                        break;
                    }
                }
            })?;
        Ok(CaptureHandle { chunks: rx, mode: mode_rx, stop: stop_tx })
    }
}
