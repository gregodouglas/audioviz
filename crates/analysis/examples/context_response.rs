//! Measures the context signals (ADR-0006 step 4) on a made-up song at
//! 128 BPM: A (8 bars), B (8 bars, other chords, hats on sixteenths), a
//! build (8 bars: no kick or bass, a snare roll speeding up, a noise riser),
//! a drop (8 bars, the groove harder with a heavy sub), a breakdown (4 bars,
//! the chords alone), A again (8 bars). It reports every transition and drop
//! event against the true section changes, and the build intensity, bass
//! presence and contrast per section. A second song, the groove alone
//! throughout, should give no drops and few transitions.
//!
//!     cargo run --release -p audioviz-analysis --example context_response

use audioviz_analysis::history::HopRecord;
use audioviz_analysis::{Analyzer, AnalyzerConfig};

const SR: u32 = 48_000;
const BPM: f32 = 128.0;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    A,
    B,
    Build,
    Drop,
    Breakdown,
}

fn render(sections: &[(Kind, u32)]) -> (Vec<f32>, Vec<(f32, Kind)>) {
    let beat = 60.0 / BPM;
    let mut out = Vec::new();
    let mut starts = Vec::new();
    let mut seed = 9u32;
    let mut noise = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    };
    let (mut hp, mut last) = (0.0f32, 0.0f32);
    let mut ph = [0.0f32; 5];
    let prog_a = [55.0f32, 43.65, 49.0, 41.2];
    let prog_b = [61.74f32, 46.25, 55.0, 36.71];
    let mut t0 = 0.0f32;
    for &(kind, bars) in sections {
        starts.push((t0, kind));
        let n = (bars as f32 * 4.0 * beat * SR as f32) as usize;
        let len = n as f32 / SR as f32;
        for i in 0..n {
            let t = i as f32 / SR as f32;
            let b = t / beat;
            let in_beat = b.fract() * beat;
            let beat_no = b as u32;
            let bar = (beat_no / 4) as usize;
            let prog = if kind == Kind::B { &prog_b } else { &prog_a };
            let root = prog[bar % 4];
            let n0 = noise();
            hp = 0.9 * (hp + n0 - last);
            last = n0;
            let mut x = 0.0;
            let groove = matches!(kind, Kind::A | Kind::B | Kind::Drop);
            let hard = if kind == Kind::Drop { 1.4 } else { 1.0 };
            if groove {
                if beat_no % 2 == 0 || kind == Kind::Drop {
                    x += hard * 0.55 * (std::f32::consts::TAU * 52.0 * in_beat).sin() * (in_beat / 0.003).min(1.0) * (-in_beat / 0.07).exp();
                }
                if beat_no % 2 == 1 {
                    x += 0.28 * n0 * (in_beat / 0.002).min(1.0) * (-in_beat / 0.08).exp();
                }
                let sub = if kind == Kind::B { 4.0 } else { 2.0 };
                let te = (b * sub).fract() * beat / sub;
                x += 0.10 * hp * (-te / 0.015).exp();
                ph[0] += std::f32::consts::TAU * root / SR as f32;
                x += if kind == Kind::Drop { 0.32 } else { 0.18 } * ph[0].sin();
            }
            if kind == Kind::Build {
                // A snare roll from quarters to thirty-seconds, and a riser.
                let p = t / len;
                let rate = [1.0f32, 2.0, 4.0, 8.0][((p * 4.0) as usize).min(3)];
                let tr = (b * rate).fract() * beat / rate;
                x += (0.12 + 0.2 * p) * n0 * (tr / 0.002).min(1.0) * (-tr / 0.05).exp();
                x += 0.25 * p * p * hp;
            }
            for (k, ratio) in [4.0f32, 5.0, 6.0].iter().enumerate() {
                ph[k + 1] += std::f32::consts::TAU * root * ratio / SR as f32;
                x += 0.04 * ph[k + 1].sin();
            }
            out.push(x);
        }
        t0 += len;
    }
    (out, starts)
}

fn analyse(samples: &[f32]) -> Vec<HopRecord> {
    let mut a = Analyzer::new(AnalyzerConfig::default());
    let mut recs = Vec::new();
    for c in samples.chunks(512) {
        let st: Vec<f32> = c.iter().flat_map(|s| [*s, *s]).collect();
        a.push(&st);
        a.drain_records(&mut recs);
    }
    recs
}

fn name(k: Kind) -> &'static str {
    match k {
        Kind::A => "A",
        Kind::B => "B",
        Kind::Build => "build",
        Kind::Drop => "drop",
        Kind::Breakdown => "breakdown",
    }
}

fn report(title: &str, sections: &[(Kind, u32)]) {
    let (samples, starts) = render(sections);
    if let Some(out) = std::env::var_os("SAVE").filter(|_| sections.len() > 1) {
        let spec = hound::WavSpec { channels: 2, sample_rate: SR, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut w = hound::WavWriter::create(out, spec).expect("create the WAV");
        for x in &samples {
            w.write_sample(*x).unwrap();
            w.write_sample(*x).unwrap();
        }
        w.finalize().unwrap();
    }
    let recs = analyse(&samples);
    let end = samples.len() as f32 / SR as f32;
    println!("{title}");
    for (i, (t0, k)) in starts.iter().enumerate() {
        let t1 = starts.get(i + 1).map_or(end, |s| s.0);
        let sec: Vec<&HopRecord> = recs.iter().filter(|r| (r.time_s as f32) >= *t0 && (r.time_s as f32) < t1).collect();
        let mean = |f: fn(&HopRecord) -> f32| sec.iter().map(|r| f(r)).sum::<f32>() / sec.len().max(1) as f32;
        let max = |f: fn(&HopRecord) -> f32| sec.iter().map(|r| f(r)).fold(0.0f32, f32::max);
        println!(
            "  {:>5.1} s {:<9}  build mean {:.2} max {:.2}  bass {:.2}  contrast {:.2}  bright trend {:+.2}  density trend {:+.2}",
            t0,
            name(*k),
            mean(|r| r.build),
            max(|r| r.build),
            mean(|r| r.bass_presence),
            mean(|r| r.contrast),
            mean(|r| r.brightness_trend),
            mean(|r| r.density_trend)
        );
    }
    let mut events = Vec::new();
    for w in recs.windows(2) {
        if w[1].transitions != w[0].transitions {
            events.push((w[1].time_s as f32, "transition", w[1].transition));
        }
        if w[1].drops != w[0].drops {
            events.push((w[1].time_s as f32, "DROP", w[1].drop_confidence));
        }
    }
    for (t, what, strength) in events {
        let (dist, near) = starts.iter().map(|s| (t - s.0, s.1)).min_by(|a, b| a.0.abs().partial_cmp(&b.0.abs()).unwrap()).unwrap();
        println!("    {t:>5.1} s {what:<10} strength {strength:.2}  ({:+.2} s from the start of {})", dist, name(near));
    }
}

fn file(path: &str) {
    let mut reader = hound::WavReader::open(path).expect("open the WAV");
    let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
    let mut a = Analyzer::new(AnalyzerConfig::default());
    let mut recs = Vec::new();
    for c in samples.chunks(1024) {
        a.push(c);
        a.drain_records(&mut recs);
    }
    println!("{path}: every 4 s, then the events");
    for r in recs.iter().step_by((4.0 * SR as f32 / 512.0) as usize) {
        println!(
            "  {:>5.0} s  {:>5.1} BPM  build {:.2}  bass {:.2}  contrast {:.2}  novelty {:.2}  trends bright {:+.2} dense {:+.2} energy {:+.2}",
            r.time_s, r.bpm, r.build, r.bass_presence, r.contrast, r.transition, r.brightness_trend, r.density_trend, r.energy_trend
        );
    }
    for w in recs.windows(2) {
        if w[1].transitions != w[0].transitions {
            println!("    {:>6.1} s transition {:.2}", w[1].time_s, w[1].transition);
        }
        if w[1].drops != w[0].drops {
            println!("    {:>6.1} s DROP {:.2}", w[1].time_s, w[1].drop_confidence);
        }
    }
}

fn main() {
    if let Some(path) = std::env::args().nth(1) {
        file(&path);
        return;
    }
    report("song: A, B, build, drop, breakdown, A", &[(Kind::A, 8), (Kind::B, 8), (Kind::Build, 8), (Kind::Drop, 8), (Kind::Breakdown, 4), (Kind::A, 8)]);
    report("groove alone", &[(Kind::A, 32)]);
}
