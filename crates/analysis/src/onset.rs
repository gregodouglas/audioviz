//! Spectral-flux onset detection, SuperFlux-style: log magnitude, max-filter
//! across neighbouring bins/bands on the previous frame, half-wave rectified
//! difference, adaptive median + MAD threshold, causal peak picking, and a
//! refractory period.
//!
//! Resolution matters: the 512-point spectrum is great for hi-hat timing but
//! gives bass two bins, so the beat envelope comes from the 2048-point
//! spectrum (aggregated into log bands) and mid/high transients from the
//! 512-point one. The low group's envelope is handed in: the rise of the
//! filterbank's lowest bands (`bands::BandSignals::low_flux`), which a
//! snare's noise barely moves (ADR-0006 step 2).

use std::collections::VecDeque;

use crate::follow;

pub const GROUPS: usize = 4; // low, mid, high, total (beat envelope)
const HIST_SECONDS: f32 = 1.0;
const REFRACTORY_S: f32 = 0.04;
const LAMBDA: f32 = 100.0; // log compression on normalized magnitudes
const ENV_BANDS: usize = 36;

pub struct OnsetDetector {
    // 512-point spectrum: mid and high transient groups.
    prev_short: Vec<f32>,
    cur_short: Vec<f32>,
    short_ranges: [(usize, usize); 2],
    // 2048-point spectrum: low group and the log-band beat envelope.
    prev_mid: Vec<f32>,
    cur_mid: Vec<f32>,
    band_edges: Vec<(usize, usize)>,
    prev_bands: Vec<f32>,
    cur_bands: Vec<f32>,

    hist: [VecDeque<f32>; GROUPS],
    hist_len: usize,
    scratch: Vec<f32>,
    since_hit: [f32; GROUPS],
    norm: [f32; GROUPS],
    prev_x: [f32; GROUPS],
    prev2_x: [f32; GROUPS],
    prev_thr: [f32; GROUPS],

    /// Flux normalized by its running peak, ~0..1.5, continuous.
    pub strength: [f32; GROUPS],
    /// The threshold a peak of `strength` must clear to be a hit, on the same scale.
    pub threshold: [f32; GROUPS],
    /// Discrete onset this hop (one hop late: peak-picked).
    pub hit: [bool; GROUPS],
    /// Onsets so far, per group. A reader that compares it with the last
    /// value it saw never misses a hit or sees one twice (ADR-0006).
    pub count: [u32; GROUPS],
    /// 1.0-ish at a hit, exponential decay. Use for visual pulses.
    pub pulse: [f32; GROUPS],
    /// Raw beat envelope (log-band flux), for the beat tracker.
    pub flux_total: f32,
}

impl OnsetDetector {
    pub fn new(short_bins: usize, short_bin_hz: f32, mid_bins: usize, mid_bin_hz: f32, frame_rate: f32) -> Self {
        let sbin = |f: f32| ((f / short_bin_hz).round() as usize).clamp(1, short_bins - 1);
        let short_ranges = [(sbin(200.0), sbin(2000.0)), (sbin(2000.0), sbin(16000.0).min(short_bins))];

        let (lo, hi) = (30.0f32.ln(), 16_000.0f32.ln());
        let mut band_edges = Vec::with_capacity(ENV_BANDS);
        for b in 0..ENV_BANDS {
            let f0 = (lo + (hi - lo) * b as f32 / ENV_BANDS as f32).exp();
            let f1 = (lo + (hi - lo) * (b + 1) as f32 / ENV_BANDS as f32).exp();
            let b0 = ((f0 / mid_bin_hz).floor() as usize).clamp(1, mid_bins - 1);
            let b1 = ((f1 / mid_bin_hz).ceil() as usize).clamp(b0 + 1, mid_bins);
            band_edges.push((b0, b1));
        }

        let hist_len = (HIST_SECONDS * frame_rate).max(8.0) as usize;
        Self {
            prev_short: vec![0.0; short_bins],
            cur_short: vec![0.0; short_bins],
            short_ranges,
            prev_mid: vec![0.0; mid_bins],
            cur_mid: vec![0.0; mid_bins],
            band_edges,
            prev_bands: vec![0.0; ENV_BANDS],
            cur_bands: vec![0.0; ENV_BANDS],
            hist: std::array::from_fn(|_| VecDeque::with_capacity(hist_len + 1)),
            hist_len,
            scratch: Vec::with_capacity(hist_len),
            since_hit: [1.0; GROUPS],
            norm: [1e-3; GROUPS],
            prev_x: [0.0; GROUPS],
            prev2_x: [0.0; GROUPS],
            prev_thr: [f32::MAX; GROUPS],
            strength: [0.0; GROUPS],
            threshold: [0.0; GROUPS],
            hit: [false; GROUPS],
            count: [0; GROUPS],
            pulse: [0.0; GROUPS],
            flux_total: 0.0,
        }
    }

    pub fn process(&mut self, short_mag: &[f32], mid_mag: &[f32], low_flux: f32, dt: f32, silent: bool) {
        for (c, m) in self.cur_short.iter_mut().zip(short_mag) {
            *c = (1.0 + LAMBDA * m).ln();
        }
        for (c, m) in self.cur_mid.iter_mut().zip(mid_mag) {
            *c = (1.0 + LAMBDA * m).ln();
        }
        for (c, (b0, b1)) in self.cur_bands.iter_mut().zip(&self.band_edges) {
            let s: f32 = mid_mag[*b0..*b1].iter().sum();
            *c = (1.0 + LAMBDA * s).ln();
        }

        let mut flux = [0.0f32; GROUPS];
        flux[0] = low_flux;
        flux[1] = hwr_flux(&self.cur_short, &self.prev_short, self.short_ranges[0]);
        flux[2] = hwr_flux(&self.cur_short, &self.prev_short, self.short_ranges[1]);
        flux[3] = hwr_flux(&self.cur_bands, &self.prev_bands, (0, ENV_BANDS));
        self.flux_total = if silent { 0.0 } else { flux[3] };
        std::mem::swap(&mut self.prev_short, &mut self.cur_short);
        std::mem::swap(&mut self.prev_mid, &mut self.cur_mid);
        std::mem::swap(&mut self.prev_bands, &mut self.cur_bands);

        for g in 0..GROUPS {
            let x = if silent { 0.0 } else { flux[g] };
            let h = &mut self.hist[g];
            h.push_back(x);
            while h.len() > self.hist_len {
                h.pop_front();
            }
            self.scratch.clear();
            self.scratch.extend(h.iter().copied());
            self.scratch.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = self.scratch[self.scratch.len() / 2];
            for v in self.scratch.iter_mut() {
                *v = (*v - median).abs();
            }
            self.scratch.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mad = self.scratch[self.scratch.len() / 2];

            self.norm[g] = follow(self.norm[g], x, dt, 0.0, 4.0).max(1e-3);
            self.strength[g] = (x / self.norm[g]).min(1.5);
            // MAD ~ 0.67 sigma for Gaussian noise, so 4 MAD ~ 2.7 sigma, plus a
            // floor relative to the recent peak so quiet jitter never counts.
            let thr = median + 4.0 * mad + 0.05 * self.norm[g] + 0.005;
            self.threshold[g] = (thr / self.norm[g]).min(1.5);

            // Peak-pick the previous frame: a local maximum above its threshold.
            self.since_hit[g] += dt;
            let (p1, p2) = (self.prev_x[g], self.prev2_x[g]);
            let is_peak = p1 > p2 && p1 >= x && p1 > self.prev_thr[g];
            let hit = !silent && is_peak && self.since_hit[g] >= REFRACTORY_S && h.len() >= 8;
            self.hit[g] = hit;
            if hit {
                self.since_hit[g] = 0.0;
                self.count[g] = self.count[g].wrapping_add(1);
                self.pulse[g] = (p1 / self.norm[g]).clamp(0.5, 1.5);
            } else {
                self.pulse[g] = follow(self.pulse[g], 0.0, dt, 0.0, 0.12);
            }
            self.prev2_x[g] = p1;
            self.prev_x[g] = x;
            self.prev_thr[g] = thr;
        }
    }
}

/// Mean half-wave-rectified difference over `range`, against a 3-wide max
/// filter of the previous frame (suppresses vibrato/tremolo false positives).
fn hwr_flux(cur: &[f32], prev: &[f32], range: (usize, usize)) -> f32 {
    let (lo, hi) = range;
    let n = prev.len();
    let mut sum = 0.0;
    for k in lo..hi {
        let p = prev[k].max(prev[k.saturating_sub(1)]).max(prev[(k + 1).min(n - 1)]);
        sum += (cur[k] - p).max(0.0);
    }
    sum / (hi - lo).max(1) as f32
}
