//! Tempo estimation and a predictive beat clock.
//!
//! Every half second: autocorrelate the recent onset envelope over the
//! 70..170 BPM lag range (with harmonic reinforcement and a mild prior toward
//! 120 BPM), pick a tempo, and guard against octave flips and jitter with a
//! lock that needs agreement across consecutive estimates. Phase is aligned by
//! scoring a comb of the last few beats against the onset envelope, then the
//! free-running phase is *slewed* toward that target so visuals never snap.
//!
//! A lock grows stronger the longer its tempo is confirmed: leaving it takes
//! agreeing estimates for longer (up to six seconds), so a breakdown's
//! swells cannot pull a song's tempo away, while `reset` (a new track)
//! lets the next tempo in at once; `seed` starts from a tempo heard before
//! (ADR-0006 step 3). A settled lock also gives way only to an onset
//! envelope with some of the energy that built it: a pad's tremolo in a
//! breakdown is periodic, but faint. A young lock on a tempo at 3:2 or 2:1
//! to another (the first seconds of a track often land there) gives way to
//! the one nearer the usual range once three estimates agree on it.

use std::collections::VecDeque;

use crate::follow;

const BPM_MIN: f32 = 70.0;
const BPM_MAX: f32 = 185.0;
const HISTORY_S: f32 = 8.0;
const ANALYSIS_S: f32 = 6.0;
const ESTIMATE_EVERY_S: f32 = 0.5;
const LOCK_TOL: f32 = 0.04;
/// History needed before the first estimate, s.
const FIRST_ESTIMATE_S: f32 = 2.5;
/// Estimates kept for switching and for `stability`.
const KEEP_ESTIMATES: usize = 16;

pub struct BeatTracker {
    frame_rate: f32,
    hist: VecDeque<f32>,
    hist_cap: usize,
    since_est: f32,
    /// Recent estimates: tempo, and the onset envelope's mean level then.
    candidates: VecDeque<(f32, f32)>,
    /// The envelope's level while the lock was being confirmed.
    lock_level: f32,
    locked: bool,
    /// Seconds the locked tempo has been confirmed for.
    lock_age: f32,
    /// Whether each recent estimate agreed with the locked tempo (or its octave).
    agreed: VecDeque<bool>,
    phase_err: f32,
    /// Estimated tempo. Meaningful when `confidence` is above ~0.3.
    pub bpm: f32,
    /// 0..1, how periodic the onset envelope looks at `bpm`.
    pub confidence: f32,
    /// 0..1, 0 at a beat, advances continuously.
    pub phase: f32,
    /// Increments each time `phase` wraps.
    pub count: u32,
    /// 1.0 at a beat, exponential decay.
    pub pulse: f32,
    /// 0..1: the share of the last eight seconds' estimates that agreed
    /// with the tempo (a steady pulse reads 1, a breakdown or a fill less).
    pub stability: f32,
}

impl BeatTracker {
    pub fn new(frame_rate: f32) -> Self {
        let hist_cap = (HISTORY_S * frame_rate) as usize;
        Self {
            frame_rate,
            hist: VecDeque::with_capacity(hist_cap + 1),
            hist_cap,
            since_est: 0.0,
            candidates: VecDeque::with_capacity(KEEP_ESTIMATES + 1),
            locked: false,
            lock_level: 0.0,
            lock_age: 0.0,
            agreed: VecDeque::with_capacity(KEEP_ESTIMATES + 1),
            phase_err: 0.0,
            bpm: 120.0,
            confidence: 0.0,
            phase: 0.0,
            count: 0,
            pulse: 0.0,
            stability: 0.0,
        }
    }

    /// A new track: forget the tempo's evidence (the clock keeps running at
    /// the old tempo, so nothing jumps, until the new one locks).
    pub fn reset(&mut self) {
        self.hist.clear();
        self.candidates.clear();
        self.agreed.clear();
        self.locked = false;
        self.lock_level = 0.0;
        self.lock_age = 0.0;
        self.since_est = 0.0;
        self.confidence = 0.0;
        self.stability = 0.0;
    }

    /// Starts from a tempo heard before (the same track played again): held
    /// loosely, so the music overrules it at once if it is wrong.
    pub fn seed(&mut self, bpm: f32) {
        self.bpm = bpm.clamp(BPM_MIN, BPM_MAX);
        self.locked = true;
        self.lock_age = 0.0;
    }

    /// Whether the tempo is locked and has held for a while: worth remembering.
    pub fn settled(&self) -> bool {
        self.locked && self.lock_age >= 8.0 && self.confidence >= 0.4
    }

    pub fn process(&mut self, onset_total: f32, dt: f32, silent: bool) {
        self.hist.push_back(if silent { 0.0 } else { onset_total });
        while self.hist.len() > self.hist_cap {
            self.hist.pop_front();
        }

        self.since_est += dt;
        if self.since_est >= ESTIMATE_EVERY_S && self.hist.len() as f32 >= FIRST_ESTIMATE_S * self.frame_rate {
            self.since_est = 0.0;
            self.estimate();
        }

        if silent {
            self.confidence = follow(self.confidence, 0.0, dt, 0.0, 2.0);
        }

        // Advance the clock; apply a slice of any pending phase correction.
        let corr = self.phase_err * (dt * 4.0).min(1.0);
        self.phase_err -= corr;
        self.phase += dt * self.bpm / 60.0 + corr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.count = self.count.wrapping_add(1);
            self.pulse = 1.0;
        } else if self.phase < 0.0 {
            self.phase += 1.0;
        } else {
            self.pulse = follow(self.pulse, 0.0, dt, 0.0, 0.15);
        }
    }

    fn estimate(&mut self) {
        let n = (ANALYSIS_S * self.frame_rate) as usize;
        let n = n.min(self.hist.len());
        let start = self.hist.len() - n;
        let x: Vec<f32> = self.hist.iter().skip(start).copied().collect();
        let mean = x.iter().sum::<f32>() / n as f32;
        let x: Vec<f32> = x.iter().map(|v| v - mean).collect();
        let r0: f32 = x.iter().map(|v| v * v).sum::<f32>() / n as f32;
        if r0 <= 1e-9 {
            self.confidence = follow(self.confidence, 0.0, ESTIMATE_EVERY_S, 0.0, 1.0);
            return;
        }

        let lag_min = (60.0 / BPM_MAX * self.frame_rate).floor() as usize;
        let lag_max = (60.0 / BPM_MIN * self.frame_rate).ceil() as usize;
        let acf = |lag: usize| -> f32 {
            if lag >= n {
                return 0.0;
            }
            let s: f32 = (lag..n).map(|t| x[t] * x[t - lag]).sum();
            s / ((n - lag) as f32 * r0)
        };
        let mut raw = vec![0.0f32; lag_max * 3 + 2];
        for (lag, r) in raw.iter_mut().enumerate() {
            *r = acf(lag);
        }

        // Score with harmonics and a log-normal prior around 120 BPM.
        let mut best = (0usize, f32::MIN);
        let mut scores = Vec::with_capacity(lag_max - lag_min + 1);
        for lag in lag_min..=lag_max {
            let bpm = 60.0 * self.frame_rate / lag as f32;
            let prior = (-0.5 * ((bpm / 120.0).log2() / 0.7).powi(2)).exp();
            let s = (raw[lag] + 0.5 * raw[2 * lag] + 0.25 * raw[3 * lag]) * (0.6 + 0.4 * prior);
            scores.push(s);
            if s > best.1 {
                best = (lag, s);
            }
        }
        let mean_score = scores.iter().sum::<f32>() / scores.len() as f32;

        // Parabolic interpolation for a sub-frame lag.
        let (l, s) = best;
        let lag_f = if l > lag_min && l < lag_max {
            let (a, b, c) = (raw[l - 1], raw[l], raw[l + 1]);
            let denom = a - 2.0 * b + c;
            if denom.abs() > 1e-6 { l as f32 + 0.5 * (a - c) / denom } else { l as f32 }
        } else {
            l as f32
        };
        let cand_bpm = 60.0 * self.frame_rate / lag_f;
        // Confidence: periodicity at the chosen lag, relative to the field.
        let conf = ((raw[l] - 0.05) / 0.45).clamp(0.0, 1.0) * ((s - mean_score) / (s.abs() + 1e-6)).clamp(0.0, 1.0).sqrt();

        // Lock logic.
        self.candidates.push_back((cand_bpm, mean));
        while self.candidates.len() > KEEP_ESTIMATES {
            self.candidates.pop_front();
        }
        let agree = |a: f32, b: f32| ((a / b).ln()).abs() < LOCK_TOL;
        let octave = |a: f32, b: f32| agree(a, 2.0 * b) || agree(2.0 * a, b);

        // Leaving a lock takes agreeing estimates for longer the longer it
        // has held: 1.5 s fresh, up to 6 s after 45 s.
        let needed = (3 + (self.lock_age / 5.0) as usize).min(12);
        let related = |a: f32, b: f32| octave(a, b) || agree(a, 1.5 * b) || agree(1.5 * a, b);
        let prior = |bpm: f32| (-0.5 * ((bpm / 120.0).log2() / 0.7).powi(2)).exp();
        let recent_agree = |n: usize| self.candidates.len() >= n && self.candidates.iter().rev().take(n).all(|c| agree(c.0, cand_bpm));
        // Strong enough to move a settled lock: some of its energy.
        let strong = |n: usize| self.lock_age < 8.0 || self.candidates.iter().rev().take(n).all(|c| c.1 >= 0.4 * self.lock_level);
        let agreed = self.locked && (agree(cand_bpm, self.bpm) || octave(cand_bpm, self.bpm));
        self.agreed.push_back(agreed);
        while self.agreed.len() > KEEP_ESTIMATES {
            self.agreed.pop_front();
        }
        if self.locked {
            if agree(cand_bpm, self.bpm) {
                self.bpm = self.bpm * 0.7 + cand_bpm * 0.3;
                self.confidence = follow(self.confidence, conf, ESTIMATE_EVERY_S, 0.3, 1.0);
                self.lock_level = if self.lock_age == 0.0 { mean } else { follow(self.lock_level, mean, ESTIMATE_EVERY_S, 2.0, 30.0) };
                self.lock_age += ESTIMATE_EVERY_S;
            } else if self.lock_age < 12.0 && related(cand_bpm, self.bpm) && recent_agree(3) && prior(cand_bpm) > prior(self.bpm) {
                // A young lock at 3:2 or 2:1 to a likelier tempo: move to it.
                self.bpm = cand_bpm;
                self.confidence = conf * 0.7;
                self.phase_err = 0.0;
                self.lock_age = 0.0;
            } else if octave(cand_bpm, self.bpm) {
                // Keep the locked octave; just note slightly lower confidence.
                self.confidence = follow(self.confidence, conf * 0.8, ESTIMATE_EVERY_S, 0.3, 1.0);
            } else if recent_agree(if related(cand_bpm, self.bpm) && prior(cand_bpm) < prior(self.bpm) { 12 } else { needed }) && strong(needed) {
                // (To a related tempo further from the usual range only on 6 s of agreement.)
                self.bpm = cand_bpm;
                self.confidence = conf * 0.5;
                self.phase_err = 0.0;
                self.lock_age = 0.0;
            } else {
                self.confidence = follow(self.confidence, conf * 0.5, ESTIMATE_EVERY_S, 0.0, 1.0);
            }
        } else if recent_agree(3) {
            self.locked = true;
            self.bpm = cand_bpm;
            self.confidence = conf;
            self.lock_age = 0.0;
        } else {
            self.bpm = cand_bpm;
            self.confidence = conf * 0.3;
        }
        // A lock is dropped only when nothing recent agrees and it is young.
        if self.confidence < 0.1 && self.locked && self.lock_age < 10.0 && self.candidates.iter().rev().take(3).all(|c| !agree(c.0, self.bpm)) {
            self.locked = false;
        }
        self.stability = self.agreed.iter().filter(|a| **a).count() as f32 / KEEP_ESTIMATES as f32;

        // Phase alignment by comb scoring against the recent envelope.
        if self.confidence >= 0.2 {
            let period = 60.0 * self.frame_rate / self.bpm; // frames
            let steps = 48;
            let mut best_off = 0.0f32;
            let mut best_s = f32::MIN;
            let sample = |pos: f32| -> f32 {
                // pos: frames back from the newest sample (0 = newest).
                let i = n as f32 - 1.0 - pos;
                if i < 0.0 {
                    return 0.0;
                }
                let i0 = i.floor() as usize;
                let f = i - i0 as f32;
                let a = x.get(i0).copied().unwrap_or(0.0);
                let b = x.get(i0 + 1).copied().unwrap_or(a);
                a + (b - a) * f
            };
            for k in 0..steps {
                let off = period * k as f32 / steps as f32;
                let mut s = 0.0;
                for beat in 0..4 {
                    // Weight recent beats more and look slightly around the comb tooth.
                    let pos = off + beat as f32 * period;
                    let w = 1.0 / (1.0 + beat as f32 * 0.3);
                    s += w * (sample(pos) + 0.5 * sample(pos - 1.0) + 0.5 * sample(pos + 1.0));
                }
                if s > best_s {
                    best_s = s;
                    best_off = off;
                }
            }
            // best_off frames ago was a beat => current phase target.
            let target = (best_off / period).fract();
            let mut err = target - self.phase;
            if err > 0.5 {
                err -= 1.0;
            } else if err < -0.5 {
                err += 1.0;
            }
            // Slew the correction in; scale by confidence so weak locks move gently,
            // and a settled lock by how much of its energy the envelope has (in a
            // breakdown the clock runs on, not led by a pad's swells).
            let energy = if self.locked && self.lock_age >= 8.0 && self.lock_level > 0.0 { ((mean / self.lock_level - 0.3) / 0.5).clamp(0.0, 1.0) } else { 1.0 };
            self.phase_err = err * (0.4 + 0.6 * self.confidence) * energy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_track_locks_to_tempo() {
        let fr = 48_000.0 / 512.0;
        let mut bt = BeatTracker::new(fr);
        let bpm = 128.0;
        let dt = 1.0 / fr;
        let period = 60.0 / bpm;
        let mut t = 0.0f32;
        let mut next_click = 0.0f32;
        let mut phases_at_click = Vec::new();
        for _ in 0..(fr * 20.0) as usize {
            let mut onset = 0.02;
            if t >= next_click {
                onset = 1.0;
                next_click += period;
                phases_at_click.push(bt.phase);
            }
            bt.process(onset, dt, false);
            t += dt;
        }
        assert!((bt.bpm - bpm).abs() < 2.0, "bpm {}", bt.bpm);
        assert!(bt.confidence > 0.4, "confidence {}", bt.confidence);
        // Over the last few clicks, the clock should read close to a beat (phase near 0 or 1).
        let recent = &phases_at_click[phases_at_click.len() - 4..];
        for p in recent {
            let d = p.min(1.0 - p);
            assert!(d < 0.15, "phase at click {p}");
        }
    }

    #[test]
    fn noise_has_low_confidence() {
        let fr = 48_000.0 / 512.0;
        let mut bt = BeatTracker::new(fr);
        let mut seed = 12345u32;
        for _ in 0..(fr * 12.0) as usize {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let r = (seed >> 8) as f32 / (1u32 << 24) as f32;
            bt.process(r, 1.0 / fr, false);
        }
        assert!(bt.confidence < 0.35, "confidence {}", bt.confidence);
    }
}
