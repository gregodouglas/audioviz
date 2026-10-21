//! The engine: capture + analysis on a dedicated thread. The render thread
//! reads the latest `FeatureFrame` and drains raw (L, R) points for the scope.
//! This boundary is also where a feature-frame WebSocket tap would attach.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use audioviz_analysis::history::HopRecord;
use audioviz_analysis::{Analyzer, AnalyzerConfig, FeatureFrame};
use audioviz_audio::{CaptureHandle, CaptureHealth, CaptureMode, CaptureTarget, SharedHealth};
use crossbeam_channel::{Receiver, Sender};

const CHUNK_FRAMES: usize = 256;
/// Seconds of audio kept for a session (ADR-0006 step 1b).
pub const SESSION_S: usize = 120;

enum Cmd {
    DelayMs(f32),
    /// A new track started (its id from the player's metadata).
    NewTrack(Option<u64>),
    /// Save the last `SESSION_S` seconds as a session in `dir`, with the
    /// track changes in them as (analysis time, label).
    SaveSession { dir: std::path::PathBuf, marks: Vec<(f64, String)>, done: Sender<Result<std::path::PathBuf, String>> },
}

/// Wall-clock times of recent onset detections (after the delay line), for
/// tap-along latency calibration.
type OnsetLog = Arc<Mutex<VecDeque<Instant>>>;
const ONSET_LOG_LEN: usize = 64;

pub struct Engine {
    features: Arc<Mutex<FeatureFrame>>,
    mode: Arc<Mutex<CaptureMode>>,
    health: SharedHealth,
    onsets: OnsetLog,
    points_rx: Receiver<Vec<[f32; 2]>>,
    records_rx: Receiver<Vec<HopRecord>>,
    cmd_tx: Sender<Cmd>,
    pub sample_rate: u32,
}

impl Engine {
    pub fn start(target: CaptureTarget) -> anyhow::Result<Self> {
        // AUDIOVIZ_WAV=<path> plays a file as if live, looping (the test bench, ADR-0006).
        let source: Box<dyn audioviz_audio::AudioSource> = match std::env::var_os("AUDIOVIZ_WAV") {
            Some(path) => Box::new(audioviz_audio::wav::WavSource { path: path.into() }),
            None => audioviz_audio::default_source(),
        };
        let capture = source.start(target, CHUNK_FRAMES)?;
        let health = capture.shared_health();
        let sample_rate = 48_000;

        let features = Arc::new(Mutex::new(FeatureFrame::default()));
        let mode = Arc::new(Mutex::new(CaptureMode::Starting));
        let onsets: OnsetLog = Arc::new(Mutex::new(VecDeque::with_capacity(ONSET_LOG_LEN)));
        let (points_tx, points_rx) = crossbeam_channel::bounded::<Vec<[f32; 2]>>(512);
        let (records_tx, records_rx) = crossbeam_channel::bounded::<Vec<HopRecord>>(512);
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<Cmd>();

        let worker = Worker {
            capture,
            analyzer: Analyzer::new(AnalyzerConfig { sample_rate, ..Default::default() }),
            sample_rate,
            delay_ms: 0.0,
            delay_line: VecDeque::with_capacity(sample_rate as usize * 2),
            features: features.clone(),
            mode: mode.clone(),
            onsets: onsets.clone(),
            last_onset: None,
            onset_count: [0; 4],
            points_tx,
            records: Vec::new(),
            records_tx,
            cmd_rx,
            ring: VecDeque::with_capacity(SESSION_S * sample_rate as usize * 2),
            frames_heard: 0,
        };
        std::thread::Builder::new().name("engine".into()).spawn(move || worker.run())?;

        Ok(Self { features, mode, health, onsets, points_rx, records_rx, cmd_tx, sample_rate })
    }

    pub fn features(&self) -> FeatureFrame {
        self.features.lock().map(|f| f.clone()).unwrap_or_default()
    }

    /// Recent onset detection times, oldest first.
    pub fn recent_onsets(&self) -> Vec<Instant> {
        self.onsets.lock().map(|o| o.iter().copied().collect()).unwrap_or_default()
    }

    pub fn capture_mode(&self) -> CaptureMode {
        self.mode.lock().map(|m| m.clone()).unwrap_or(CaptureMode::Starting)
    }

    /// How the capture is going: running counts (ADR-0006).
    pub fn capture_health(&self) -> CaptureHealth {
        self.health.lock().map(|h| h.clone()).unwrap_or_default()
    }

    /// Saves the last two minutes the analysis heard as a session (ADR-0006
    /// step 1b): `<dir>/session-<UTC time>.wav` (16-bit stereo) and a `.json`
    /// beside it with the app's version and where each track starts in the
    /// WAV. Written on a thread of its own; the answer comes on the channel.
    pub fn save_session(&self, dir: std::path::PathBuf, marks: Vec<(f64, String)>) -> Receiver<Result<std::path::PathBuf, String>> {
        let (done, rx) = crossbeam_channel::bounded(1);
        let _ = self.cmd_tx.send(Cmd::SaveSession { dir, marks, done });
        rx
    }

    /// Tells the analysis a new track started, so the beat clock looks for
    /// its tempo afresh (or starts from the one it settled on last time).
    pub fn new_track(&self, id: Option<u64>) {
        let _ = self.cmd_tx.send(Cmd::NewTrack(id));
    }

    pub fn set_delay_ms(&self, ms: f32) {
        let _ = self.cmd_tx.send(Cmd::DelayMs(ms));
    }

    /// The analysis records of the hops since the last call, oldest first (ADR-0006).
    pub fn drain_records(&self) -> Vec<HopRecord> {
        let mut out = Vec::new();
        while let Ok(mut batch) = self.records_rx.try_recv() {
            out.append(&mut batch);
        }
        out
    }

    /// Append all pending scope points to `out`. Returns how many were added.
    pub fn drain_points(&self, out: &mut Vec<[f32; 2]>) -> usize {
        let before = out.len();
        while let Ok(batch) = self.points_rx.try_recv() {
            out.extend_from_slice(&batch);
        }
        out.len() - before
    }
}

struct Worker {
    capture: CaptureHandle,
    analyzer: Analyzer,
    sample_rate: u32,
    delay_ms: f32,
    delay_line: VecDeque<f32>,
    features: Arc<Mutex<FeatureFrame>>,
    mode: Arc<Mutex<CaptureMode>>,
    onsets: OnsetLog,
    last_onset: Option<Instant>,
    onset_count: [u32; 4],
    points_tx: Sender<Vec<[f32; 2]>>,
    records: Vec<HopRecord>,
    records_tx: Sender<Vec<HopRecord>>,
    cmd_rx: Receiver<Cmd>,
    /// The last `SESSION_S` seconds the analysis heard (interleaved 16-bit),
    /// and how many frames it has heard since it started (its clock).
    ring: VecDeque<i16>,
    frames_heard: u64,
}

impl Worker {
    fn run(mut self) {
        loop {
            while let Ok(cmd) = self.cmd_rx.try_recv() {
                match cmd {
                    Cmd::DelayMs(ms) => self.delay_ms = ms.max(0.0),
                    Cmd::NewTrack(id) => self.analyzer.new_track(id),
                    Cmd::SaveSession { dir, marks, done } => self.save_session(dir, marks, done),
                }
            }
            while let Ok(m) = self.capture.mode.try_recv() {
                if let Ok(mut slot) = self.mode.lock() {
                    *slot = m;
                }
            }
            match self.capture.chunks.recv_timeout(Duration::from_millis(100)) {
                Ok(chunk) => {
                    if chunk.sample_rate != self.sample_rate {
                        self.sample_rate = chunk.sample_rate;
                        self.analyzer = Analyzer::new(AnalyzerConfig { sample_rate: self.sample_rate, ..Default::default() });
                        self.ring.clear();
                        self.frames_heard = 0;
                    }
                    self.delay_line.extend(chunk.samples.iter().copied());
                    self.release();
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    tracing::warn!("capture channel closed; engine stopping");
                    if let Ok(mut slot) = self.mode.lock() {
                        *slot = CaptureMode::Failed("capture stopped".into());
                    }
                    return;
                }
            }
        }
    }

    fn release(&mut self) {
        let delay_samples = ((self.delay_ms / 1000.0) * self.sample_rate as f32) as usize * 2;
        let releasable = self.delay_line.len().saturating_sub(delay_samples) & !1usize;
        if releasable == 0 {
            return;
        }
        let released: Vec<f32> = self.delay_line.drain(..releasable).collect();
        self.analyzer.push(&released);
        self.frames_heard += (released.len() / 2) as u64;
        self.ring.extend(released.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16));
        let keep = SESSION_S * self.sample_rate as usize * 2;
        if self.ring.len() > keep {
            self.ring.drain(..self.ring.len() - keep);
        }
        let frame = self.analyzer.frame();
        // Log low-band and overall onsets (kicks, snares) for calibration;
        // one release may span several hops, so compare counts, and debounce.
        let new_onset = frame.onset_count[0] != self.onset_count[0] || frame.onset_count[3] != self.onset_count[3];
        self.onset_count = frame.onset_count;
        if new_onset && self.last_onset.is_none_or(|t| t.elapsed() > Duration::from_millis(60)) {
            let now = Instant::now();
            self.last_onset = Some(now);
            if let Ok(mut log) = self.onsets.lock() {
                if log.len() >= ONSET_LOG_LEN {
                    log.pop_front();
                }
                log.push_back(now);
            }
        }
        if let Ok(mut f) = self.features.lock() {
            f.clone_from(frame);
        }
        let points: Vec<[f32; 2]> = released.chunks_exact(2).map(|p| [p[0], p[1]]).collect();
        self.analyzer.drain_records(&mut self.records);
        if !self.records.is_empty() {
            let _ = self.records_tx.try_send(std::mem::take(&mut self.records));
        }
        // If the renderer is stalled (minimized), drop rather than block capture.
        let _ = self.points_tx.try_send(points);
    }

    /// Copies the ring and writes the session on a thread of its own, so
    /// capture never waits on the disk.
    fn save_session(&self, dir: std::path::PathBuf, marks: Vec<(f64, String)>, done: Sender<Result<std::path::PathBuf, String>>) {
        let samples: Vec<i16> = self.ring.iter().copied().collect();
        let rate = self.sample_rate;
        // Analysis time at the WAV's first frame.
        let start_s = (self.frames_heard - (samples.len() / 2) as u64) as f64 / rate as f64;
        let _ = std::thread::Builder::new().name("session".into()).spawn(move || {
            let _ = done.send(write_session(&dir, &samples, rate, start_s, &marks).map_err(|e| format!("{e:#}")));
        });
    }
}

/// Writes `session-<UTC time>.wav` and `.json` into `dir`.
fn write_session(dir: &std::path::Path, samples: &[i16], rate: u32, start_s: f64, marks: &[(f64, String)]) -> anyhow::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let wav = dir.join(format!("session-{}.wav", utc_stamp(secs)));
    let spec = hound::WavSpec { channels: 2, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(&wav, spec)?;
    for s in samples {
        w.write_sample(*s)?;
    }
    w.finalize()?;
    let seconds = (samples.len() / 2) as f64 / rate as f64;
    // Track changes inside the WAV, and the one playing as it starts.
    let mut tracks: Vec<serde_json::Value> = Vec::new();
    if let Some((_, label)) = marks.iter().rev().find(|(t, _)| *t <= start_s) {
        tracks.push(serde_json::json!({ "at_s": 0.0, "label": label, "began_before": true }));
    }
    for (t, label) in marks.iter().filter(|(t, _)| *t > start_s) {
        tracks.push(serde_json::json!({ "at_s": ((t - start_s) * 1000.0).round() / 1000.0, "label": label }));
    }
    let meta = serde_json::json!({
        "app": env!("CARGO_PKG_VERSION"),
        "saved_utc": utc_stamp(secs),
        "sample_rate": rate,
        "seconds": (seconds * 1000.0).round() / 1000.0,
        "tracks": tracks,
    });
    std::fs::write(wav.with_extension("json"), serde_json::to_string_pretty(&meta)?)?;
    Ok(wav)
}

/// Seconds since 1970 as "20261021-145233" (UTC).
fn utc_stamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem / 60 % 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_stamps_read_as_dates() {
        assert_eq!(utc_stamp(0), "19700101-000000");
        assert_eq!(utc_stamp(1_792_593_153), "20261021-143233");
        assert_eq!(utc_stamp(951_782_400), "20000229-000000");
    }
}
