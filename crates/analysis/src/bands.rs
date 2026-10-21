//! What the bands say (ADR-0006 step 2). From each band's power, per hop:
//!
//! - **relative** (`FeatureFrame::bands`, as before): the band against its
//!   own recent best, so every band moves whatever the mix; but a band more
//!   than `LEAK_DB` under the loudest band is not lifted, so a lone tone or a
//!   filter's skirt no longer lights the whole spectrum.
//! - **level**: every band on one scale, 0..1 over the `RANGE_DB` under the
//!   track's loud level (a peak follower that rises in 50 ms and falls over
//!   half a minute, never under `REF_FLOOR_DB`), so bands keep their balance
//!   and a quiet passage reads quiet.
//! - **attack**: how far the band rose over the mean of its last three hops
//!   (32 ms), 0..1 (1 at `ATTACK_DB`), gated by its level so the floor's noise never
//!   attacks; it fades over ~80 ms, and a held note gives none.
//!
//! The same for six broad bands (sub, bass, low mid, mid, presence, air),
//! whose relative value is against their own average (1 = typical), as
//! bass/mid/treb always were.

use crate::follow;

pub const BROAD: usize = 6;
pub const BROAD_NAMES: [&str; BROAD] = ["Sub", "Bass", "Low mid", "Mid", "Presence", "Air"];
/// Default broad band edges, Hz (the usual mixing ranges).
pub const BROAD_EDGES_HZ: [f32; BROAD + 1] = [30.0, 60.0, 250.0, 500.0, 2000.0, 6000.0, 16000.0];

/// The span of `level`, dB under the track's loud level.
const RANGE_DB: f32 = 42.0;
/// The loud level never falls under this (band power, dB under full
/// scale), so a near-silent passage is never scaled up to full.
const REF_FLOOR_DB: f32 = -40.0;
/// How far under the loudest band a band may be and still be lifted to
/// full by its own gain (in power).
const LEAK_DB: f32 = 30.0;
/// The rise over the last 32 ms that reads as a full attack.
const ATTACK_DB: f32 = 9.0;
/// Release of the relative value and the level, s (attack 10 / 5 ms).
const RELEASE_S: f32 = 0.09;
/// dB added per octave above 1 kHz (taken off below), so typical music
/// reads level across the bands (measured on recordings; see phase8 log).
const TILT_DB_PER_OCT: f32 = 1.5;

#[derive(Clone, Copy)]
struct Track {
    norm: f32,
    value: f32,
    level: f32,
    /// Power of the last three hops, newest first.
    recent: [f32; 3],
    attack: f32,
    avg: f32,
    tilt_db: f32,
}

impl Track {
    fn new(centre_hz: f32) -> Self {
        Self { norm: 1e-3, value: 0.0, level: 0.0, recent: [-1.0; 3], attack: 0.0, avg: 1e-3, tilt_db: TILT_DB_PER_OCT * (centre_hz / 1000.0).log2() }
    }

    /// Level and attack from this hop's power, against the reference (tilted dB).
    fn level_attack(&mut self, power: f32, reference: f32, dt: f32, silent: bool) {
        let tilted = db(power) + self.tilt_db;
        let target = if silent { 0.0 } else { ((tilted - (reference - RANGE_DB)) / RANGE_DB).clamp(0.0, 1.0) };
        self.level = follow(self.level, target, dt, 0.005, RELEASE_S);
        let gate = ((target - 0.02) / 0.10).clamp(0.0, 1.0);
        let before = self.recent.iter().sum::<f32>() / 3.0;
        let rise = if silent || self.recent[2] < 0.0 { 0.0 } else { ((db(power) - db(before)) / ATTACK_DB).clamp(0.0, 1.0) * gate };
        self.recent = [power.max(0.0), self.recent[0], self.recent[1]];
        self.attack = follow(self.attack, rise, dt, 0.0, 0.08);
    }
}

fn db(power: f32) -> f32 {
    10.0 * (power.max(0.0) + 1e-12).log10()
}

pub struct BandSignals {
    bands: Vec<Track>,
    broad: [Track; BROAD],
    /// For each band, the broad band its centre falls in.
    broad_of: Vec<Option<usize>>,
    reference: Option<f32>,
    pub relative: Vec<f32>,
    pub level: Vec<f32>,
    pub attack: Vec<f32>,
    pub broad_level: [f32; BROAD],
    pub broad_relative: [f32; BROAD],
    pub broad_attack: [f32; BROAD],
}

impl BandSignals {
    pub fn new(centres_hz: &[f32], broad_edges_hz: &[f32; BROAD + 1]) -> Self {
        let n = centres_hz.len();
        let broad_of = centres_hz.iter().map(|c| broad_edges_hz.windows(2).position(|e| *c >= e[0] && *c < e[1])).collect();
        let broad = std::array::from_fn(|k| Track::new((broad_edges_hz[k] * broad_edges_hz[k + 1]).sqrt()));
        Self {
            bands: centres_hz.iter().map(|c| Track::new(*c)).collect(),
            broad,
            broad_of,
            reference: None,
            relative: vec![0.0; n],
            level: vec![0.0; n],
            attack: vec![0.0; n],
            broad_level: [0.0; BROAD],
            broad_relative: [0.0; BROAD],
            broad_attack: [0.0; BROAD],
        }
    }

    pub fn update(&mut self, power: &[f32], dt: f32, silent: bool) {
        let dbs: Vec<f32> = power.iter().zip(&self.bands).map(|(p, t)| db(*p) + t.tilt_db).collect();
        let loudest = dbs.iter().copied().fold(f32::MIN, f32::max);
        if !silent {
            self.reference = Some(match self.reference {
                None => loudest,
                Some(r) => follow(r, loudest, dt, 0.05, 30.0),
            });
        }
        let reference = self.reference.unwrap_or(REF_FLOOR_DB).max(REF_FLOOR_DB);

        // Relative, with the leak floor.
        let max_norm = self.bands.iter().map(|t| t.norm).fold(0.0f32, f32::max);
        let floor = max_norm * 10f32.powf(-LEAK_DB * 0.4 / 10.0);
        for (i, t) in self.bands.iter_mut().enumerate() {
            let v = power[i].max(0.0).powf(0.4);
            t.norm = follow(t.norm, v, dt, 0.05, 8.0).max(1e-3);
            let n = if silent { 0.0 } else { (v / t.norm.max(floor)).min(1.5) };
            t.value = follow(t.value, n, dt, 0.010, RELEASE_S);
            t.level_attack(power[i], reference, dt, silent);
            self.relative[i] = t.value;
            self.level[i] = t.level;
            self.attack[i] = t.attack;
        }

        // Broad bands: the mean power of the bands inside each, so they
        // sit on the same scale as one band.
        let mut broad_power = [0.0f32; BROAD];
        let mut count = [0u32; BROAD];
        for (i, k) in self.broad_of.iter().enumerate() {
            if let Some(k) = k {
                broad_power[*k] += power[i].max(0.0);
                count[*k] += 1;
            }
        }
        for (p, c) in broad_power.iter_mut().zip(count) {
            *p /= c.max(1) as f32;
        }
        for (k, t) in self.broad.iter_mut().enumerate() {
            let v = broad_power[k].powf(0.4);
            t.avg = follow(t.avg, v, dt, 5.0, 5.0).max(1e-4);
            let rel = if silent { 0.0 } else { (v / t.avg).min(3.0) };
            t.value = follow(t.value, rel, dt, 0.010, RELEASE_S);
            t.level_attack(broad_power[k], reference, dt, silent);
            self.broad_level[k] = t.level;
            self.broad_relative[k] = t.value;
            self.broad_attack[k] = t.attack;
        }
    }
}
