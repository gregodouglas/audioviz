//! Run the analyzer over a WAV file and print one line per second, so the
//! beat clock, onsets, loudness and key can be sanity-checked against real
//! music. Pair with `cargo run -p audioviz-audio --example record`.
//!
//! cargo run -p audioviz-analysis --example analyze_wav -- target/recording.wav

use audioviz_analysis::{Analyzer, AnalyzerConfig};

const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: analyze_wav <file.wav>")?;
    let mut reader = hound::WavReader::open(&path)?;
    let spec = reader.spec();
    eprintln!("{path}: {} Hz, {} ch, {:?} {} bit", spec.sample_rate, spec.channels, spec.sample_format, spec.bits_per_sample);

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 * scale).collect()
        }
    };
    // Interleave to stereo if needed.
    let stereo: Vec<f32> = match spec.channels {
        2 => samples,
        1 => samples.iter().flat_map(|s| [*s, *s]).collect(),
        n => samples.chunks(n as usize).flat_map(|c| [c[0], c[1.min(c.len() - 1)]]).collect(),
    };

    let mut a = Analyzer::new(AnalyzerConfig { sample_rate: spec.sample_rate, ..Default::default() });
    let sr = spec.sample_rate as usize;
    let mut hits = [0u32; 4];
    let mut bpms = Vec::new();
    let mut confs = Vec::new();
    println!("{:>6} {:>7} {:>6} {:>5} {:>5} {:>12} {:>4} {:>6} {:>6}", "t", "LUFS-S", "bpm", "conf", "phase", "hits l/m/h/t", "key", "trend", "width");
    let mut next_report = 1.0;
    for chunk in stereo.chunks(512 * 2) {
        a.push(chunk);
        let f = a.frame();
        for g in 0..4 {
            if f.onset_hit[g] {
                hits[g] += 1;
            }
        }
        if f.time_s >= next_report {
            next_report += 1.0;
            println!(
                "{:>6.1} {:>7.1} {:>6.1} {:>5.2} {:>5.2} {:>12} {:>4} {:>+6.2} {:>6.2}",
                f.time_s,
                f.lufs_s,
                f.bpm,
                f.beat_confidence,
                f.beat_phase,
                format!("{}/{}/{}/{}", hits[0], hits[1], hits[2], hits[3]),
                NAMES[f.key_class as usize],
                f.energy_trend,
                f.stereo_width
            );
            if f.time_s > 6.0 {
                bpms.push(f.bpm);
                confs.push(f.beat_confidence);
            }
            hits = [0; 4];
        }
    }
    let _ = sr;
    if !bpms.is_empty() {
        let mut sorted = bpms.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        let within = bpms.iter().filter(|b| ((*b / median).ln()).abs() < 0.03).count();
        let mean_conf = confs.iter().sum::<f32>() / confs.len() as f32;
        println!("\nsummary: median bpm {median:.1}, {}/{} seconds within 3% of it, mean confidence {mean_conf:.2}", within, bpms.len());
    }
    Ok(())
}
