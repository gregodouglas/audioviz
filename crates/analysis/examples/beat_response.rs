//! Measures the beat clock (ADR-0006 step 3) on made-up songs: a 4/4 groove
//! (kick on 1 and 3, snare on 2 and 4, hats on the eighths, a bass note and
//! a chord that change on each bar's first beat), then a breakdown (the
//! chord alone), the groove again, then a new track at another tempo. For
//! each: how long the tempo takes to lock, how far off it is, where the beat
//! clock's phase sits at the kicks, whether the tempo holds through the
//! breakdown, how fast the new track locks, and (when the analysis has one)
//! whether the bar's first beat is found.
//!
//!     cargo run --release -p audioviz-analysis --example beat_response [file.wav]

use audioviz_analysis::history::HopRecord;
use audioviz_analysis::{Analyzer, AnalyzerConfig};

const SR: u32 = 48_000;

/// One section of a made-up song.
#[derive(Clone, Copy)]
struct Section {
    bpm: f32,
    bars: u32,
    drums: bool,
    /// The analysis is told a new track starts here.
    new_track: bool,
}

struct Song {
    sections: Vec<Section>,
}

/// Renders a song: mono samples and the true times of every beat (with its
/// place in the bar), section by section.
fn render(song: &Song) -> (Vec<f32>, Vec<(f32, u32, usize)>, Vec<f32>) {
    let mut out = Vec::new();
    let mut beats = Vec::new();
    let mut track_starts = Vec::new();
    let mut seed = 5u32;
    let mut noise = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    };
    let (mut hp, mut last) = (0.0f32, 0.0f32);
    let roots = [55.0f32, 43.65, 49.0, 41.2];
    let mut t0 = 0.0f32;
    let mut phases = [0.0f32; 4];
    for (si, s) in song.sections.iter().enumerate() {
        if s.new_track {
            track_starts.push(t0);
        }
        let beat = 60.0 / s.bpm;
        let seconds = s.bars as f32 * 4.0 * beat;
        let n = (seconds * SR as f32) as usize;
        for k in 0..s.bars * 4 {
            beats.push((t0 + k as f32 * beat, k % 4, si));
        }
        for i in 0..n {
            let t = i as f32 / SR as f32;
            let b = t / beat;
            let in_beat = b.fract() * beat;
            let beat_no = b as u32;
            let bar = (beat_no / 4) as usize;
            let root = roots[bar % 4];
            let n0 = noise();
            hp = 0.9 * (hp + n0 - last);
            last = n0;
            let mut x = 0.0;
            if s.drums {
                if beat_no % 2 == 0 {
                    x += 0.6 * (std::f32::consts::TAU * 55.0 * in_beat).sin() * (in_beat / 0.003).min(1.0) * (-in_beat / 0.06).exp();
                } else {
                    x += 0.3 * n0 * (in_beat / 0.002).min(1.0) * (-in_beat / 0.08).exp();
                }
                let te = (b * 2.0).fract() * beat * 0.5;
                x += 0.12 * hp * (-te / 0.02).exp();
                phases[0] += std::f32::consts::TAU * root / SR as f32;
                x += 0.2 * phases[0].sin();
            }
            // The chord: root's octave, a third and a fifth above, a little swell per bar.
            let bar_t = (b / 4.0).fract();
            let swell = 0.5 + 0.5 * (bar_t * std::f32::consts::PI).sin();
            for (k, ratio) in [4.0f32, 5.04, 6.0].iter().enumerate() {
                phases[k + 1] += std::f32::consts::TAU * root * ratio / SR as f32;
                x += 0.05 * swell * phases[k + 1].sin();
            }
            out.push(x);
        }
        t0 += seconds;
    }
    (out, beats, track_starts)
}

fn analyse(samples: &[f32], track_starts: &[f32]) -> Vec<HopRecord> {
    let mut a = Analyzer::new(AnalyzerConfig::default());
    let mut recs = Vec::new();
    let mut next_track = 0;
    let chunk = 512;
    for (i, c) in samples.chunks(chunk).enumerate() {
        let t = (i * chunk) as f32 / SR as f32;
        if next_track < track_starts.len() && t >= track_starts[next_track] {
            if t > 0.0 {
                a.new_track(Some(next_track as u64));
            }
            next_track += 1;
        }
        let st: Vec<f32> = c.iter().flat_map(|s| [*s, *s]).collect();
        a.push(&st);
        a.drain_records(&mut recs);
    }
    recs
}

/// The record at time t (the first whose hop ends after it).
fn at(recs: &[HopRecord], t: f32) -> Option<&HopRecord> {
    recs.iter().find(|r| r.time_s as f32 >= t)
}

fn report(name: &str, song: &Song) {
    let (samples, beats, track_starts) = render(song);
    let recs = analyse(&samples, &track_starts);
    println!("{name}");
    let mut t0 = 0.0f32;
    for (si, s) in song.sections.iter().enumerate() {
        let seconds = s.bars as f32 * 240.0 / s.bpm;
        let t1 = t0 + seconds;
        let in_sec: Vec<&HopRecord> = recs.iter().filter(|r| (r.time_s as f32) >= t0 && (r.time_s as f32) < t1).collect();
        let ok = |r: &HopRecord| (r.bpm / s.bpm - 1.0).abs() < 0.02;
        // Lock: the first moment from which the tempo stays within 2% to the section's end.
        let lock = (0..in_sec.len()).find(|i| in_sec[*i..].iter().all(|r| ok(r))).map(|i| in_sec[i].time_s as f32 - t0);
        let octave = in_sec.iter().filter(|r| (r.bpm / s.bpm / 2.0 - 1.0).abs() < 0.03 || (r.bpm / s.bpm * 2.0 - 1.0).abs() < 0.03).count();
        let conf = in_sec.iter().map(|r| r.beat_confidence).sum::<f32>() / in_sec.len().max(1) as f32;
        // Phase at the true beats in the section's last half: distance from 0 in beats.
        let half = t0 + seconds * 0.5;
        let errs: Vec<f32> = beats.iter().filter(|b| b.2 == si && b.0 >= half).filter_map(|b| at(&recs, b.0 + 0.003)).map(|r| { let p = r.beat_phase; if p > 0.5 { p - 1.0 } else { p } }).collect();
        let mean_err = errs.iter().sum::<f32>() / errs.len().max(1) as f32;
        let spread = (errs.iter().map(|e| (e - mean_err).powi(2)).sum::<f32>() / errs.len().max(1) as f32).sqrt();
        let bpm_end = in_sec.last().map_or(0.0, |r| r.bpm);
        // The bar: past the middle of each true beat of the second half, is it the right beat of the bar?
        let bar_checks: Vec<(bool, f32)> = beats.iter().filter(|b| b.2 == si && b.0 >= half).filter_map(|b| at(&recs, b.0 + 0.6 * 60.0 / s.bpm).map(|r| (r.beat_in_bar as u32 == b.1, r.bar_confidence))).collect();
        let bar_right = bar_checks.iter().filter(|c| c.0).count() as f32 / bar_checks.len().max(1) as f32;
        let bar_conf = bar_checks.iter().map(|c| c.1).sum::<f32>() / bar_checks.len().max(1) as f32;
        println!(
            "  {:>3.0} BPM {:>4.1} s {:<9} lock {:>9}  end {:>6.1} BPM  octave-off {:>3.0}%  conf {:.2}  stable {:.2}  phase at kicks {:+.3} beat ({:.0} ms) +- {:.3}  bar right {:>3.0}% conf {:.2}",
            s.bpm,
            seconds,
            if s.drums { "groove" } else { "breakdown" },
            lock.map_or("never".to_string(), |l| format!("{l:.1} s")),
            bpm_end,
            100.0 * octave as f32 / in_sec.len().max(1) as f32,
            conf,
            in_sec.iter().map(|r| r.pulse_stability).sum::<f32>() / in_sec.len().max(1) as f32,
            mean_err,
            mean_err * 60.0 / s.bpm * 1000.0,
            spread,
            bar_right * 100.0,
            bar_conf
        );
        t0 = t1;
    }
}

fn main() -> anyhow::Result<()> {
    for bpm in [92.0f32, 128.0, 140.0, 174.0] {
        let other = if bpm > 120.0 { 100.0 } else { 124.0 };
        let song = Song {
            sections: vec![
                Section { bpm, bars: 12, drums: true, new_track: true },
                Section { bpm, bars: 4, drums: false, new_track: false },
                Section { bpm, bars: 8, drums: true, new_track: false },
                Section { bpm: other, bars: 10, drums: true, new_track: true },
            ],
        };
        report(&format!("song at {bpm} BPM, then a new track at {other}:"), &song);
    }
    Ok(())
}
