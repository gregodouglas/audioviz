//! Audio capture for audioviz.
//!
//! The product listens to whatever the OS is playing (Spotify first), or to
//! the room through a microphone when the music plays elsewhere. Every
//! platform backend implements [`AudioSource`] and delivers interleaved f32
//! stereo chunks over a channel. The rest of the app never touches a
//! platform API.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};

pub mod wav;
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
    /// The stream's frame clock at this chunk's first frame: frames sent
    /// since the capture started, synthetic silence included, so it keeps
    /// pace with real time across gaps and reopens.
    pub frame_index: u64,
    /// True when the backend produced this chunk to fill a gap (nothing was
    /// playing), so downstream knows it is synthetic silence.
    pub synthetic: bool,
    /// True when audio was lost just before this chunk (a glitch, a reopen).
    pub discontinuity: bool,
}

/// Running counts of how the capture is going, for the debug line and the
/// panel. Counts only grow; compare two snapshots for a rate.
#[derive(Debug, Clone, Default)]
pub struct CaptureHealth {
    /// Real frames received from the device.
    pub frames: u64,
    /// Frames of silence made up while nothing played.
    pub synthetic_frames: u64,
    /// Glitches the device reported (audio lost inside the stream).
    pub discontinuities: u32,
    /// Samples at or over full scale.
    pub clipped_samples: u64,
    /// Times the stream was opened again (target restarted, device changed, error).
    pub reopens: u32,
    /// Times the default output device changed under endpoint loopback.
    pub device_changes: u32,
    /// Chunks dropped because the engine fell behind.
    pub dropped_chunks: u32,
    /// Packets that came 40 ms or more after the last while audio was flowing.
    pub late_packets: u32,
}

pub type SharedHealth = Arc<Mutex<CaptureHealth>>;

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
    health: SharedHealth,
    stop: Sender<()>,
}

impl CaptureHandle {
    pub fn health(&self) -> CaptureHealth {
        self.health.lock().map(|h| h.clone()).unwrap_or_default()
    }

    /// The shared counters, for a reader on another thread.
    pub fn shared_health(&self) -> SharedHealth {
        self.health.clone()
    }

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
        let health = SharedHealth::default();
        let _ = mode_tx.send(CaptureMode::Failed("no audio backend on this platform".into()));
        std::thread::Builder::new()
            .name("audio-null".into())
            .spawn(move || {
                let sample_rate = 48_000;
                let period = std::time::Duration::from_secs_f64(chunk_frames as f64 / sample_rate as f64);
                let mut frame_index = 0u64;
                loop {
                    if stop_rx.recv_timeout(period).is_ok() {
                        break;
                    }
                    let chunk = AudioChunk {
                        samples: vec![0.0; chunk_frames * 2],
                        sample_rate,
                        captured_at: Instant::now(),
                        frame_index,
                        synthetic: true,
                        discontinuity: false,
                    };
                    frame_index += chunk_frames as u64;
                    if tx.send(chunk).is_err() {
                        break;
                    }
                }
            })?;
        Ok(CaptureHandle { chunks: rx, mode: mode_rx, health, stop: stop_tx })
    }
}
