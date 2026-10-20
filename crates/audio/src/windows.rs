//! WASAPI backend: per-process loopback (Windows 10 20348+ / Windows 11) with
//! fallback to default-endpoint loopback, automatic reconnection when the
//! target process restarts (new PID) or appears after we fell back, when the
//! default output device changes, or when the stream fails, and the default
//! microphone for listening to the room.
//!
//! Live first (ADR-0006): every waiting packet is read at once and sent
//! whole, so nothing waits for a later wake-up or for a chunk to fill; while
//! nothing plays, silence is made up at the pace of real time, so the frame
//! clock never slows.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use crossbeam_channel::{Receiver, Sender, TrySendError};
use tracing::{info, warn};
use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};

use crate::{AudioChunk, AudioSource, CaptureHandle, CaptureMode, CaptureTarget, SharedHealth};

const SAMPLE_RATE: u32 = 48_000;
/// If no audio arrives for this long, make up silence so the visualizer
/// keeps ticking (loopback delivers nothing while the app is idle).
const SILENCE_AFTER: Duration = Duration::from_millis(60);
/// Longest sleep between looks at the device; also the silence pacing.
const WAIT: Duration = Duration::from_millis(20);
/// A gap between packets this long while audio flows counts as late.
const LATE_AFTER: Duration = Duration::from_millis(40);
/// How often to check whether the target process died or (re)appeared, or
/// the default output device changed.
const CHECK_EVERY: Duration = Duration::from_secs(2);
/// Back-off after a failed open or a failed stream.
const RETRY_AFTER: Duration = Duration::from_secs(3);
/// About a second and a half of 10 ms packets: the engine falling further
/// behind than that drops audio rather than stalling the device.
const QUEUE_CHUNKS: usize = 160;

pub struct WasapiSource;

impl AudioSource for WasapiSource {
    fn start(&self, target: CaptureTarget, chunk_frames: usize) -> anyhow::Result<CaptureHandle> {
        let (tx, rx) = crossbeam_channel::bounded::<AudioChunk>(QUEUE_CHUNKS);
        let (mode_tx, mode_rx) = crossbeam_channel::bounded::<CaptureMode>(8);
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(1);
        let health = SharedHealth::default();
        let thread_health = health.clone();

        std::thread::Builder::new()
            .name("audio-wasapi".into())
            .spawn(move || {
                let _ = mode_tx.try_send(CaptureMode::Starting);
                let mut sink = Sink { tx, health: thread_health, frame_index: 0, pending_discontinuity: false };
                if let Err(e) = capture_thread(target, chunk_frames, &mut sink, &mode_tx, stop_rx) {
                    warn!("capture thread ended: {e:#}");
                    let _ = mode_tx.try_send(CaptureMode::Failed(format!("{e:#}")));
                }
            })
            .context("spawn capture thread")?;

        Ok(CaptureHandle { chunks: rx, mode: mode_rx, health, stop: stop_tx })
    }
}

/// Where chunks go: keeps the frame clock and the health counters.
struct Sink {
    tx: Sender<AudioChunk>,
    health: SharedHealth,
    frame_index: u64,
    pending_discontinuity: bool,
}

impl Sink {
    /// Sends one chunk; false once the receiver is gone.
    fn send(&mut self, samples: Vec<f32>, synthetic: bool) -> bool {
        let frames = (samples.len() / 2) as u64;
        let chunk = AudioChunk {
            samples,
            sample_rate: SAMPLE_RATE,
            captured_at: Instant::now(),
            frame_index: self.frame_index,
            synthetic,
            discontinuity: std::mem::take(&mut self.pending_discontinuity),
        };
        self.frame_index += frames;
        match self.tx.try_send(chunk) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                // Never stall the device for a slow reader: drop, and say so.
                self.pending_discontinuity = true;
                self.health(|h| h.dropped_chunks += 1);
                true
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    fn health(&self, f: impl FnOnce(&mut crate::CaptureHealth)) {
        if let Ok(mut h) = self.health.lock() {
            f(&mut h);
        }
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

/// The default output device's id, to notice when it changes.
fn default_render_id() -> Option<String> {
    let enumerator = wasapi::DeviceEnumerator::new().ok()?;
    enumerator.get_default_device(&Direction::Render).ok()?.get_id().ok()
}

/// Builds an initialized audio client for the target, falling back to the
/// endpoint. Returns the client, the mode we ended up in, and for endpoint
/// loopback the device's id.
fn open_client(target: &CaptureTarget, format: &WaveFormat) -> anyhow::Result<(AudioClient, CaptureMode, Option<String>)> {
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
        return Ok((client, CaptureMode::Microphone { device: device_name }, None));
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
                        return Ok((client, CaptureMode::Process { name: name.clone(), pid }, None));
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
    let device_id = device.get_id().ok();
    let mut client = device.get_iaudioclient().map_err(|e| anyhow!("{e}"))?;
    // Capture direction on a render device = endpoint loopback.
    client.initialize_client(format, &Direction::Capture, &mode).map_err(|e| anyhow!("{e}"))?;
    info!("endpoint loopback on '{device_name}'");
    Ok((client, CaptureMode::Endpoint { device: device_name }, device_id))
}

enum Outcome {
    Stop,
    Reopen,
}

fn capture_thread(target: CaptureTarget, chunk_frames: usize, sink: &mut Sink, mode_tx: &Sender<CaptureMode>, stop_rx: Receiver<()>) -> anyhow::Result<()> {
    wasapi::initialize_mta().ok().map_err(|e| anyhow!("COM init: {e}"))?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, SAMPLE_RATE as usize, 2, None);

    let mut opened_before = false;
    loop {
        let (client, mode, endpoint_id) = match open_client(&target, &format) {
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
        if opened_before {
            sink.pending_discontinuity = true;
            sink.health(|h| h.reopens += 1);
        }
        opened_before = true;
        let _ = mode_tx.try_send(mode.clone());
        let outcome = run_stream(&client, &format, &target, &mode, endpoint_id.as_deref(), chunk_frames, sink, &stop_rx);
        let _ = client.stop_stream();
        match outcome {
            Ok(Outcome::Stop) => return Ok(()),
            Ok(Outcome::Reopen) => {
                let _ = mode_tx.try_send(CaptureMode::Starting);
            }
            Err(e) => {
                // A device unplugged or invalidated: keep trying, never give up.
                warn!("capture stream failed ({e:#}); reopening in {RETRY_AFTER:?}");
                let _ = mode_tx.try_send(CaptureMode::Failed(format!("{e:#}")));
                if stop_rx.recv_timeout(RETRY_AFTER).is_ok() {
                    return Ok(());
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_stream(
    client: &AudioClient,
    format: &WaveFormat,
    target: &CaptureTarget,
    mode: &CaptureMode,
    endpoint_id: Option<&str>,
    chunk_frames: usize,
    sink: &mut Sink,
    stop_rx: &Receiver<()>,
) -> anyhow::Result<Outcome> {
    let block_align = format.get_blockalign() as usize;
    let event = client.set_get_eventhandle().map_err(|e| anyhow!("{e}"))?;
    let capture = client.get_audiocaptureclient().map_err(|e| anyhow!("{e}"))?;
    client.start_stream().map_err(|e| anyhow!("{e}"))?;

    let mut queue: VecDeque<u8> = VecDeque::with_capacity(chunk_frames * block_align * 16);
    let mut last_check = Instant::now();
    // When audio, real or made up, was last accounted for.
    let mut accounted_to = Instant::now();
    let mut last_packet: Option<Instant> = None;
    let mut first_packet = true;

    loop {
        if stop_rx.try_recv().is_ok() {
            return Ok(Outcome::Stop);
        }

        if last_check.elapsed() >= CHECK_EVERY {
            last_check = Instant::now();
            // Watch the target process: died (new PID or gone) or appeared.
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
            // Endpoint loopback follows the default output device.
            if let (CaptureMode::Endpoint { device }, Some(id)) = (mode, endpoint_id) {
                if default_render_id().is_some_and(|now| now != id) {
                    info!("default output changed from '{device}'; reopening capture");
                    sink.health(|h| h.device_changes += 1);
                    return Ok(Outcome::Reopen);
                }
            }
        }

        // Read every packet that is waiting, not just one.
        let mut got = false;
        while capture.get_next_packet_size().map_err(|e| anyhow!("{e}"))?.unwrap_or(0) > 0 {
            let before = queue.len();
            let info = capture.read_from_device_to_deque(&mut queue).map_err(|e| anyhow!("{e}"))?;
            if info.flags.silent {
                // The device says this packet is silence; its bytes may not be.
                queue.range_mut(before..).for_each(|b| *b = 0);
            }
            // The first packet of a stream often carries the flag; it means nothing then.
            if info.flags.data_discontinuity && !first_packet {
                sink.pending_discontinuity = true;
                sink.health(|h| h.discontinuities += 1);
            }
            first_packet = false;
            got = true;
        }

        if got {
            let now = Instant::now();
            if last_packet.is_some_and(|t| now - t >= LATE_AFTER) {
                sink.health(|h| h.late_packets += 1);
            }
            last_packet = Some(now);
            accounted_to = now;
            // Send every whole frame now: holding a remainder back for a full
            // chunk only adds delay.
            let whole = queue.len() / block_align * block_align;
            let mut samples = Vec::with_capacity(whole / 4);
            let mut clipped = 0u64;
            let mut bytes = queue.drain(..whole);
            while let (Some(a), Some(b), Some(c), Some(d)) = (bytes.next(), bytes.next(), bytes.next(), bytes.next()) {
                let x = f32::from_le_bytes([a, b, c, d]);
                clipped += (x.abs() >= 1.0) as u64;
                samples.push(x);
            }
            drop(bytes);
            let frames = (samples.len() / 2) as u64;
            sink.health(|h| {
                h.frames += frames;
                h.clipped_samples += clipped;
            });
            if !sink.send(samples, false) {
                return Ok(Outcome::Stop);
            }
        } else if accounted_to.elapsed() >= SILENCE_AFTER {
            // Nothing is playing: make up the silence that real time has
            // covered since, in whole chunks.
            last_packet = None;
            let due = (accounted_to.elapsed().as_secs_f64() * SAMPLE_RATE as f64) as u64;
            let chunks = due / chunk_frames as u64;
            for _ in 0..chunks {
                if !sink.send(vec![0.0; chunk_frames * 2], true) {
                    return Ok(Outcome::Stop);
                }
            }
            let made = chunks * chunk_frames as u64;
            sink.health(|h| h.synthetic_frames += made);
            accounted_to += Duration::from_secs_f64(made as f64 / SAMPLE_RATE as f64);
        }

        let _ = event.wait_for_event(WAIT.as_millis() as u32);
    }
}
