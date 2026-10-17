//! Real-time music feature extraction for audioviz.
//!
//! Sample-clocked (per hop), frame-rate independent. v1 as planned in
//! docs/research/03-audio-analysis.md: multi-resolution STFT, log bands with
//! per-band auto-gain, MilkDrop-style bass/mid/treb, K-weighted loudness,
//! spectral-flux onsets per band group, a predictive beat clock, chroma,
//! stereo width and a long-window energy trend.

pub mod beat;
pub mod chroma;
pub mod loudness;
pub mod onset;
pub mod stft;

use beat::BeatTracker;
use chroma::Chroma;
use loudness::Loudness;
use onset::OnsetDetector;
use stft::{Ring, Stft};

#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    pub sample_rate: u32,
    pub hop: usize,
    pub bands: usize,
    pub band_lo_hz: f32,
    pub band_hi_hz: f32,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self { sample_rate: 48_000, hop: 512, bands: 32, band_lo_hz: 30.0, band_hi_hz: 16_000.0 }
    }
}

const N_LONG: usize = 8192;
const N_MID: usize = 2048;
const N_SHORT: usize = 512;
const SILENCE_RMS: f32 = 0.001; // -60 dBFS

/// One frame of musical features. "Normalized" values are relative to a
/// running average or peak, so quiet and loud tracks look alike.
#[derive(Debug, Clone)]
pub struct FeatureFrame {
    /// Seconds of audio analyzed so far.
    pub time_s: f64,
    /// Linear RMS of the most recent hop (mono).
    pub rms: f32,
    pub rms_slow: f32,
    /// Decaying peak of |sample|. Useful for auto-gain.
    pub peak: f32,
    /// K-weighted loudness, LUFS (momentary 400 ms, short-term 3 s).
    pub lufs_m: f32,
    pub lufs_s: f32,
    /// MilkDrop-style: 1.0 == running average energy of that band.
    pub bass: f32,
    pub mid: f32,
    pub treb: f32,
    pub bass_att: f32,
    pub mid_att: f32,
    pub treb_att: f32,
    /// Log-spaced bands, per-band auto-gained and smoothed, roughly 0..1.
    pub bands: Vec<f32>,
    pub centroid_hz: f32,
    /// 0 = mono, 1 = wide.
    pub stereo_width: f32,
    /// Onsets per group [low, mid, high, total]: continuous strength,
    /// discrete hit this hop, and a decaying pulse.
    pub onset_strength: [f32; 4],
    pub onset_hit: [bool; 4],
    pub onset_pulse: [f32; 4],
    /// Beat clock.
    pub bpm: f32,
    pub beat_confidence: f32,
    pub beat_phase: f32,
    pub beat_count: u32,
    pub beat_pulse: f32,
    /// Pitch-class profile (C=0), fast and slow, plus the dominant class.
    pub chroma: [f32; 12],
    pub chroma_slow: [f32; 12],
    pub key_class: u8,
    /// -1..1: negative = energy falling vs the last ~16 s, positive = building.
    pub energy_trend: f32,
    pub quiet: bool,
    pub silent: bool,
}

impl Default for FeatureFrame {
    fn default() -> Self {
        Self {
            time_s: 0.0,
            rms: 0.0,
            rms_slow: 0.0,
            peak: 0.0,
            lufs_m: -70.0,
            lufs_s: -70.0,
            bass: 0.0,
            mid: 0.0,
            treb: 0.0,
            bass_att: 0.0,
            mid_att: 0.0,
            treb_att: 0.0,
            bands: Vec::new(),
            centroid_hz: 0.0,
            stereo_width: 0.0,
            onset_strength: [0.0; 4],
            onset_hit: [false; 4],
            onset_pulse: [0.0; 4],
            bpm: 120.0,
            beat_confidence: 0.0,
            beat_phase: 0.0,
            beat_count: 0,
            beat_pulse: 0.0,
            chroma: [0.0; 12],
            chroma_slow: [0.0; 12],
            key_class: 0,
            energy_trend: 0.0,
            quiet: true,
            silent: true,
        }
    }
}

/// Asymmetric one-pole follower: fast attack, slow release, dt-aware.
/// A tau of 0 snaps instantly in that direction.
#[inline]
pub fn follow(y: f32, x: f32, dt: f32, attack_tau: f32, release_tau: f32) -> f32 {
    let tau = if x > y { attack_tau } else { release_tau };
    if tau <= 0.0 {
        return x;
    }
    y + (x - y) * (1.0 - (-dt / tau).exp())
}

struct Band {
    src: usize, // 0 = long, 1 = mid
    bin_lo: usize,
    bin_hi: usize,
    tilt: f32,
    norm: f32,
    value: f32,
}

pub struct Analyzer {
    cfg: AnalyzerConfig,
    ring: Ring,
    long: Stft,
    mid: Stft,
    short: Stft,
    loudness: Loudness,
    onsets: OnsetDetector,
    beat: BeatTracker,
    chroma: Chroma,
    since_hop: usize,
    hop_dt: f32,
    samples_seen: u64,
    acc_mid: f64,
    acc_side: f64,
    acc_peak: f32,
    bands: Vec<Band>,
    bmt_avg: [f32; 3],
    bmt_fast: [f32; 3],
    bmt_att: [f32; 3],
    e_short: f32,
    e_long: f32,
    frame: FeatureFrame,
}

impl Analyzer {
    pub fn new(cfg: AnalyzerConfig) -> Self {
        let sr = cfg.sample_rate;
        let long = Stft::new(N_LONG);
        let mid = Stft::new(N_MID);
        let short = Stft::new(N_SHORT);
        let frame_rate = sr as f32 / cfg.hop as f32;

        // Log-spaced bands; the lowest ones come from the long FFT for resolution.
        let mut bands = Vec::with_capacity(cfg.bands);
        let (lo, hi) = (cfg.band_lo_hz.ln(), cfg.band_hi_hz.ln());
        for b in 0..cfg.bands {
            let f0 = (lo + (hi - lo) * b as f32 / cfg.bands as f32).exp();
            let f1 = (lo + (hi - lo) * (b + 1) as f32 / cfg.bands as f32).exp();
            let fc = (f0 * f1).sqrt();
            let (src, stft) = if fc < 150.0 { (0, &long) } else { (1, &mid) };
            let bin_hz = stft.bin_hz(sr);
            let nb = stft.bins();
            let bin_lo = ((f0 / bin_hz).floor() as usize).clamp(1, nb - 1);
            let bin_hi = ((f1 / bin_hz).ceil() as usize).clamp(bin_lo + 1, nb);
            bands.push(Band { src, bin_lo, bin_hi, tilt: fc / 1000.0, norm: 1e-3, value: 0.0 });
        }

        let onsets = OnsetDetector::new(short.bins(), short.bin_hz(sr), mid.bins(), mid.bin_hz(sr), frame_rate);
        let chroma = Chroma::new(long.bins(), long.bin_hz(sr));
        Self {
            ring: Ring::new(N_LONG),
            loudness: Loudness::new(sr),
            onsets,
            beat: BeatTracker::new(frame_rate),
            chroma,
            long,
            mid,
            short,
            since_hop: 0,
            hop_dt: cfg.hop as f32 / sr as f32,
            samples_seen: 0,
            acc_mid: 0.0,
            acc_side: 0.0,
            acc_peak: 0.0,
            bands,
            bmt_avg: [1e-4; 3],
            bmt_fast: [0.0; 3],
            bmt_att: [0.0; 3],
            e_short: 0.0,
            e_long: 0.0,
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
            self.acc_peak = self.acc_peak.max(l.abs()).max(r.abs());
            self.ring.push(mid);
            self.loudness.push(l, r);
            self.samples_seen += 1;
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
        let sr = self.cfg.sample_rate;
        let f = &mut self.frame;
        f.time_s = self.samples_seen as f64 / sr as f64;

        // Level, width, loudness.
        let mid_e = (self.acc_mid / hop) as f32;
        let side_e = (self.acc_side / hop) as f32;
        let rms = mid_e.sqrt();
        let peak = self.acc_peak;
        self.acc_mid = 0.0;
        self.acc_side = 0.0;
        self.acc_peak = 0.0;

        let silent = rms < SILENCE_RMS;
        f.silent = silent;
        f.rms = rms;
        f.rms_slow = follow(f.rms_slow, rms, dt, 0.3, 3.0);
        f.peak = follow(f.peak, peak, dt, 0.0, 1.5);
        let width = if silent { 0.0 } else { (side_e.sqrt() / (mid_e.sqrt() + 1e-6)).min(1.0) };
        f.stereo_width = follow(f.stereo_width, width, dt, 0.1, 0.5);

        self.loudness.hop(dt);
        f.lufs_m = self.loudness.lufs_m;
        f.lufs_s = self.loudness.lufs_s;
        f.quiet = f.lufs_s < -38.0;

        // Energy trend: 1 s vs 16 s mean-square.
        self.e_short = follow(self.e_short, mid_e, dt, 1.0, 1.0);
        self.e_long = follow(self.e_long, mid_e, dt, 16.0, 16.0);
        let trend = if self.e_long > 1e-9 && !silent { (10.0 * (self.e_short / self.e_long).log10() / 12.0).clamp(-1.0, 1.0) } else { 0.0 };
        f.energy_trend = follow(f.energy_trend, trend, dt, 0.5, 0.5);

        // Spectra.
        self.long.compute(&self.ring);
        self.mid.compute(&self.ring);
        self.short.compute(&self.ring);

        // Centroid from the mid spectrum.
        let bin_hz = self.mid.bin_hz(sr);
        let (mut num, mut den) = (0.0f32, 0.0f32);
        for (i, p) in self.mid.power.iter().enumerate().skip(1) {
            num += i as f32 * bin_hz * p;
            den += p;
        }
        let centroid = if silent || den <= 0.0 { 0.0 } else { num / den };
        f.centroid_hz = follow(f.centroid_hz, centroid, dt, 0.2, 0.6);

        // Bands.
        for (b, out) in self.bands.iter_mut().zip(f.bands.iter_mut()) {
            let power = if b.src == 0 { &self.long.power } else { &self.mid.power };
            let p: f32 = power[b.bin_lo..b.bin_hi].iter().sum::<f32>() * b.tilt;
            let v = p.max(0.0).powf(0.4);
            b.norm = follow(b.norm, v, dt, 0.05, 8.0).max(1e-3);
            let n = if silent { 0.0 } else { (v / b.norm).min(1.5) };
            b.value = follow(b.value, n, dt, 0.010, 0.200);
            *out = b.value;
        }

        // bass / mid / treb relative to running average (mid spectrum).
        let ranges = [(30.0, 200.0), (200.0, 2000.0), (2000.0, 16000.0)];
        for (k, (f0, f1)) in ranges.iter().enumerate() {
            let lo = ((f0 / bin_hz) as usize).max(1);
            let hi = ((f1 / bin_hz) as usize).min(self.mid.power.len());
            let e: f32 = self.mid.power[lo..hi].iter().sum();
            self.bmt_avg[k] = follow(self.bmt_avg[k], e, dt, 5.0, 5.0).max(1e-7);
            let rel = if silent { 0.0 } else { (e / self.bmt_avg[k]).min(4.0) };
            self.bmt_fast[k] = follow(self.bmt_fast[k], rel, dt, 0.010, 0.100);
            self.bmt_att[k] = follow(self.bmt_att[k], rel, dt, 0.3, 0.5);
        }
        f.bass = self.bmt_fast[0];
        f.mid = self.bmt_fast[1];
        f.treb = self.bmt_fast[2];
        f.bass_att = self.bmt_att[0];
        f.mid_att = self.bmt_att[1];
        f.treb_att = self.bmt_att[2];

        // Onsets, beat, chroma.
        self.onsets.process(&self.short.mag, &self.mid.mag, dt, silent);
        f.onset_strength = self.onsets.strength;
        f.onset_hit = self.onsets.hit;
        f.onset_pulse = self.onsets.pulse;

        self.beat.process(self.onsets.flux_total, dt, silent);
        f.bpm = self.beat.bpm;
        f.beat_confidence = self.beat.confidence;
        f.beat_phase = self.beat.phase;
        f.beat_count = self.beat.count;
        f.beat_pulse = self.beat.pulse;

        self.chroma.process(&self.long.mag, dt, silent);
        f.chroma = self.chroma.values;
        f.chroma_slow = self.chroma.slow;
        f.key_class = self.chroma.dominant;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(f: f32, amp: f32, sr: u32, i: usize) -> f32 {
        amp * (2.0 * std::f32::consts::PI * f * i as f32 / sr as f32).sin()
    }

    #[test]
    fn sine_lands_in_the_right_band() {
        let cfg = AnalyzerConfig::default();
        let mut a = Analyzer::new(cfg.clone());
        let mut buf = Vec::new();
        for i in 0..cfg.sample_rate as usize {
            let s = sine(1000.0, 0.5, cfg.sample_rate, i);
            buf.push(s);
            buf.push(s);
        }
        a.push(&buf);
        let fr = a.frame();
        assert!(!fr.silent);
        assert!((fr.centroid_hz - 1000.0).abs() < 150.0, "centroid {}", fr.centroid_hz);
        assert!(fr.stereo_width < 0.05);
        let (imax, _) = fr.bands.iter().enumerate().fold((0, 0.0), |m, (i, v)| if *v > m.1 { (i, *v) } else { m });
        assert!((14..=20).contains(&imax), "peak band {imax}");
    }

    #[test]
    fn silence_is_silent() {
        let mut a = Analyzer::new(AnalyzerConfig::default());
        a.push(&vec![0.0; 48_000]);
        assert!(a.frame().silent);
        assert_eq!(a.frame().bass, 0.0);
        assert_eq!(a.frame().lufs_s, -70.0);
    }

    #[test]
    fn a_major_chord_shows_in_chroma() {
        let cfg = AnalyzerConfig::default();
        let sr = cfg.sample_rate;
        let mut a = Analyzer::new(cfg);
        let mut buf = Vec::new();
        for i in 0..(sr as usize * 2) {
            // A4, C#5, E5
            let s = sine(440.0, 0.2, sr, i) + sine(554.37, 0.2, sr, i) + sine(659.25, 0.2, sr, i);
            buf.push(s);
            buf.push(s);
        }
        a.push(&buf);
        let c = a.frame().chroma;
        let mut idx: Vec<usize> = (0..12).collect();
        idx.sort_by(|x, y| c[*y].partial_cmp(&c[*x]).unwrap());
        let top: Vec<usize> = idx[..3].to_vec();
        for pc in [9usize, 1, 4] {
            assert!(top.contains(&pc), "chroma top3 {top:?} values {c:?}");
        }
    }

    #[test]
    fn clicks_produce_onsets_and_tempo() {
        let cfg = AnalyzerConfig::default();
        let sr = cfg.sample_rate;
        let mut a = Analyzer::new(cfg);
        let bpm = 120.0;
        let period = (60.0 / bpm * sr as f32) as usize;
        let total = sr as usize * 16;
        let mut buf = Vec::with_capacity(total * 2);
        let mut seed = 1u32;
        for i in 0..total {
            // Low-level noise bed plus a 10 ms burst of noise every beat.
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let r = ((seed >> 8) as f32 / (1u32 << 24) as f32) * 2.0 - 1.0;
            let in_click = i % period < sr as usize / 100;
            let s = if in_click { 0.6 * r } else { 0.01 * r };
            buf.push(s);
            buf.push(s);
        }
        let mut hits = 0;
        for chunk in buf.chunks(1024) {
            a.push(chunk);
            if a.frame().onset_hit[3] {
                hits += 1;
            }
        }
        let f = a.frame();
        // 32 clicks in 16 s at 120 BPM; allow some misses and a few extras.
        assert!((24..=48).contains(&hits), "onset hits {hits}");
        assert!((f.bpm - bpm).abs() < 3.0, "bpm {} conf {}", f.bpm, f.beat_confidence);
        assert!(f.beat_confidence > 0.3, "confidence {}", f.beat_confidence);
    }
}
