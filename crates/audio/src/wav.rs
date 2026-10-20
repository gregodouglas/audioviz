//! A WAV file played as if it were live, looping (ADR-0006: the test bench).
//! It goes through the same engine and analysis as captured audio, at the
//! pace of real time, so what it shows is what the live app does with the
//! same sound. `AUDIOVIZ_WAV=<path>` picks it in the app.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Context;

use crate::{AudioChunk, AudioSource, CaptureHandle, CaptureMode, CaptureTarget, SharedHealth};

pub struct WavSource {
    pub path: PathBuf,
}

/// Reads a WAV as interleaved stereo f32 (mono is doubled) and its rate.
pub fn read_stereo(path: &std::path::Path) -> anyhow::Result<(Vec<f32>, u32)> {
    let mut reader = hound::WavReader::open(path).with_context(|| format!("open {}", path.display()))?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>()?
        }
    };
    let ch = spec.channels as usize;
    let stereo = match ch {
        1 => samples.iter().flat_map(|s| [*s, *s]).collect(),
        2 => samples,
        _ => samples.chunks_exact(ch).flat_map(|f| [f[0], f[1]]).collect(),
    };
    Ok((stereo, spec.sample_rate))
}

impl AudioSource for WavSource {
    fn start(&self, _target: CaptureTarget, chunk_frames: usize) -> anyhow::Result<CaptureHandle> {
        let (samples, sample_rate) = read_stereo(&self.path)?;
        anyhow::ensure!(samples.len() >= chunk_frames * 2, "{} is too short", self.path.display());
        let (tx, rx) = crossbeam_channel::bounded(64);
        let (mode_tx, mode_rx) = crossbeam_channel::bounded(4);
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(1);
        let health = SharedHealth::default();
        let thread_health = health.clone();
        let name = self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let _ = mode_tx.send(CaptureMode::Endpoint { device: format!("{name} (file)") });
        std::thread::Builder::new().name("audio-wav".into()).spawn(move || {
            let chunk = chunk_frames * 2;
            let period = chunk_frames as f64 / sample_rate as f64;
            let start = Instant::now();
            let (mut pos, mut frame_index) = (0usize, 0u64);
            loop {
                // Keep to real time from the start, so the pace never drifts.
                let due = start + Duration::from_secs_f64(frame_index as f64 / sample_rate as f64 + period);
                if stop_rx.recv_timeout(due.saturating_duration_since(Instant::now())).is_ok() {
                    break;
                }
                let mut out = Vec::with_capacity(chunk);
                while out.len() < chunk {
                    let take = (chunk - out.len()).min(samples.len() - pos);
                    out.extend_from_slice(&samples[pos..pos + take]);
                    pos = (pos + take) % samples.len();
                }
                let c = AudioChunk { samples: out, sample_rate, captured_at: Instant::now(), frame_index, synthetic: false, discontinuity: false };
                frame_index += chunk_frames as u64;
                if let Ok(mut h) = thread_health.lock() {
                    h.frames += chunk_frames as u64;
                }
                if tx.send(c).is_err() {
                    break;
                }
            }
        })?;
        Ok(CaptureHandle { chunks: rx, mode: mode_rx, health, stop: stop_tx })
    }
}
