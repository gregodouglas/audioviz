//! WASAPI backend: per-process loopback (Windows 10 20348+ / Windows 11) with
//! fallback to default-endpoint loopback.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use crossbeam_channel::{Receiver, Sender};
use tracing::{info, warn};
use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};

use crate::{AudioChunk, AudioSource, CaptureHandle, CaptureMode, CaptureTarget};

const SAMPLE_RATE: u32 = 48_000;
/// If no audio arrives for this long, emit synthetic silence so the
/// visualizer keeps ticking (loopback delivers nothing while the app is idle).
const SILENCE_AFTER: Duration = Duration::from_millis(60);

pub struct WasapiSource;

impl AudioSource for WasapiSource {
    fn start(&self, target: CaptureTarget, chunk_frames: usize) -> anyhow::Result<CaptureHandle> {
        let (tx, rx) = crossbeam_channel::bounded::<AudioChunk>(16);
        let (mode_tx, mode_rx) = crossbeam_channel::bounded::<CaptureMode>(4);
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(1);

        std::thread::Builder::new()
            .name("audio-wasapi".into())
            .spawn(move || {
                let _ = mode_tx.send(CaptureMode::Starting);
                if let Err(e) = capture_thread(target, chunk_frames, tx, &mode_tx, stop_rx) {
                    warn!("capture thread ended: {e:#}");
                    let _ = mode_tx.send(CaptureMode::Failed(format!("{e:#}")));
                }
            })
            .context("spawn capture thread")?;

        Ok(CaptureHandle { chunks: rx, mode: mode_rx, stop: stop_tx })
    }
}

fn find_process(name: &str) -> Option<u32> {
    use sysinfo::{ProcessRefreshKind, RefreshKind, System};
    let refreshes = RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing());
    let system = System::new_with_specifics(refreshes);
    // Per-process loopback wants the *root* of the process tree. Spotify
    // spawns helpers whose parent is the main Spotify.exe; pick the one whose
    // parent is not itself a Spotify.exe.
    let procs: Vec<_> = system.processes_by_name(OsStr::new(name)).collect();
    if procs.is_empty() {
        return None;
    }
    let is_ours = |pid: sysinfo::Pid| procs.iter().any(|p| p.pid() == pid);
    procs
        .iter()
        .find(|p| !p.parent().is_some_and(is_ours))
        .or(procs.first())
        .map(|p| p.pid().as_u32())
}

/// Builds an initialized audio client for the target, falling back to the
/// endpoint. Returns the client and the mode we ended up in.
fn open_client(target: &CaptureTarget, format: &WaveFormat) -> anyhow::Result<(AudioClient, CaptureMode)> {
    let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 0 };

    if let CaptureTarget::Process(name) = target {
        match find_process(name) {
            Some(pid) => {
                let attempt = (|| -> anyhow::Result<AudioClient> {
                    let mut client = AudioClient::new_application_loopback_client(pid, true)
                        .map_err(|e| anyhow!("{e}"))?;
                    client
                        .initialize_client(format, &Direction::Capture, &mode)
                        .map_err(|e| anyhow!("{e}"))?;
                    Ok(client)
                })();
                match attempt {
                    Ok(client) => {
                        info!("per-process loopback on {name} (pid {pid})");
                        return Ok((client, CaptureMode::Process { name: name.clone(), pid }));
                    }
                    Err(e) => warn!("per-process loopback failed ({e:#}); falling back to endpoint"),
                }
            }
            None => info!("{name} not running; falling back to endpoint loopback"),
        }
    }

    let enumerator = wasapi::DeviceEnumerator::new().map_err(|e| anyhow!("{e}"))?;
    let device = enumerator
        .get_default_device(&Direction::Render)
        .map_err(|e| anyhow!("{e}"))?;
    let device_name = device.get_friendlyname().unwrap_or_else(|_| "default output".into());
    let mut client = device.get_iaudioclient().map_err(|e| anyhow!("{e}"))?;
    // Capture direction on a render device = endpoint loopback.
    client
        .initialize_client(format, &Direction::Capture, &mode)
        .map_err(|e| anyhow!("{e}"))?;
    info!("endpoint loopback on '{device_name}'");
    Ok((client, CaptureMode::Endpoint { device: device_name }))
}

fn capture_thread(
    target: CaptureTarget,
    chunk_frames: usize,
    tx: Sender<AudioChunk>,
    mode_tx: &Sender<CaptureMode>,
    stop_rx: Receiver<()>,
) -> anyhow::Result<()> {
    wasapi::initialize_mta().ok().map_err(|e| anyhow!("COM init: {e}"))?;

    let format = WaveFormat::new(32, 32, &SampleType::Float, SAMPLE_RATE as usize, 2, None);
    let block_align = format.get_blockalign() as usize; // bytes per frame

    let (client, mode) = open_client(&target, &format)?;
    let _ = mode_tx.send(mode);

    let event = client.set_get_eventhandle().map_err(|e| anyhow!("{e}"))?;
    let capture = client.get_audiocaptureclient().map_err(|e| anyhow!("{e}"))?;
    client.start_stream().map_err(|e| anyhow!("{e}"))?;

    let chunk_bytes = chunk_frames * block_align;
    let mut queue: VecDeque<u8> = VecDeque::with_capacity(chunk_bytes * 8);
    let mut last_audio = Instant::now();

    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }

        // Flush full chunks.
        while queue.len() >= chunk_bytes {
            let bytes: Vec<u8> = queue.drain(..chunk_bytes).collect();
            let samples: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            let chunk = AudioChunk { samples, sample_rate: SAMPLE_RATE, captured_at: Instant::now(), synthetic: false };
            if tx.send(chunk).is_err() {
                return Ok(());
            }
        }

        let frames = capture.get_next_packet_size().map_err(|e| anyhow!("{e}"))?.unwrap_or(0);
        if frames > 0 {
            capture.read_from_device_to_deque(&mut queue).map_err(|e| anyhow!("{e}"))?;
            last_audio = Instant::now();
        }

        // Wait for the next buffer event; on a quiet source, synthesize silence.
        if event.wait_for_event(SILENCE_AFTER.as_millis() as u32).is_err() {
            if last_audio.elapsed() >= SILENCE_AFTER {
                let chunk = AudioChunk {
                    samples: vec![0.0; chunk_frames * 2],
                    sample_rate: SAMPLE_RATE,
                    captured_at: Instant::now(),
                    synthetic: true,
                };
                if tx.send(chunk).is_err() {
                    return Ok(());
                }
                last_audio = Instant::now();
            }
        }
    }

    client.stop_stream().map_err(|e| anyhow!("{e}"))?;
    Ok(())
}
