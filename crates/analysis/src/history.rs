//! The per-hop record the engine remembers (ADR-0006): one compact entry
//! per analysis hop with everything a director view or a later prediction
//! needs to look back on, including a log-frequency dB spectrum column for
//! a spectrogram.

use crate::stft::Stft;

/// Rows of the spectrum column, log-spaced from `SPEC_LO_HZ` to `SPEC_HI_HZ`.
pub const SPEC_ROWS: usize = 160;
pub const SPEC_LO_HZ: f32 = 30.0;
pub const SPEC_HI_HZ: f32 = 16_000.0;
/// The column's floor: 0 in `spectrum` is this many dB below full scale, 255 is 0 dB.
pub const SPEC_FLOOR_DB: f32 = -96.0;
/// Bands kept per record (the analyzer's default count).
pub const RECORD_BANDS: usize = 32;

#[derive(Debug, Clone)]
pub struct HopRecord {
    /// Hops since the analyzer started.
    pub hop: u64,
    /// Seconds of audio analyzed at the end of this hop.
    pub time_s: f64,
    pub rms: f32,
    pub lufs_m: f32,
    /// dB spectrum, low to high: 0 = `SPEC_FLOOR_DB`, 255 = full scale.
    pub spectrum: [u8; SPEC_ROWS],
    /// The bands as the visuals get them (relative, smoothed), low to high,
    /// their levels on one scale, and their attacks.
    pub bands: [f32; RECORD_BANDS],
    pub levels: [f32; RECORD_BANDS],
    pub attacks: [f32; RECORD_BANDS],
    /// The six broad bands' level and attack (see `bands`).
    pub broad_level: [f32; crate::bands::BROAD],
    pub broad_attack: [f32; crate::bands::BROAD],
    /// Onset strength and the threshold it must peak over, on the same scale
    /// (low, mid, high, all); and which groups hit this hop, as bits.
    pub onset_strength: [f32; 4],
    pub onset_threshold: [f32; 4],
    pub onset_hits: u8,
    pub bpm: f32,
    pub beat_confidence: f32,
    pub pulse_stability: f32,
    /// Which beat of the bar (0 = the first), and how sure.
    pub beat_in_bar: u8,
    pub bar_confidence: f32,
    pub beat_phase: f32,
    /// The beat clock wrapped this hop.
    pub beat: bool,
    pub bass: f32,
    pub mid: f32,
    pub treb: f32,
    pub centroid_hz: f32,
    pub stereo_width: f32,
    pub balance: f32,
    pub noisiness: f32,
    pub activity: f32,
    pub energy_trend: f32,
    /// Context (see `context`).
    pub brightness_trend: f32,
    pub density_trend: f32,
    pub bass_presence: f32,
    pub contrast: f32,
    pub transition: f32,
    pub transitions: u32,
    pub build: f32,
    pub drops: u32,
    pub drop_confidence: f32,
    pub silent: bool,
}

/// Which spectrum and which bins feed each row of the column.
pub(crate) struct SpectrumMap {
    rows: Vec<Row>,
}

struct Row {
    long: bool,
    lo: usize,
    hi: usize,
}

impl SpectrumMap {
    /// Rows under 300 Hz read the long transform (resolution), the rest the
    /// mid one (timing).
    pub(crate) fn new(long: &Stft, mid: &Stft, sample_rate: u32) -> Self {
        let (lo, hi) = (SPEC_LO_HZ.ln(), SPEC_HI_HZ.ln());
        let rows = (0..SPEC_ROWS)
            .map(|r| {
                let f0 = (lo + (hi - lo) * r as f32 / SPEC_ROWS as f32).exp();
                let f1 = (lo + (hi - lo) * (r + 1) as f32 / SPEC_ROWS as f32).exp();
                let long_row = (f0 * f1).sqrt() < 300.0;
                let stft = if long_row { long } else { mid };
                let bin_hz = stft.bin_hz(sample_rate);
                let n = stft.bins();
                let lo = ((f0 / bin_hz).round() as usize).clamp(1, n - 1);
                let hi = ((f1 / bin_hz).round() as usize).clamp(lo + 1, n);
                Row { long: long_row, lo, hi }
            })
            .collect();
        Self { rows }
    }

    /// The loudest bin under each row, in dB, quantised.
    pub(crate) fn column(&self, long: &Stft, mid: &Stft, out: &mut [u8; SPEC_ROWS]) {
        for (row, o) in self.rows.iter().zip(out.iter_mut()) {
            let power = if row.long { &long.power } else { &mid.power };
            let p = power[row.lo..row.hi].iter().fold(0.0f32, |m, v| m.max(*v));
            let db = 10.0 * p.max(1e-12).log10();
            *o = ((db - SPEC_FLOOR_DB) / -SPEC_FLOOR_DB * 255.0).clamp(0.0, 255.0) as u8;
        }
    }
}
