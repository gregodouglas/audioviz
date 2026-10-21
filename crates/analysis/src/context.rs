//! Context over seconds (ADR-0006 step 4): where the music is heading, not
//! what it does this instant. Each hop a short description of the sound (its
//! loudness, the six broad bands, brightness, noisiness, activity, and the
//! pitch-class profile at half weight) is averaged over four timescales:
//! fast (0.4 s), medium (3 s), slow (12 s) and long (40 s). From those:
//!
//! - **trends**: brightness and density (activity), medium against slow, -1..1.
//! - **bass presence**: the sub and bass bands over the medium scale.
//! - **contrast**: how far the medium scale is from the long one, 0..1: a
//!   section unlike the last minute reads high.
//! - **transition**: novelty, the last 1.5 s against the 4 s before them,
//!   against its own typical level; a clear peak is a transition event.
//! - **build**: rising energy, brightness, density and noise, and the bass
//!   taken out, over seconds, 0..1.
//! - **drop**: after a build, the bass and the energy coming back at once; an
//!   event, with a confidence from how big the build was and the jump.
//!
//! The averages start 2 s into a track (what feeds them is still settling
//! before) and the signals fade in over the next 8 s.
//!
//! Everything is from what has been heard: a transition is known about a
//! second and a half after it happens, a drop at once.

use std::collections::VecDeque;

use crate::follow;
use crate::sections::Sections;

pub(crate) const DIMS: usize = 10 + 12;
/// The novelty ring holds one averaged description every this many hops.
const STEP_HOPS: usize = 4;
const RECENT_S: f32 = 1.5;
const PRIOR_S: f32 = 4.0;
const TRANSITION_GAP_S: f32 = 4.0;
const DROP_GAP_S: f32 = 8.0;

pub struct ContextInput<'a> {
    pub lufs_m: f32,
    pub broad_level: &'a [f32; 6],
    pub brightness: f32,
    pub noisiness: f32,
    pub activity: f32,
    pub chroma: &'a [f32; 12],
    /// Seconds analysed, whether a bar starts on this hop, and how far
    /// through the bar it is (for `sections`).
    pub time_s: f64,
    pub bar_started: bool,
    pub bar_phase: f32,
}

pub struct Context {
    fast: [f32; DIMS],
    mid: [f32; DIMS],
    slow: [f32; DIMS],
    long: [f32; DIMS],
    started: bool,
    /// Seconds of sound since the start (or the last track change).
    age: f32,
    ring: VecDeque<[f32; DIMS]>,
    ring_cap: usize,
    recent_n: usize,
    acc: [f32; DIMS],
    acc_n: usize,
    novelty_avg: f32,
    prev_novelty: [f32; 2],
    since_transition: f32,
    since_drop: f32,
    build_peak: f32,
    /// The track's sections so far, and the current one.
    pub sections: Sections,
    pub brightness_trend: f32,
    pub density_trend: f32,
    pub bass_presence: f32,
    pub contrast: f32,
    /// Novelty against its typical level, 0..1.5 (about 1 at a clear change).
    pub transition: f32,
    pub transitions: u32,
    pub transition_pulse: f32,
    pub build: f32,
    pub drops: u32,
    pub drop_confidence: f32,
    pub drop_pulse: f32,
}

fn describe(i: &ContextInput) -> [f32; DIMS] {
    let mut d = [0.0f32; DIMS];
    d[0] = ((i.lufs_m + 40.0) / 35.0).clamp(0.0, 1.0);
    d[1..7].copy_from_slice(i.broad_level);
    d[7] = i.brightness;
    d[8] = i.noisiness;
    d[9] = i.activity;
    for (o, c) in d[10..].iter_mut().zip(i.chroma) {
        *o = 0.5 * c;
    }
    d
}

pub(crate) fn distance(a: &[f32; DIMS], b: &[f32; DIMS]) -> f32 {
    (a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>() / DIMS as f32).sqrt()
}

fn mean(it: impl Iterator<Item = [f32; DIMS]>) -> [f32; DIMS] {
    let mut m = [0.0f32; DIMS];
    let mut n = 0;
    for v in it {
        for (o, x) in m.iter_mut().zip(v) {
            *o += x;
        }
        n += 1;
    }
    for o in m.iter_mut() {
        *o /= n.max(1) as f32;
    }
    m
}

impl Context {
    pub fn new(hop_s: f32) -> Self {
        let step_s = hop_s * STEP_HOPS as f32;
        Self {
            fast: [0.0; DIMS],
            mid: [0.0; DIMS],
            slow: [0.0; DIMS],
            long: [0.0; DIMS],
            started: false,
            age: 0.0,
            ring: VecDeque::new(),
            ring_cap: ((RECENT_S + PRIOR_S) / step_s) as usize,
            recent_n: (RECENT_S / step_s) as usize,
            acc: [0.0; DIMS],
            acc_n: 0,
            novelty_avg: 0.0,
            prev_novelty: [0.0; 2],
            since_transition: 0.0,
            since_drop: DROP_GAP_S,
            build_peak: 0.0,
            sections: Sections::new(),
            brightness_trend: 0.0,
            density_trend: 0.0,
            bass_presence: 0.0,
            contrast: 0.0,
            transition: 0.0,
            transitions: 0,
            transition_pulse: 0.0,
            build: 0.0,
            drops: 0,
            drop_confidence: 0.0,
            drop_pulse: 0.0,
        }
    }

    /// A new track: the context starts again (counts keep counting).
    pub fn reset(&mut self) {
        self.started = false;
        self.age = 0.0;
        self.ring.clear();
        self.acc_n = 0;
        self.build = 0.0;
        self.build_peak = 0.0;
        self.sections.reset();
    }

    pub fn process(&mut self, input: &ContextInput, dt: f32, silent: bool) {
        self.transition_pulse = follow(self.transition_pulse, 0.0, dt, 0.0, 0.6);
        self.drop_pulse = follow(self.drop_pulse, 0.0, dt, 0.0, 0.6);
        self.since_transition += dt;
        self.since_drop += dt;
        if silent {
            self.build = follow(self.build, 0.0, dt, 0.0, 1.0);
            return;
        }
        let d = describe(input);
        self.age += dt;
        if !self.started || self.age < 2.0 {
            self.started = true;
            (self.fast, self.mid, self.slow, self.long) = (d, d, d, d);
        }
        self.sections.on_hop(&d, input.time_s, input.bar_started, input.bar_phase);
        for k in 0..DIMS {
            self.fast[k] = follow(self.fast[k], d[k], dt, 0.4, 0.4);
            self.mid[k] = follow(self.mid[k], d[k], dt, 3.0, 3.0);
            self.slow[k] = follow(self.slow[k], d[k], dt, 12.0, 12.0);
            self.long[k] = follow(self.long[k], d[k], dt, 40.0, 40.0);
        }

        // Trends, presence, contrast.
        let warm = ((self.age - 2.0) / 8.0).clamp(0.0, 1.0);
        let trend = |k: usize, gain: f32| ((self.mid[k] - self.slow[k]) * gain).clamp(-1.0, 1.0) * warm;
        self.brightness_trend = trend(7, 5.0);
        self.density_trend = trend(9, 4.0);
        let energy_trend = trend(0, 4.0);
        let noise_trend = trend(8, 4.0);
        let bass_mid = 0.5 * (self.mid[1] + self.mid[2]);
        let bass_slow = 0.5 * (self.slow[1] + self.slow[2]);
        let bass_trend = ((bass_mid - bass_slow) * 4.0).clamp(-1.0, 1.0);
        self.bass_presence = bass_mid;
        self.contrast = (distance(&self.mid, &self.long) * 5.0).min(1.0) * warm;

        // Build: things rising together, the bass taken out. One cue alone
        // (busier hats, a brighter chord) stays under the floor.
        let cues = 0.3 * energy_trend.max(0.0) + 0.3 * self.brightness_trend.max(0.0) + 0.3 * self.density_trend.max(0.0) + 0.3 * noise_trend.max(0.0) + 0.3 * (-bass_trend).max(0.0);
        let target = ((cues - 0.3) / 0.6).clamp(0.0, 1.0);
        self.build = follow(self.build, target, dt, 1.0, 1.5);
        self.build_peak = follow(self.build_peak, self.build, dt, 0.0, 6.0);

        // Drop: after a build, the bass and the energy back at once.
        let bass_fast = 0.5 * (self.fast[1] + self.fast[2]);
        let jump = (bass_fast - bass_mid).max(0.0) + (self.fast[0] - self.mid[0]).max(0.0);
        if self.build_peak > 0.3 && bass_fast - bass_mid > 0.12 && self.fast[0] - self.mid[0] > 0.04 && self.since_drop > DROP_GAP_S {
            self.drops = self.drops.wrapping_add(1);
            self.since_drop = 0.0;
            self.drop_confidence = (self.build_peak * jump / 0.25).clamp(0.0, 1.0);
            self.drop_pulse = 1.0;
        }

        // Novelty: the last 1.5 s against the 4 s before.
        for (a, x) in self.acc.iter_mut().zip(d) {
            *a += x;
        }
        self.acc_n += 1;
        if self.acc_n >= STEP_HOPS {
            let step = self.acc.map(|a| a / self.acc_n as f32);
            self.acc = [0.0; DIMS];
            self.acc_n = 0;
            self.ring.push_back(step);
            while self.ring.len() > self.ring_cap {
                self.ring.pop_front();
            }
            if self.ring.len() == self.ring_cap {
                let split = self.ring_cap - self.recent_n;
                let recent = mean(self.ring.iter().skip(split).copied());
                let prior = mean(self.ring.iter().take(split).copied());
                let novelty = distance(&recent, &prior);
                let step_s = dt * STEP_HOPS as f32;
                self.novelty_avg = if self.novelty_avg == 0.0 { novelty } else { follow(self.novelty_avg, novelty, step_s, 20.0, 20.0) };
                let level = |n: f32| (n / (3.0 * self.novelty_avg + 0.01)).min(1.5);
                self.transition = level(novelty);
                // A clear peak, a while after the last.
                let [p1, p2] = self.prev_novelty;
                if p1 > p2 && p1 >= novelty && level(p1) > 0.6 && self.since_transition > TRANSITION_GAP_S {
                    self.transitions = self.transitions.wrapping_add(1);
                    self.sections.on_transition(input.time_s);
                    self.since_transition = 0.0;
                    self.transition_pulse = 1.0;
                }
                self.prev_novelty = [novelty, p1];
            }
        }
    }
}
