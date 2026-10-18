//! WASAPI backend: per-process loopback (Windows 10 20348+ / Windows 11) with
//! fallback to default-endpoint loopback, automatic reconnection when the
//! target process restarts (new PID) or appears after we fell back, and the
//! default microphone for listening to the room.

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
/// How often to check whether the target process died or (re)appeared.
const PROCESS_CHECK: Duration = Duration::from_secs(2);
/// Back-off after a failed open.
const RETRY_AFTER: Duration = Duration::from_secs(3);

pub struct WasapiSource;

impl AudioSource for WasapiSource {
    fn start(&self, target: CaptureTarget, chunk_frames: usize) -> anyhow::Result<CaptureHandle> {
        let (tx, rx) = crossbeam_channel::bounded::<AudioChunk>(16);
        let (mode_tx, mode_rx) = crossbeam_channel::bounded::<CaptureMode>(8);
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(1);

        std::thread::Builder::new()
            .name("audio-wasapi".into())
            .spawn(move || {
                let _ = mode_tx.try_send(CaptureMode::Starting);
                if let Err(e) = capture_thread(target, chunk_frames, tx, &mode_tx, stop_rx) {
                    warn!("capture thread ended: {e:#}");
                    let _ = mode_tx.try_send(CaptureMode::Failed(format!("{e:#}")));
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
    procs.iter().find(|p| !p.parent().is_some_and(is_ours)).or(procs.first()).map(|p| p.pid().as_u32())
}

/// Builds an initialized audio client for the target, falling back to the
/// endpoint. Returns the client and the mode we ended up in.
fn open_client(target: &CaptureTarget, format: &WaveFormat) -> anyhow::Result<(AudioClient, CaptureMode)> {
    let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 0 };

    if *target == CaptureTarget::Microphone {
        // Shared mode with auto-conversion brings a mono or 44.1 kHz mic to
        // our stereo 48 kHz float format.
        let enumerator = wasapi::DeviceEnumerator::new().map_err(|e| anyhow!("{e}"))?;
        let device = enumerator.get_default_device(&Direction::Capture).map_err(|e| anyhow!("no microphone: {e}"))?;
        let device_name = device.get_friendlyname().unwrap_or_else(|_| "default microphone".into());
        let mut client = device.get_iaudioclient().map_err(|e| anyhow!("{e}"))?;
        client.initialize_client(format, &Direction::Capture, &mode).map_err(|e| anyhow!("{e}"))?;
        info!("microphone capture on '{device_name}'");
        return Ok((client, CaptureMode::Microphone { device: device_name }));
    }

    if let CaptureTarget::Process(name) = target {
        match find_process(name) {
            Some(pid) => {
                let attempt = (|| -> anyhow::Result<AudioClient> {
                    let mut client = AudioClient::new_application_loopback_client(pid, true).map_err(|e| anyhow!("{e}"))?;
                    client.initialize_client(format, &Direction::Capture, &mode).map_err(|e| anyhow!("{e}"))?;
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
    let device = enumerator.get_default_device(&Direction::Render).map_err(|e| anyhow!("{e}"))?;
    let device_name = device.get_friendlyname().unwrap_or_else(|_| "default output".into());
    let mut client = device.get_iaudioclient().map_err(|e| anyhow!("{e}"))?;
    // Capture direction on a render device = endpoint loopback.
    client.initialize_client(format, &Direction::Capture, &mode).map_err(|e| anyhow!("{e}"))?;
    info!("endpoint loopback on '{device_name}'");
    Ok((client, CaptureMode::Endpoint { device: device_name }))
}

enum Outcome {
    Stop,
    Reopen,
}

fn capture_thread(target: CaptureTarget, chunk_frames: usize, tx: Sender<AudioChunk>, mode_tx: &Sender<CaptureMode>, stop_rx: Receiver<()>) -> anyhow::Result<()> {
    wasapi::initialize_mta().ok().map_err(|e| anyhow!("COM init: {e}"))?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, SAMPLE_RATE as usize, 2, None);

    loop {
        let (client, mode) = match open_client(&target, &format) {
            Ok(x) => x,
            Err(e) => {
                warn!("could not open capture ({e:#}); retrying in {RETRY_AFTER:?}");
                let _ = mode_tx.try_send(CaptureMode::Failed(format!("{e:#}")));
                if stop_rx.recv_timeout(RETRY_AFTER).is_ok() {
                    return Ok(());
                }
                continue;
            }
        };
        let _ = mode_tx.try_send(mode.clone());
        let outcome = run_stream(&client, &format, &target, &mode, chunk_frames, &tx, &stop_rx)?;
        let _ = client.stop_stream();
        match outcome {
            Outcome::Stop => return Ok(()),
            Outcome::Reopen => {
                let _ = mode_tx.try_send(CaptureMode::Starting);
            }
        }
    }
}

fn run_stream(
    client: &AudioClient,
    format: &WaveFormat,
    target: &CaptureTarget,
    mode: &CaptureMode,
    chunk_frames: usize,
    tx: &Sender<AudioChunk>,
    stop_rx: &Receiver<()>,
) -> anyhow::Result<Outcome> {
    let block_align = format.get_blockalign() as usize;
    let event = client.set_get_eventhandle().map_err(|e| anyhow!("{e}"))?;
    let capture = client.get_audiocaptureclient().map_err(|e| anyhow!("{e}"))?;
    client.start_stream().map_err(|e| anyhow!("{e}"))?;

    let chunk_bytes = chunk_frames * block_align;
    let mut queue: VecDeque<u8> = VecDeque::with_capacity(chunk_bytes * 8);
    let mut last_audio = Instant::now();
    let mut last_check = Instant::now();

    loop {
        if stop_rx.try_recv().is_ok() {
            return Ok(Outcome::Stop);
        }

        // Watch the target process: died (new PID or gone) or appeared.
        if last_check.elapsed() >= PROCESS_CHECK {
            last_check = Instant::now();
            if let CaptureTarget::Process(name) = target {
                let now_pid = find_process(name);
                match mode {
                    CaptureMode::Process { pid, .. } if now_pid != Some(*pid) => {
                        info!("{name} pid changed/exited ({pid} -> {now_pid:?}); reopening capture");
                        return Ok(Outcome::Reopen);
                    }
                    CaptureMode::Endpoint { .. } if now_pid.is_some() => {
                        info!("{name} appeared; switching to per-process loopback");
                        return Ok(Outcome::Reopen);
                    }
                    _ => {}
                }
            }
        }

        while queue.len() >= chunk_bytes {
            let bytes: Vec<u8> = queue.drain(..chunk_bytes).collect();
            let samples: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            let chunk = AudioChunk { samples, sample_rate: SAMPLE_RATE, captured_at: Instant::now(), synthetic: false };
            if tx.send(chunk).is_err() {
                return Ok(Outcome::Stop);
            }
        }

        let frames = capture.get_next_packet_size().map_err(|e| anyhow!("{e}"))?.unwrap_or(0);
        if frames > 0 {
            capture.read_from_device_to_deque(&mut queue).map_err(|e| anyhow!("{e}"))?;
            last_audio = Instant::now();
        }

        if event.wait_for_event(SILENCE_AFTER.as_millis() as u32).is_err() && last_audio.elapsed() >= SILENCE_AFTER {
            let chunk = AudioChunk { samples: vec![0.0; chunk_frames * 2], sample_rate: SAMPLE_RATE, captured_at: Instant::now(), synthetic: true };
            if tx.send(chunk).is_err() {
                return Ok(Outcome::Stop);
            }
            last_audio = Instant::now();
        }
    }
}
