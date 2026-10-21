//! Measures each signal's delay (ADR-0006 step 1c): how long after a kick
//! starts each signal a visual reads has risen halfway to its peak for that
//! kick. A made-up groove at 120 BPM (a kick on every beat, a snare on two
//! and four, hats on the eighths) runs 40 s; the analysis is read after
//! every hop, as the engine reads it; the first 10 s (the warm-up and the
//! beat clock's lock) are left out. The delay of the analysis alone: the
//! rest of the way to the screen (capture, a frame, the display) is added
//! in `docs/visuals.md`.
//!
//!     cargo run --release -p audioviz-analysis --example delay_response

use audioviz_analysis::{Analyzer, AnalyzerConfig, FeatureFrame};

const SR: u32 = 48_000;
const BPM: f32 = 120.0;
const SECONDS: f32 = 40.0;
const SKIP_S: f32 = 10.0;

/// The groove, mono.
fn render() -> Vec<f32> {
    let beat = 60.0 / BPM;
    let n = (SECONDS * SR as f32) as usize;
    let mut seed = 7u32;
    let mut noise = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    };
    let (mut hp, mut last) = (0.0f32, 0.0f32);
    (0..n)
        .map(|i| {
            let t = i as f32 / SR as f32;
            let b = t / beat;
            let in_beat = b.fract() * beat;
            let beat_no = b as u32;
            // Kick: a sweep from 120 to 45 Hz over ~120 ms.
            let kp = std::f32::consts::TAU * (45.0 * in_beat + 75.0 * 0.02 * (1.0 - (-in_beat / 0.02).exp()));
            let kick = kp.sin() * (-in_beat / 0.12).exp();
            // Snare on two and four: noise, high-passed a little, and a body.
            let w = noise();
            hp = 0.7 * (hp + w - last);
            last = w;
            let snare = if beat_no % 2 == 1 { (0.5 * hp + 0.3 * (std::f32::consts::TAU * 190.0 * in_beat).sin()) * (-in_beat / 0.09).exp() } else { 0.0 };
            // Hats on the eighths.
            let in_eighth = (b * 2.0).fract() * beat * 0.5;
            let hat = hp * 0.25 * (-in_eighth / 0.03).exp();
            0.7 * kick + 0.5 * snare + hat
        })
        .collect()
}

type Read = fn(&FeatureFrame) -> f32;

const SIGNALS: [(&str, Read); 9] = [
    ("broad_attack(BASS)", |f| f.broad_attack[1]),
    ("broad_attack(SUB)", |f| f.broad_attack[0]),
    ("attack(1)", |f| f.attacks[1]),
    ("V.onsets.x (low pulse)", |f| f.onset_pulse[0]),
    ("V.hits.x (low hit)", |f| if f.onset_hit[0] { 1.0 } else { 0.0 }),
    ("V.beat (beat pulse)", |f| f.beat_pulse * f.beat_confidence),
    ("broad(BASS) (level)", |f| f.broad_level[1]),
    ("level(1)", |f| f.levels[1]),
    ("V.bass (relative)", |f| f.bass_att),
];

fn main() {
    let mono = render();
    let stereo: Vec<f32> = mono.iter().flat_map(|s| [*s, *s]).collect();
    let mut a = Analyzer::new(AnalyzerConfig { sample_rate: SR, ..Default::default() });
    // Read after every hop, as the engine does (it reads after each release).
    let mut frames: Vec<(f32, Vec<f32>)> = Vec::new();
    for chunk in stereo.chunks(512 * 2) {
        a.push(chunk);
        let f = a.frame();
        frames.push((f.time_s as f32, SIGNALS.iter().map(|(_, r)| r(f)).collect()));
    }
    let beat = 60.0 / BPM;
    println!("Delay from a kick's start to each signal's half rise, 120 BPM, {} kicks (the analysis alone; a hop is {:.1} ms)", ((SECONDS - SKIP_S) / beat) as u32 - 1, 512.0 / SR as f32 * 1000.0);
    println!("  {:<24} {:>9} {:>9} {:>9} {:>8}", "signal", "median ms", "min ms", "max ms", "found");
    for (s, (name, _)) in SIGNALS.iter().enumerate() {
        let mut delays: Vec<f32> = Vec::new();
        let mut k = (SKIP_S / beat).ceil();
        let kicks = ((SECONDS - SKIP_S) / beat) as u32 - 1;
        for _ in 0..kicks {
            let t0 = k * beat;
            k += 1.0;
            // The window: from a quarter beat before (a clock that predicts
            // may lead) to just under half a beat after.
            let win: Vec<&(f32, Vec<f32>)> = frames.iter().filter(|(t, _)| *t >= t0 - beat * 0.25 && *t < t0 + beat * 0.45).collect();
            let base = win.first().map_or(0.0, |w| w.1[s]);
            let peak = win.iter().map(|w| w.1[s]).fold(f32::MIN, f32::max);
            if peak - base < 0.05 {
                continue;
            }
            let half = base + 0.5 * (peak - base);
            if let Some(w) = win.iter().find(|w| w.1[s] >= half) {
                delays.push((w.0 - t0) * 1000.0);
            }
        }
        if delays.is_empty() {
            println!("  {name:<24} {:>9}", "none");
            continue;
        }
        delays.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("  {name:<24} {:>9.0} {:>9.0} {:>9.0} {:>5}/{}", delays[delays.len() / 2], delays[0], delays[delays.len() - 1], delays.len(), kicks);
    }
}
