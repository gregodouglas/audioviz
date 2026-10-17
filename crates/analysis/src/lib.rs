//! Real-time feature extraction. v0 for the Phase 1 spike: a single-resolution
//! STFT, log-spaced bands with per-band auto-gain, MilkDrop-style
//! bass/mid/treb (fast + attenuated), RMS, spectral centroid and stereo width.
//!
//! Everything is clocked by the *sample* clock (hops), not the render frame,
//! so results are frame-rate independent. See docs/research/03-audio-analysis.md
//! for the v1 plan (multi-resolution, onsets, beat clock, chroma).

use std::sync::Arc;

use realfft::{RealFftPlanner, RealToComplex, num_complex::Complex};

#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    pub sample_rate: u32,
    pub fft_size: usize,
    pub hop: usize,
    pub bands: usize,
    pub band_lo_hz: f32,
    pub band_hi_hz: f32,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self { sample_rate: 48_000, fft_size: 2048, hop: 512, bands: 32, band_lo_hz: 30.0, band_hi_hz: 16_000.0 }
    }
}

/// One frame of musical features. All "normalized" values are relative to a
/// running average, so quiet and loud tracks look alike.
#[derive(Debug, Clone, Default)]
pub struct FeatureFrame {
    /// Linear RMS of the most recent hop (mono), 0..1.
    pub rms: f32,
    /// Slow RMS, tau ~3 s.
    pub rms_slow: f32,
    /// Decaying peak of |sample|, 0..1. Useful for auto-gain.
    pub peak: f32,
    /// MilkDrop-style: 1.0 == running average energy of that band.
    pub bass: f32,
    pub mid: f32,
    pub treb: f32,
    /// Heavily smoothed versions of the above.
    pub bass_att: f32,
    pub mid_att: f32,
    pub treb_att: f32,
    /// Log-spaced bands, per-band auto-gained and smoothed, roughly 0..1.
    pub bands: Vec<f32>,
    /// Spectral centroid in Hz, smoothed. 0 when silent.
    pub centroid_hz: f32,
    /// 0 = mono, 1 = fully wide / uncorrelated. Smoothed.
    pub stereo_width: f32,
    /// True when the input is below the noise floor.
    pub silent: bool,
}

/// Asymmetric one-pole follower: fast attack, slow release, dt-aware.
#[inline]
pub fn follow(y: f32, x: f32, dt: f32, attack_tau: f32, release_tau: f32) -> f32 {
    let tau = if x > y { attack_tau } else { release_tau };
    if tau <= 0.0 {
        return x;
    }
    y + (x - y) * (1.0 - (-dt / tau).exp())
}

struct Band {
    bin_lo: usize,
    bin_hi: usize, // exclusive
    tilt: f32,
    norm: f32, // slow peak tracker for AGC
    value: f32,
}

pub struct Analyzer {
    cfg: AnalyzerConfig,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    fft_in: Vec<f32>,
    fft_out: Vec<Complex<f32>>,
    power: Vec<f32>,
    /// Mono history ring, length fft_size; `history_pos` is the next write slot.
    history: Vec<f32>,
    history_pos: usize,
    since_hop: usize,
    hop_dt: f32,
    // Per-hop stereo accumulators.
    acc_mid: f64,
    acc_side: f64,
    acc_sq: f64,
    acc_peak: f32,
    bands: Vec<Band>,
    // bass / mid / treb running averages.
    bmt_avg: [f32; 3],
    bmt_fast: [f32; 3],
    bmt_att: [f32; 3],
    frame: FeatureFrame,
}

const SILENCE_RMS: f32 = 0.001; // -60 dBFS

impl Analyzer {
    pub fn new(cfg: AnalyzerConfig) -> Self {
        let n = cfg.fft_size;
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(n);
        let window: Vec<f32> = (0..n)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())
            .collect();
        let bin_hz = cfg.sample_rate as f32 / n as f32;
        let nbins = n / 2 + 1;

        // Log-spaced band edges.
        let mut bands = Vec::with_capacity(cfg.bands);
        let lo = cfg.band_lo_hz.ln();
        let hi = cfg.band_hi_hz.ln();
        for b in 0..cfg.bands {
            let f0 = (lo + (hi - lo) * b as f32 / cfg.bands as f32).exp();
            let f1 = (lo + (hi - lo) * (b + 1) as f32 / cfg.bands as f32).exp();
            let mut bin_lo = (f0 / bin_hz).floor() as usize;
            let mut bin_hi = (f1 / bin_hz).ceil() as usize;
            bin_lo = bin_lo.clamp(1, nbins - 1);
            bin_hi = bin_hi.clamp(bin_lo + 1, nbins);
            let fc = (f0 * f1).sqrt();
            // +3 dB/oct tilt: power scales with f, referenced to 1 kHz.
            let tilt = fc / 1000.0;
            bands.push(Band { bin_lo, bin_hi, tilt, norm: 1e-3, value: 0.0 });
        }

        let fft_in = fft.make_input_vec();
        let fft_out = fft.make_output_vec();
        Self {
            hop_dt: cfg.hop as f32 / cfg.sample_rate as f32,
            fft,
            window,
            fft_in,
            fft_out,
            power: vec![0.0; nbins],
            history: vec![0.0; n],
            history_pos: 0,
            since_hop: 0,
            acc_mid: 0.0,
            acc_side: 0.0,
            acc_sq: 0.0,
            acc_peak: 0.0,
            bands,
            bmt_avg: [1e-4; 3],
            bmt_fast: [0.0; 3],
            bmt_att: [0.0; 3],
            frame: FeatureFrame { bands: vec![0.0; cfg.bands], ..Default::default() },
            cfg,
        }
    }

    pub fn config(&self) -> &AnalyzerConfig {
        &self.cfg
    }

    pub fn frame(&self) -> &FeatureFrame {
        &self.frame
    }

    /// Feed interleaved stereo samples. Runs analysis every hop.
    pub fn push(&mut self, interleaved: &[f32]) {
        for pair in interleaved.chunks_exact(2) {
            let (l, r) = (pair[0], pair[1]);
            let mid = 0.5 * (l + r);
            let side = 0.5 * (l - r);
            self.acc_mid += (mid * mid) as f64;
            self.acc_side += (side * side) as f64;
            self.acc_sq += (mid * mid) as f64;
            self.acc_peak = self.acc_peak.max(l.abs()).max(r.abs());
            self.history[self.history_pos] = mid;
            self.history_pos = (self.history_pos + 1) % self.history.len();
            self.since_hop += 1;
            if self.since_hop >= self.cfg.hop {
                self.since_hop = 0;
                self.analyze_hop();
            }
        }
    }

    fn analyze_hop(&mut self) {
        let dt = self.hop_dt;
        let hop = self.cfg.hop as f64;

        // Level.
        let rms = (self.acc_sq / hop).sqrt() as f32;
        let mid_e = (self.acc_mid / hop) as f32;
        let side_e = (self.acc_side / hop) as f32;
        let peak = self.acc_peak;
        self.acc_sq = 0.0;
        self.acc_mid = 0.0;
        self.acc_side = 0.0;
        self.acc_peak = 0.0;

        let silent = rms < SILENCE_RMS;
        self.frame.rms = rms;
        self.frame.rms_slow = follow(self.frame.rms_slow, rms, dt, 0.3, 3.0);
        self.frame.peak = follow(self.frame.peak, peak, dt, 0.0, 1.5);
        let width = if silent { 0.0 } else { (side_e.sqrt() / (mid_e.sqrt() + 1e-6)).min(1.0) };
        self.frame.stereo_width = follow(self.frame.stereo_width, width, dt, 0.1, 0.5);
        self.frame.silent = silent;

        // Spectrum.
        // Unroll the ring so the oldest sample lands at index 0.
        let (tail, head) = self.history.split_at(self.history_pos);
        for (i, (x, w)) in head.iter().chain(tail.iter()).zip(&self.window).enumerate() {
            self.fft_in[i] = x * w;
        }
        if self.fft.process(&mut self.fft_in, &mut self.fft_out).is_err() {
            return;
        }
        let scale = 1.0 / (self.cfg.fft_size as f32 * 0.5);
        for (p, c) in self.power.iter_mut().zip(&self.fft_out) {
            *p = (c.re * c.re + c.im * c.im) * scale * scale;
        }

        // Centroid.
        let bin_hz = self.cfg.sample_rate as f32 / self.cfg.fft_size as f32;
        let (mut num, mut den) = (0.0f32, 0.0f32);
        for (i, p) in self.power.iter().enumerate().skip(1) {
            num += i as f32 * bin_hz * p;
            den += p;
        }
        let centroid = if silent || den <= 0.0 { 0.0 } else { num / den };
        self.frame.centroid_hz = follow(self.frame.centroid_hz, centroid, dt, 0.2, 0.6);

        // Bands with tilt, compression, per-band AGC, smoothing.
        for (b, out) in self.bands.iter_mut().zip(self.frame.bands.iter_mut()) {
            let p: f32 = self.power[b.bin_lo..b.bin_hi].iter().sum::<f32>() * b.tilt;
            let v = p.max(0.0).powf(0.4);
            // Slow peak tracker as AGC reference (release tau ~8 s).
            b.norm = follow(b.norm, v, dt, 0.05, 8.0).max(1e-3);
            let n = if silent { 0.0 } else { (v / b.norm).min(1.5) };
            b.value = follow(b.value, n, dt, 0.010, 0.200);
            *out = b.value;
        }

        // bass / mid / treb (power sums over fixed ranges), normalized to running average.
        let ranges = [(30.0, 200.0), (200.0, 2000.0), (2000.0, 16000.0)];
        for (k, (f0, f1)) in ranges.iter().enumerate() {
            let lo = ((f0 / bin_hz) as usize).max(1);
            let hi = ((f1 / bin_hz) as usize).min(self.power.len());
            let e: f32 = self.power[lo..hi].iter().sum();
            // Running average (tau ~5 s) with a floor so silence doesn't explode.
            self.bmt_avg[k] = follow(self.bmt_avg[k], e, dt, 5.0, 5.0).max(1e-7);
            let rel = if silent { 0.0 } else { (e / self.bmt_avg[k]).min(4.0) };
            self.bmt_fast[k] = follow(self.bmt_fast[k], rel, dt, 0.010, 0.100);
            self.bmt_att[k] = follow(self.bmt_att[k], rel, dt, 0.3, 0.5);
        }
        self.frame.bass = self.bmt_fast[0];
        self.frame.mid = self.bmt_fast[1];
        self.frame.treb = self.bmt_fast[2];
        self.frame.bass_att = self.bmt_att[0];
        self.frame.mid_att = self.bmt_att[1];
        self.frame.treb_att = self.bmt_att[2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_lands_in_the_right_band() {
        let cfg = AnalyzerConfig::default();
        let mut a = Analyzer::new(cfg.clone());
        let f = 1000.0f32;
        let mut buf = Vec::new();
        for i in 0..cfg.sample_rate as usize {
            let s = (2.0 * std::f32::consts::PI * f * i as f32 / cfg.sample_rate as f32).sin() * 0.5;
            buf.push(s);
            buf.push(s);
        }
        a.push(&buf);
        let fr = a.frame();
        assert!(!fr.silent);
        assert!((fr.centroid_hz - 1000.0).abs() < 150.0, "centroid {}", fr.centroid_hz);
        assert!(fr.stereo_width < 0.05);
        let (imax, _) = fr.bands.iter().enumerate().fold((0, 0.0), |m, (i, v)| if *v > m.1 { (i, *v) } else { m });
        // 1 kHz is a bit past the middle of a 30 Hz..16 kHz log scale.
        assert!((14..=20).contains(&imax), "peak band {imax}");
    }

    #[test]
    fn silence_is_silent() {
        let mut a = Analyzer::new(AnalyzerConfig::default());
        a.push(&vec![0.0; 48_000]);
        assert!(a.frame().silent);
        assert_eq!(a.frame().bass, 0.0);
    }
}
