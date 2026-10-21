//! Measures how the bands answer (ADR-0006 step 2): kick trains for delay,
//! hang and contrast in the low bands; steady tones for how many bands light
//! up that should not; optionally a WAV for the balance across bands and
//! flicker. Each for the relative bands, the levels and the attacks.
//!
//!     cargo run --release -p audioviz-analysis --example band_response [file.wav]

use audioviz_analysis::history::HopRecord;
use audioviz_analysis::{Analyzer, AnalyzerConfig};

const SR: u32 = 48_000;
const HOP_S: f32 = 512.0 / SR as f32;

type Field = fn(&HopRecord) -> &[f32; 32];
const FIELDS: [(&str, Field); 3] = [("bands", |r| &r.bands), ("levels", |r| &r.levels), ("attacks", |r| &r.attacks)];

fn run(samples: &[f32]) -> Vec<HopRecord> {
    let mut a = Analyzer::new(AnalyzerConfig::default());
    let mut recs = Vec::new();
    for chunk in samples.chunks(1024) {
        a.push(chunk);
        a.drain_records(&mut recs);
    }
    recs
}

fn stereo(mono: impl Iterator<Item = f32>) -> Vec<f32> {
    mono.flat_map(|s| [s, s]).collect()
}

/// A kick: a sine sweeping 120 -> 45 Hz, decaying over ~120 ms.
fn sweep_kick(t: f32) -> f32 {
    let phase = std::f32::consts::TAU * (45.0 * t + 75.0 * 0.02 * (1.0 - (-t / 0.02).exp()));
    phase.sin() * (-t / 0.12).exp()
}

/// A plain thump: 55 Hz, 3 ms in, gone in ~60 ms.
fn thump(t: f32) -> f32 {
    (std::f32::consts::TAU * 55.0 * t).sin() * (t / 0.003).min(1.0) * (-t / 0.06).exp()
}

fn kick_table(name: &str, kick: fn(f32) -> f32) {
    let period = 0.5f32;
    let mut seed = 7u32;
    let samples = stereo((0..SR as usize * 16).map(|i| {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let noise = ((seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * 0.002;
        0.6 * kick((i as f32 / SR as f32) % period) + noise
    }));
    let recs = run(&samples);
    if std::env::var_os("TRACE").is_some() {
        for r in recs.iter().filter(|r| r.time_s >= 5.0 && r.time_s < 5.25) {
            println!("  t {:>5.0} ms  levels {:.2} {:.2} {:.2} {:.2}  bands {:.2} {:.2} {:.2} {:.2}", (r.time_s - 5.0) * 1000.0, r.levels[0], r.levels[1], r.levels[2], r.levels[3], r.bands[0], r.bands[1], r.bands[2], r.bands[3]);
        }
    }
    println!("{name} at 120 BPM: the four bands that answer most");
    println!("  field     band   peak   delay ms   over half ms   contrast");
    for (fname, field) in FIELDS {
        let mut rows = Vec::new();
        for b in 0..32 {
            let (mut delay, mut hang, mut contrast, mut peak_sum) = (0.0, 0.0, 0.0, 0.0);
            for k in 8..16 {
                let t0 = k as f32 * period;
                let window: Vec<&HopRecord> = recs.iter().filter(|r| r.time_s as f32 >= t0 && (r.time_s as f32) < t0 + period).collect();
                let (ip, peak) = window.iter().enumerate().fold((0, 0.0f32), |m, (i, r)| if field(r)[b] > m.1 { (i, field(r)[b]) } else { m });
                let trough = window.iter().map(|r| field(r)[b]).fold(f32::MAX, f32::min);
                // A record's time is the end of its hop: its sound is up to a hop older.
                delay += window[ip].time_s as f32 - t0 - HOP_S * 0.5;
                let half = trough + 0.5 * (peak - trough);
                hang += window.iter().filter(|r| field(r)[b] >= half).count() as f32 * HOP_S;
                contrast += if peak > 0.0 { (peak - trough) / peak } else { 0.0 };
                peak_sum += peak;
            }
            rows.push((b, peak_sum / 8.0, delay / 8.0 * 1000.0, hang / 8.0 * 1000.0, contrast / 8.0));
        }
        rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        for (b, peak, delay, hang, contrast) in rows.iter().take(4) {
            println!("  {fname:<8}  {b:>4}  {peak:>5.2}   {delay:>8.0}   {hang:>12.0}   {contrast:>8.2}");
        }
    }
}

/// A groove for the onset groups: kicks on beats 1 and 3, a noise snare on
/// 2 and 4, closed hats on the eighths, and a bass note that changes on
/// each beat's second sixteenth (so a low onset there is a bass note, not a kick).
fn groove_onsets() {
    let bpm = 120.0f32;
    let beat = 60.0 / bpm;
    let mut seed = 11u32;
    let mut noise = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    };
    let mut hp = 0.0f32;
    let mut last = 0.0f32;
    let notes = [41.2f32, 55.0, 49.0, 61.7];
    let mut bass_phase = 0.0f32;
    let total = SR as usize * 20;
    let mut mono = Vec::with_capacity(total);
    for i in 0..total {
        let t = i as f32 / SR as f32;
        let b = t / beat;
        let tb = (b.fract()) * beat;
        let n = noise();
        // A crude high-pass for the hats.
        hp = 0.9 * (hp + n - last);
        last = n;
        let kick = if (b as u32) % 2 == 0 { thump(tb) * 0.6 } else { 0.0 };
        let snare = if (b as u32) % 2 == 1 { n * (-tb / 0.08).exp() * 0.35 * (tb / 0.002).min(1.0) } else { 0.0 };
        let te = (b * 2.0).fract() * beat * 0.5;
        let hat = hp * (-te / 0.02).exp() * 0.15;
        let note = notes[((b - 0.25).max(0.0) as usize) % 4];
        bass_phase += std::f32::consts::TAU * note / SR as f32;
        let bass = bass_phase.sin() * 0.25;
        mono.push(kick + snare + hat + bass);
    }
    let mut a = Analyzer::new(AnalyzerConfig::default());
    let mut hits = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    let mut recs = Vec::new();
    for chunk in stereo(mono.into_iter()).chunks(1024) {
        a.push(chunk);
        a.drain_records(&mut recs);
    }
    for r in recs.iter().filter(|r| r.time_s > 6.0) {
        for g in 0..4 {
            if r.onset_hits & (1 << g) != 0 {
                hits[g].push(r.time_s as f32);
            }
        }
    }
    // Where each hit fell: on a kick (beats 1, 3), a snare (2, 4), the bass note (a beat's second sixteenth), the off-eighth, else.
    println!("groove (kick on 1 and 3, snare on 2 and 4, hats on eighths, bass note on each beat's second sixteenth), 28 beats:");
    for (g, name) in ["low", "mid", "high", "all"].iter().enumerate() {
        let (mut kick, mut snare, mut at_note, mut at_eighth, mut other) = (0, 0, 0, 0, 0);
        for t in &hits[g] {
            let x = (t / beat).fract();
            // A hit just before a beat belongs to it.
            let which = ((t / beat) + 0.08).floor() as u32;
            let near = |p: f32| (x - p).abs() < 0.12 || (x - p - 1.0).abs() < 0.12;
            if near(0.0) { if which % 2 == 0 { kick += 1 } else { snare += 1 } } else if near(0.25) { at_note += 1 } else if near(0.5) { at_eighth += 1 } else { other += 1 }
        }
        if std::env::var_os("TRACE").is_some() && g == 0 {
            let mut xs: Vec<String> = hits[g].iter().map(|t| format!("{:.2}", (t / beat) % 2.0)).collect();
            xs.sort();
            println!("  low hit positions in the bar of two beats: {}", xs.join(" "));
        }
        println!("  {name:<5} {:>3} hits: {kick:>3} kicks (of 14), {snare:>3} snares (14), {at_note:>3} bass notes (28), {at_eighth:>3} off-eighths (28), {other:>3} elsewhere", hits[g].len());
    }
}

fn main() -> anyhow::Result<()> {
    groove_onsets();
    kick_table("sweep kicks (120 -> 45 Hz, 120 ms)", sweep_kick);
    kick_table("thumps (55 Hz, 60 ms)", thump);

    // Steady tones: how many bands read over a half and over a tenth.
    for hz in [60.0f32, 1000.0] {
        let tone = stereo((0..SR as usize * 6).map(|i| 0.3 * (std::f32::consts::TAU * hz * i as f32 / SR as f32).sin()));
        let recs = run(&tone);
        let last = recs.last().unwrap();
        for (fname, field) in FIELDS.iter().take(2) {
            let v = field(last);
            println!("steady {hz} Hz tone, {fname}: {} of 32 over 0.5, {} over 0.1", v.iter().filter(|v| **v > 0.5).count(), v.iter().filter(|v| **v > 0.1).count());
        }
    }

    // A recording: the mean per band, and flicker (mean change hop to hop).
    if let Some(path) = std::env::args().nth(1) {
        let mut reader = hound::WavReader::open(&path)?;
        let samples: Vec<f32> = reader.samples::<f32>().collect::<Result<_, _>>()?;
        let started = std::time::Instant::now();
        let recs = run(&samples);
        let took = started.elapsed().as_secs_f64();
        let audio = samples.len() as f64 / 2.0 / SR as f64;
        println!("{path}: {audio:.1} s of audio analysed in {:.0} ms ({:.2}% of one core)", took * 1000.0, took / audio * 100.0);
        let skip = (5.0 / HOP_S) as usize;
        let recs = &recs[skip.min(recs.len())..];
        let n = recs.len() as f32;
        let round = |v: Vec<f32>, k: f32| v.iter().map(|x| (x * k).round() / k).collect::<Vec<_>>();
        for (fname, field) in FIELDS {
            let mean: Vec<f32> = (0..32).map(|b| recs.iter().map(|r| field(r)[b]).sum::<f32>() / n).collect();
            let flicker: Vec<f32> = (0..32).map(|b| recs.windows(2).map(|w| (field(&w[1])[b] - field(&w[0])[b]).abs()).sum::<f32>() / n).collect();
            println!("{fname} mean    {:?}", round(mean, 100.0));
            println!("{fname} flicker {:?}", round(flicker, 1000.0));
        }
        let broad: Vec<f32> = (0..6).map(|k| recs.iter().map(|r| r.broad_level[k]).sum::<f32>() / n).collect();
        println!("broad level mean {:?}", round(broad, 100.0));
        let stat = |f: fn(&HopRecord) -> f32| {
            let mut v: Vec<f32> = recs.iter().map(f).collect();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            (v[v.len() / 10], v[v.len() / 2], v[v.len() * 9 / 10])
        };
        for (name, f) in [("noisiness", (|r: &HopRecord| r.noisiness) as fn(&HopRecord) -> f32), ("activity", |r| r.activity), ("balance", |r| r.balance), ("width", |r| r.stereo_width)] {
            let (a, b, c) = stat(f);
            println!("{name}: 10th {a:.2}, median {b:.2}, 90th {c:.2}");
        }
    }
    Ok(())
}
