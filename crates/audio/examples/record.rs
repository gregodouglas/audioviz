//! Record N seconds of Spotify (or the default endpoint) to a 48 kHz stereo
//! float WAV, for offline analysis with `analyze_wav`.
//!
//! cargo run -p audioviz-audio --example record -- [seconds] [out.wav] [process.exe]

use std::time::{Duration, Instant};

use audioviz_audio::{CaptureMode, CaptureTarget};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let seconds: f64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(30.0);
    let out = args.next().unwrap_or_else(|| "target/recording.wav".into());
    let process = args.next().unwrap_or_else(|| "Spotify.exe".into());

    let source = audioviz_audio::default_source();
    let handle = source.start(CaptureTarget::Process(process), 480)?;

    let spec = hound::WavSpec { channels: 2, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut writer = hound::WavWriter::create(&out, spec)?;

    let start = Instant::now();
    let mut frames = 0usize;
    let mut synthetic = 0usize;
    let mut mode: Option<CaptureMode> = None;
    while start.elapsed() < Duration::from_secs_f64(seconds) {
        while let Ok(m) = handle.mode.try_recv() {
            eprintln!("capture: {m:?}");
            mode = Some(m);
        }
        match handle.chunks.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => {
                if chunk.synthetic {
                    synthetic += chunk.frames();
                }
                frames += chunk.frames();
                for s in &chunk.samples {
                    writer.write_sample(*s)?;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(e) => anyhow::bail!("capture ended: {e}"),
        }
    }
    writer.finalize()?;
    eprintln!("health: {:?}", handle.health());
    eprintln!("wrote {out}: {:.1} s, {:.1}% synthetic silence, mode {:?}", frames as f64 / 48_000.0, 100.0 * synthetic as f64 / frames.max(1) as f64, mode);
    Ok(())
}
