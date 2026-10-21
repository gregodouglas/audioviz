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

enum Cmd {
    DelayMs(f32),
    /// A new track started (its id from the player's metadata).
    NewTrack(Option<u64>),
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
}

impl Worker {
    fn run(mut self) {
        loop {
            while let Ok(cmd) = self.cmd_rx.try_recv() {
                match cmd {
                    Cmd::DelayMs(ms) => self.delay_ms = ms.max(0.0),
                    Cmd::NewTrack(id) => self.analyzer.new_track(id),
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
}
