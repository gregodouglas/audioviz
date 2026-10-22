//! Real-time music feature extraction for audioviz.
//!
//! Sample-clocked (per hop), frame-rate independent. v1 as planned in
//! docs/research/03-audio-analysis.md: multi-resolution STFT, bands from a
//! gammatone filterbank (ADR-0006) with per-band auto-gain, MilkDrop-style bass/mid/treb, K-weighted loudness,
//! spectral-flux onsets per band group, a predictive beat clock, chroma,
//! stereo width and a long-window energy trend.

pub mod bands;
pub mod bar;
pub mod beat;
pub mod chroma;
pub mod context;
pub mod filterbank;
pub mod history;
pub mod loudness;
pub mod onset;
pub mod sections;
pub mod stft;

use bands::{BROAD, BandSignals};
use bar::BarTracker;
use beat::BeatTracker;
use chroma::Chroma;
use context::{Context, ContextInput};
use filterbank::FilterBank;
use history::{HopRecord, RECORD_BANDS, SPEC_ROWS, SpectrumMap};
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
    /// The bands are even on ln(1 + f / corner): logarithmic above it,
    /// close to even in hertz below (see `filterbank`).
    pub band_corner_hz: f32,
    /// Edges of the six broad bands (sub, bass, low mid, mid, presence, air), Hz.
    pub broad_edges_hz: [f32; BROAD + 1],
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self { sample_rate: 48_000, hop: 512, bands: 32, band_lo_hz: 30.0, band_hi_hz: 16_000.0, band_corner_hz: 100.0, broad_edges_hz: bands::BROAD_EDGES_HZ }
    }
}

const N_LONG: usize = 8192;
const N_MID: usize = 2048;
const N_SHORT: usize = 512;
const SILENCE_RMS: f32 = 0.0001; // -80 dBFS: quiet app volume must still count as signal

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
    /// The bands (low to high, see `filterbank`), each against its own
    /// recent best (relative), smoothed, roughly 0..1.
    pub bands: Vec<f32>,
    /// The bands on one scale for all: 0..1 over the 42 dB under the
    /// track's loud level (see `bands`).
    pub levels: Vec<f32>,
    /// How far each band rose over its last ~150 ms, 0..1.
    pub attacks: Vec<f32>,
    /// Six broad bands (sub, bass, low mid, mid, presence, air): level,
    /// relative to their own average (1 = typical), attack.
    pub broad_level: [f32; BROAD],
    pub broad_relative: [f32; BROAD],
    pub broad_attack: [f32; BROAD],
    pub centroid_hz: f32,
    /// 0 = mono, 1 = wide.
    pub stereo_width: f32,
    /// -1 = all left, 1 = all right (energy), smoothed over ~150 ms.
    pub balance: f32,
    /// 0 = tonal (notes, chords), 1 = noise-like (hats, cymbals, distortion,
    /// rain): spectral flatness from 100 Hz to 10 kHz.
    pub noisiness: f32,
    /// 0..1: how busy the music is, onsets per second over the last couple
    /// of seconds (1 = eight or more a second).
    pub activity: f32,
    /// 0..1: the centroid on a log scale, 200 Hz to 8 kHz.
    pub brightness: f32,
    /// Onsets per group [low, mid, high, total]: continuous strength,
    /// discrete hit this hop, and a decaying pulse. A hit lasts one hop, so
    /// a reader slower than the hop rate should use `onset_count` instead.
    pub onset_strength: [f32; 4],
    pub onset_hit: [bool; 4],
    /// Onsets so far per group; wraps.
    pub onset_count: [u32; 4],
    pub onset_pulse: [f32; 4],
    /// Beat clock.
    pub bpm: f32,
    pub beat_confidence: f32,
    pub beat_phase: f32,
    pub beat_count: u32,
    pub beat_pulse: f32,
    /// 0..1: how steady the pulse has been over the last ~8 s (the share of
    /// the tempo estimates that agreed with it).
    pub pulse_stability: f32,
    /// The bar, assuming four beats: 0..1 through it, which beat this is
    /// (0 = the first), and how sure the first beat is (see `bar`).
    pub bar_phase: f32,
    pub beat_in_bar: u32,
    pub bar_confidence: f32,
    /// Pitch-class profile (C=0), fast and slow, plus the dominant class.
    pub chroma: [f32; 12],
    pub chroma_slow: [f32; 12],
    pub key_class: u8,
    /// -1..1: negative = energy falling vs the last ~16 s, positive = building.
    pub energy_trend: f32,
    /// Context over seconds (see `context`): brightness and density
    /// trends (-1..1), bass presence and contrast (0..1), novelty against
    /// its typical level (0..1.5) with a count of transitions and a pulse
    /// (1 at one, fading over ~0.6 s), build intensity (0..1), and drops
    /// (a count, how sure the last was, a pulse).
    pub brightness_trend: f32,
    pub density_trend: f32,
    pub bass_presence: f32,
    pub contrast: f32,
    pub transition: f32,
    pub transition_count: u32,
    pub transition_pulse: f32,
    pub build: f32,
    pub drop_count: u32,
    pub drop_confidence: f32,
    pub drop_pulse: f32,
    /// Sections from the track's history (see `sections`): the current
    /// one's label (0 = A), how much it sounds like an earlier one (0..1),
    /// bars into it, bars to the next expected change and how sure, and
    /// 0..1 through the current eight-bar phrase.
    pub section_label: u32,
    pub seen_before: f32,
    pub bars_in_section: u32,
    pub bars_to_change: f32,
    pub change_confidence: f32,
    pub phrase_phase: f32,
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
            levels: Vec::new(),
            attacks: Vec::new(),
            broad_level: [0.0; BROAD],
            broad_relative: [0.0; BROAD],
            broad_attack: [0.0; BROAD],
            centroid_hz: 0.0,
            stereo_width: 0.0,
            balance: 0.0,
            noisiness: 0.0,
            activity: 0.0,
            brightness: 0.0,
            onset_strength: [0.0; 4],
            onset_hit: [false; 4],
            onset_count: [0; 4],
            onset_pulse: [0.0; 4],
            bpm: 120.0,
            beat_confidence: 0.0,
            beat_phase: 0.0,
            beat_count: 0,
            beat_pulse: 0.0,
            pulse_stability: 0.0,
            bar_phase: 0.0,
            beat_in_bar: 0,
            bar_confidence: 0.0,
            chroma: [0.0; 12],
            chroma_slow: [0.0; 12],
            key_class: 0,
            energy_trend: 0.0,
            brightness_trend: 0.0,
            density_trend: 0.0,
            bass_presence: 0.0,
            contrast: 0.0,
            transition: 0.0,
            transition_count: 0,
            transition_pulse: 0.0,
            build: 0.0,
            drop_count: 0,
            drop_confidence: 0.0,
            drop_pulse: 0.0,
            section_label: 0,
            seen_before: 0.0,
            bars_in_section: 0,
            bars_to_change: 8.0,
            change_confidence: 0.0,
            phrase_phase: 0.0,
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

pub struct Analyzer {
    cfg: AnalyzerConfig,
    ring: Ring,
    long: Stft,
    mid: Stft,
    short: Stft,
    loudness: Loudness,
    onsets: OnsetDetector,
    beat: BeatTracker,
    bar: BarTracker,
    context: Context,
    chroma: Chroma,
    since_hop: usize,
    hop_dt: f32,
    samples_seen: u64,
    acc_mid: f64,
    acc_side: f64,
    acc_peak: f32,
    acc_lr: [f64; 2],
    /// Onsets (all) counted down over ~2 s, for `activity`.
    onset_rate: f32,
    bands: BandSignals,
    band_edges: Vec<(f32, f32)>,
    filterbank: FilterBank,
    band_power: Vec<f32>,
    /// This hop's mono samples, for the filterbank.
    hop_buf: Vec<f32>,
    bmt_avg: [f32; 3],
    bmt_fast: [f32; 3],
    bmt_att: [f32; 3],
    e_short: f32,
    e_long: f32,
    frame: FeatureFrame,
    spectrum_map: SpectrumMap,
    /// Records of the hops since the last `drain_records`.
    records: Vec<HopRecord>,
    hops: u64,
    last_beat_count: u32,
    /// The playing track, and the settled tempos of tracks heard this session.
    track: Option<u64>,
    tempos: std::collections::HashMap<u64, f32>,
}

impl Analyzer {
    pub fn new(cfg: AnalyzerConfig) -> Self {
        let sr = cfg.sample_rate;
        let long = Stft::new(N_LONG);
        let mid = Stft::new(N_MID);
        let short = Stft::new(N_SHORT);
        let frame_rate = sr as f32 / cfg.hop as f32;

        let band_edges = filterbank::band_edges(cfg.bands, cfg.band_lo_hz, cfg.band_hi_hz, cfg.band_corner_hz);
        let filterbank = FilterBank::new(&band_edges, cfg.band_corner_hz, sr);
        let centres: Vec<f32> = band_edges.iter().map(|e| filterbank::band_centre_hz(*e, cfg.band_corner_hz)).collect();
        let bands = BandSignals::new(&centres, &cfg.broad_edges_hz);

        let onsets = OnsetDetector::new(short.bins(), short.bin_hz(sr), mid.bins(), mid.bin_hz(sr), frame_rate);
        let chroma = Chroma::new(long.bins(), long.bin_hz(sr));
        let spectrum_map = SpectrumMap::new(&long, &mid, sr);
        Self {
            ring: Ring::new(N_LONG),
            loudness: Loudness::new(sr),
            onsets,
            beat: BeatTracker::new(frame_rate),
            bar: BarTracker::new(),
            context: Context::new(cfg.hop as f32 / sr as f32),
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
            acc_lr: [0.0; 2],
            onset_rate: 0.0,
            bands,
            band_power: vec![0.0; band_edges.len()],
            band_edges,
            filterbank,
            hop_buf: Vec::with_capacity(cfg.hop),
            bmt_avg: [1e-4; 3],
            bmt_fast: [0.0; 3],
            bmt_att: [0.0; 3],
            e_short: 0.0,
            e_long: 0.0,
            frame: FeatureFrame { bands: vec![0.0; cfg.bands], levels: vec![0.0; cfg.bands], attacks: vec![0.0; cfg.bands], ..Default::default() },
            spectrum_map,
            records: Vec::with_capacity(64),
            hops: 0,
            last_beat_count: 0,
            track: None,
            tempos: std::collections::HashMap::new(),
            cfg,
        }
    }

    pub fn config(&self) -> &AnalyzerConfig {
        &self.cfg
    }

    /// Each band's edges in Hz, low to high.
    pub fn band_edges(&self) -> &[(f32, f32)] {
        &self.band_edges
    }

    pub fn frame(&self) -> &FeatureFrame {
        &self.frame
    }

    /// Tells the analysis a new track has started (from the player's
    /// metadata), so what it learnt about the last one can go.
    pub fn new_track(&mut self, track_id: Option<u64>) {
        // Remember the last track's tempo if it had settled.
        if let Some(id) = self.track {
            if self.beat.settled() {
                if self.tempos.len() >= 512 {
                    self.tempos.clear();
                }
                self.tempos.insert(id, self.beat.bpm);
            }
        }
        self.beat.reset();
        self.bar.reset();
        self.context.reset();
        if let Some(bpm) = track_id.and_then(|id| self.tempos.get(&id)) {
            self.beat.seed(*bpm);
        }
        self.track = track_id;
    }

    /// Moves the records of the hops analyzed since the last call into
    /// `out`. Records pile up until drained (about 94 a second), so a user
    /// of the analyzer that wants none should still drain now and then.
    pub fn drain_records(&mut self, out: &mut Vec<HopRecord>) {
        out.append(&mut self.records);
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
            self.acc_lr[0] += (l * l) as f64;
            self.acc_lr[1] += (r * r) as f64;
            self.ring.push(mid);
            self.hop_buf.push(mid);
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
        let [el, er] = self.acc_lr;
        self.acc_lr = [0.0; 2];
        let balance = if silent || el + er <= 0.0 { 0.0 } else { ((er - el) / (er + el)) as f32 };
        f.balance = follow(f.balance, balance, dt, 0.15, 0.15);

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
        f.brightness = if f.centroid_hz <= 0.0 { 0.0 } else { ((f.centroid_hz / 200.0).log2() / 40f32.log2()).clamp(0.0, 1.0) };

        // Noisiness: the spectrum's flatness (geometric over arithmetic mean)
        // from 100 Hz to 10 kHz, in dB. Windowed white noise reads about
        // -2.5 dB, music -15 to -30, a lone tone far lower.
        let (lo, hi) = ((100.0 / bin_hz) as usize, ((10_000.0 / bin_hz) as usize).min(self.mid.power.len()));
        let n = (hi - lo).max(1) as f32;
        let (mut log_sum, mut sum) = (0.0f32, 0.0f32);
        for p in &self.mid.power[lo..hi] {
            log_sum += (p + 1e-12).ln();
            sum += p;
        }
        let flat_db = if sum > 0.0 { 10.0 * ((log_sum / n).exp() / (sum / n + 1e-12)).max(1e-12).log10() } else { -60.0 };
        let noisy = if silent { 0.0 } else { ((flat_db + 25.0) / 22.5).clamp(0.0, 1.0) };
        f.noisiness = follow(f.noisiness, noisy, dt, 0.05, 0.2);

        // Bands, from the filterbank.
        self.filterbank.process(&self.hop_buf, &mut self.band_power);
        self.hop_buf.clear();
        self.bands.update(&self.band_power, dt, silent);
        f.bands.copy_from_slice(&self.bands.relative);
        f.levels.copy_from_slice(&self.bands.level);
        f.attacks.copy_from_slice(&self.bands.attack);
        f.broad_level = self.bands.broad_level;
        f.broad_relative = self.bands.broad_relative;
        f.broad_attack = self.bands.broad_attack;

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
        self.onsets.process(&self.short.mag, &self.mid.mag, self.bands.low_flux, dt, silent);
        f.onset_strength = self.onsets.strength;
        f.onset_hit = self.onsets.hit;
        f.onset_count = self.onsets.count;
        self.onset_rate = self.onset_rate * (-dt / 2.0).exp() + self.onsets.hit[3] as u32 as f32;
        f.activity = (self.onset_rate / 2.0 / 8.0).min(1.0);
        f.onset_pulse = self.onsets.pulse;

        self.beat.process(self.onsets.flux_total, dt, silent);
        f.bpm = self.beat.bpm;
        f.beat_confidence = self.beat.confidence;
        f.beat_phase = self.beat.phase;
        f.beat_count = self.beat.count;
        f.beat_pulse = self.beat.pulse;
        f.pulse_stability = self.beat.stability;

        self.chroma.process(&self.long.mag, dt, silent);
        f.chroma = self.chroma.values;
        f.chroma_slow = self.chroma.slow;
        f.key_class = self.chroma.dominant;

        self.bar.process(f.beat_count, f.beat_phase, f.beat_confidence, f.onset_strength[0], f.broad_attack[1], &self.chroma.raw, dt, silent);
        f.bar_phase = self.bar.bar_phase;
        f.beat_in_bar = self.bar.beat_in_bar;
        f.bar_confidence = self.bar.confidence;

        let bar_started = f.beat_count != self.last_beat_count && f.beat_in_bar == 0;
        let input = ContextInput { lufs_m: f.lufs_m, broad_level: &f.broad_level, brightness: f.brightness, noisiness: f.noisiness, activity: f.activity, chroma: &self.chroma.raw, time_s: f.time_s, bar_started, bar_phase: f.bar_phase };
        self.context.process(&input, dt, silent);
        let c = &self.context;
        f.brightness_trend = c.brightness_trend;
        f.density_trend = c.density_trend;
        f.bass_presence = c.bass_presence;
        f.contrast = c.contrast;
        f.transition = c.transition;
        f.transition_count = c.transitions;
        f.transition_pulse = c.transition_pulse;
        f.build = c.build;
        f.drop_count = c.drops;
        f.drop_confidence = c.drop_confidence;
        f.drop_pulse = c.drop_pulse;
        f.section_label = c.sections.label;
        f.seen_before = c.sections.seen_before;
        f.bars_in_section = c.sections.bars_in;
        f.bars_to_change = c.sections.bars_to_change;
        f.change_confidence = c.sections.change_confidence;
        f.phrase_phase = c.sections.phrase_phase;

        // Remember the hop. A reader that never drains is capped at a minute.
        if self.records.len() >= 94 * 60 {
            self.records.remove(0);
        }
        let mut spectrum = [0u8; SPEC_ROWS];
        self.spectrum_map.column(&self.long, &self.mid, &mut spectrum);
        let mut linear = [0u8; history::LIN_ROWS];
        self.spectrum_map.linear(&self.mid, &mut linear);
        let copy = |v: &[f32]| -> [f32; RECORD_BANDS] {
            let mut o = [0.0f32; RECORD_BANDS];
            for (o, b) in o.iter_mut().zip(v) {
                *o = *b;
            }
            o
        };
        let (bands, levels, attacks) = (copy(&f.bands), copy(&f.levels), copy(&f.attacks));
        let hits = self.onsets.hit.iter().enumerate().fold(0u8, |m, (g, h)| m | ((*h as u8) << g));
        self.records.push(HopRecord {
            hop: self.hops,
            time_s: f.time_s,
            rms: f.rms,
            lufs_m: f.lufs_m,
            spectrum,
            linear,
            bands,
            levels,
            attacks,
            broad_level: f.broad_level,
            broad_attack: f.broad_attack,
            onset_strength: f.onset_strength,
            onset_threshold: self.onsets.threshold,
            onset_hits: hits,
            bpm: f.bpm,
            beat_confidence: f.beat_confidence,
            pulse_stability: f.pulse_stability,
            beat_in_bar: f.beat_in_bar as u8,
            bar_confidence: f.bar_confidence,
            beat_phase: f.beat_phase,
            beat: f.beat_count != self.last_beat_count,
            bass: f.bass,
            mid: f.mid,
            treb: f.treb,
            centroid_hz: f.centroid_hz,
            stereo_width: f.stereo_width,
            balance: f.balance,
            noisiness: f.noisiness,
            activity: f.activity,
            energy_trend: f.energy_trend,
            brightness_trend: f.brightness_trend,
            density_trend: f.density_trend,
            bass_presence: f.bass_presence,
            contrast: f.contrast,
            transition: f.transition,
            transitions: f.transition_count,
            build: f.build,
            drops: f.drop_count,
            drop_confidence: f.drop_confidence,
            section_label: f.section_label.min(255) as u8,
            seen_before: f.seen_before,
            bars_to_change: f.bars_to_change,
            change_confidence: f.change_confidence,
            silent: f.silent,
        });
        self.hops += 1;
        self.last_beat_count = f.beat_count;
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
    fn records_carry_a_spectrum_column() {
        let cfg = AnalyzerConfig::default();
        let mut a = Analyzer::new(cfg.clone());
        let mut buf = Vec::new();
        for i in 0..cfg.sample_rate as usize {
            let s = sine(1000.0, 0.5, cfg.sample_rate, i);
            buf.push(s);
            buf.push(s);
        }
        a.push(&buf);
        let mut recs = Vec::new();
        a.drain_records(&mut recs);
        assert_eq!(recs.len(), cfg.sample_rate as usize / cfg.hop);
        assert!(recs.windows(2).all(|w| w[1].hop == w[0].hop + 1));
        let last = recs.last().unwrap();
        let (row, _) = last.spectrum.iter().enumerate().fold((0, 0u8), |m, (i, v)| if *v > m.1 { (i, *v) } else { m });
        let (lo, hi) = (history::SPEC_LO_HZ.ln(), history::SPEC_HI_HZ.ln());
        let hz = (lo + (hi - lo) * (row as f32 + 0.5) / SPEC_ROWS as f32).exp();
        assert!((hz / 1000.0).log2().abs() < 0.1, "loudest row at {hz} Hz");
        // A half-scale sine is -6 dB.
        let db = history::SPEC_FLOOR_DB * (1.0 - last.spectrum[row] as f32 / 255.0);
        assert!((db + 6.0).abs() < 1.5, "{db} dB");
    }

    #[test]
    fn records_carry_a_linear_column() {
        // A tone lands on its row of the linear column, low or high.
        for hz in [1000.0f32, 12_000.0] {
            let cfg = AnalyzerConfig::default();
            let mut a = Analyzer::new(cfg.clone());
            let buf: Vec<f32> = (0..cfg.sample_rate as usize / 2).flat_map(|i| {
                let s = sine(hz, 0.5, cfg.sample_rate, i);
                [s, s]
            }).collect();
            a.push(&buf);
            let mut recs = Vec::new();
            a.drain_records(&mut recs);
            let last = recs.last().unwrap();
            let (row, v) = last.linear.iter().enumerate().fold((0, 0u8), |m, (i, v)| if *v > m.1 { (i, *v) } else { m });
            let want = hz / (history::LIN_HI_HZ / history::LIN_ROWS as f32);
            assert!((row as f32 + 0.5 - want).abs() <= 1.0, "{hz} Hz at row {row}, want about {want}");
            let db = history::SPEC_FLOOR_DB * (1.0 - v as f32 / 255.0);
            assert!((db + 6.0).abs() < 1.5, "{hz} Hz: {db} dB");
        }
    }

    #[test]
    fn noise_is_noisy_and_a_tone_is_not() {
        let cfg = AnalyzerConfig::default();
        let mut seed = 3u32;
        let mut noise = Analyzer::new(cfg.clone());
        let mut tone = Analyzer::new(cfg.clone());
        let (mut nb, mut tb) = (Vec::new(), Vec::new());
        for i in 0..cfg.sample_rate as usize {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let r = ((seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * 0.3;
            nb.extend([r, r]);
            let s = sine(440.0, 0.3, cfg.sample_rate, i);
            tb.extend([s, s]);
        }
        noise.push(&nb);
        tone.push(&tb);
        assert!(noise.frame().noisiness > 0.8, "noise {}", noise.frame().noisiness);
        assert!(tone.frame().noisiness < 0.1, "tone {}", tone.frame().noisiness);
    }

    #[test]
    fn the_left_channel_alone_reads_left() {
        let cfg = AnalyzerConfig::default();
        let mut a = Analyzer::new(cfg.clone());
        let buf: Vec<f32> = (0..cfg.sample_rate as usize).flat_map(|i| [sine(300.0, 0.3, cfg.sample_rate, i), 0.0]).collect();
        a.push(&buf);
        assert!(a.frame().balance < -0.95, "balance {}", a.frame().balance);
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
        // One hop per push here, so the count saw every hit.
        assert_eq!(f.onset_count[3] as usize, hits);
        assert!((f.bpm - bpm).abs() < 3.0, "bpm {} conf {}", f.bpm, f.beat_confidence);
        assert!(f.beat_confidence > 0.3, "confidence {}", f.beat_confidence);
    }
}
