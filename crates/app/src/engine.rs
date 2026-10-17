//! The engine: capture + analysis on a dedicated thread. The render thread
//! reads the latest `FeatureFrame` and drains raw (L, R) points for the scope.
//! This boundary is also where a feature-frame WebSocket tap would attach.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use audioviz_analysis::{Analyzer, AnalyzerConfig, FeatureFrame};
use audioviz_audio::{CaptureHandle, CaptureMode, CaptureTarget};
use crossbeam_channel::{Receiver, Sender};

const CHUNK_FRAMES: usize = 256;

enum Cmd {
    DelayMs(f32),
}

/// Wall-clock times of recent onset detections (after the delay line), for
/// tap-along latency calibration.
type OnsetLog = Arc<Mutex<VecDeque<Instant>>>;
const ONSET_LOG_LEN: usize = 64;

pub struct Engine {
    features: Arc<Mutex<FeatureFrame>>,
    mode: Arc<Mutex<CaptureMode>>,
    onsets: OnsetLog,
    points_rx: Receiver<Vec<[f32; 2]>>,
    cmd_tx: Sender<Cmd>,
    pub sample_rate: u32,
}

impl Engine {
    pub fn start(target: CaptureTarget) -> anyhow::Result<Self> {
        let source = audioviz_audio::default_source();
        let capture = source.start(target, CHUNK_FRAMES)?;
        let sample_rate = 48_000;

        let features = Arc::new(Mutex::new(FeatureFrame::default()));
        let mode = Arc::new(Mutex::new(CaptureMode::Starting));
        let onsets: OnsetLog = Arc::new(Mutex::new(VecDeque::with_capacity(ONSET_LOG_LEN)));
        let (points_tx, points_rx) = crossbeam_channel::bounded::<Vec<[f32; 2]>>(512);
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
            points_tx,
            cmd_rx,
        };
        std::thread::Builder::new().name("engine".into()).spawn(move || worker.run())?;

        Ok(Self { features, mode, onsets, points_rx, cmd_tx, sample_rate })
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

    pub fn set_delay_ms(&self, ms: f32) {
        let _ = self.cmd_tx.send(Cmd::DelayMs(ms));
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
    points_tx: Sender<Vec<[f32; 2]>>,
    cmd_rx: Receiver<Cmd>,
}

impl Worker {
    fn run(mut self) {
        loop {
            while let Ok(cmd) = self.cmd_rx.try_recv() {
                match cmd {
                    Cmd::DelayMs(ms) => self.delay_ms = ms.max(0.0),
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
        // Log low-band and overall onsets (kicks, snares) for calibration; the
        // hit flag lingers until the next hop, so debounce.
        if (frame.onset_hit[0] || frame.onset_hit[3]) && self.last_onset.is_none_or(|t| t.elapsed() > Duration::from_millis(60)) {
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
        // If the renderer is stalled (minimized), drop rather than block capture.
        let _ = self.points_tx.try_send(points);
    }
}
