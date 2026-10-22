//! Per-layer audio band: a layer can react to one part of the spectrum
//! instead of the full mix. Two tools: a band-pass on the raw waveform (for
//! the scope) and a band-restricted view of the feature frame (energy,
//! onsets, spectrum) for everything else.

use audioviz_analysis::FeatureFrame;

/// Direct-form-I biquad, Butterworth low/high pass.
#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn new(lowpass: bool, fc: f32, sr: f32) -> Self {
        let fc = fc.clamp(10.0, sr * 0.45);
        let w0 = std::f32::consts::TAU * fc / sr;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let (b0, b1, b2) = if lowpass { ((1.0 - cos) * 0.5, 1.0 - cos, (1.0 - cos) * 0.5) } else { ((1.0 + cos) * 0.5, -(1.0 + cos), (1.0 + cos) * 0.5) };
        let a0 = 1.0 + alpha;
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: -2.0 * cos / a0, a2: (1.0 - alpha) / a0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = if y.is_finite() { y } else { 0.0 };
        self.y1
    }
}

/// Stereo band-pass: 4th-order (two cascaded 2nd-order) Butterworth high pass
/// at `lo` and low pass at `hi`.
pub struct BandPass {
    lo: f32,
    hi: f32,
    sr: f32,
    // [channel][stage]
    hp: [[Biquad; 2]; 2],
    lp: [[Biquad; 2]; 2],
}

impl BandPass {
    pub fn new(lo: f32, hi: f32, sr: f32) -> Self {
        let hp = [[Biquad::new(false, lo, sr); 2]; 2];
        let lp = [[Biquad::new(true, hi, sr); 2]; 2];
        Self { lo, hi, sr, hp, lp }
    }

    /// Re-tune if the band or sample rate changed (keeps state otherwise).
    pub fn retune(&mut self, lo: f32, hi: f32, sr: f32) {
        if (self.lo - lo).abs() > 0.01 || (self.hi - hi).abs() > 0.01 || (self.sr - sr).abs() > 0.5 {
            *self = Self::new(lo, hi, sr);
        }
    }

    pub fn process(&mut self, points: &[[f32; 2]]) -> Vec<[f32; 2]> {
        points
            .iter()
            .map(|p| {
                let mut out = [0.0f32; 2];
                for c in 0..2 {
                    let mut v = p[c];
                    for s in 0..2 {
                        v = self.hp[c][s].process(v);
                    }
                    for s in 0..2 {
                        v = self.lp[c][s].process(v);
                    }
                    out[c] = v;
                }
                out
            })
            .collect()
    }
}

/// Centre frequency of each of `n` bands, in the analyzer's default layout.
fn band_centres(n: usize) -> Vec<f32> {
    use audioviz_analysis::filterbank::{band_centre_hz, band_edges};
    let cfg = audioviz_analysis::AnalyzerConfig::default();
    band_edges(n, cfg.band_lo_hz, cfg.band_hi_hz, cfg.band_corner_hz).into_iter().map(|e| band_centre_hz(e, cfg.band_corner_hz)).collect()
}

/// A copy of `f` as seen through the band `lo..hi` Hz: energies, onsets and
/// the spectrum come from that range only, everything else is unchanged.
pub fn band_features(f: &FeatureFrame, lo: f32, hi: f32) -> FeatureFrame {
    let mut out = f.clone();
    let n = f.bands.len();
    if n == 0 {
        return out;
    }
    let centres = band_centres(n);
    let (mut sum_in, mut count_in, mut sum_all) = (0.0f32, 0usize, 0.0f32);
    for (i, b) in f.bands.iter().enumerate() {
        let hz = centres[i];
        sum_all += b;
        if hz >= lo && hz <= hi {
            sum_in += b;
            count_in += 1;
            out.bands[i] = *b;
        } else {
            out.bands[i] = b * 0.08;
            if let Some(l) = out.levels.get_mut(i) {
                *l *= 0.08;
            }
            if let Some(a) = out.attacks.get_mut(i) {
                *a = 0.0;
            }
        }
    }
    // The fine spectrum outside the range sinks toward nothing (as dB it scales down).
    let row_hz = audioviz_analysis::history::LIN_HI_HZ / audioviz_analysis::history::LIN_ROWS as f32;
    for (r, (s, m)) in out.spectrum.iter_mut().zip(out.spectrum_smooth.iter_mut()).enumerate() {
        let hz = (r as f32 + 0.5) * row_hz;
        if hz < lo || hz > hi {
            *s *= 0.3;
            *m *= 0.3;
        }
    }
    let energy = if count_in > 0 { sum_in / count_in as f32 } else { 0.0 };
    let share = if sum_all > 1e-6 { sum_in / sum_all } else { 0.0 };
    // Band energy stands in for every "how loud is it" feature the visuals read.
    let att = (energy * 2.0).clamp(0.0, 2.0);
    out.bass = att;
    out.mid = att;
    out.treb = att;
    out.bass_att = att;
    out.mid_att = att;
    out.treb_att = att;
    out.rms = f.rms * share.sqrt();
    out.peak = f.peak * share.sqrt().max(0.05);
    // Broad bands that the range does not reach fade out like the bands.
    let edges = audioviz_analysis::bands::BROAD_EDGES_HZ;
    for k in 0..audioviz_analysis::bands::BROAD {
        if edges[k + 1] <= lo || edges[k] >= hi {
            out.broad_level[k] *= 0.08;
            out.broad_relative[k] *= 0.08;
            out.broad_attack[k] = 0.0;
        }
    }
    // Onsets from the transient group that covers the band's centre.
    let centre = (lo * hi).sqrt();
    let g = if centre < 250.0 { 0 } else if centre < 2000.0 { 1 } else { 2 };
    out.onset_pulse[3] = f.onset_pulse[g];
    out.onset_strength[3] = f.onset_strength[g];
    out.onset_hit[3] = f.onset_hit[g];
    out.onset_count[3] = f.onset_count[g];
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bandpass_passes_in_band_and_rejects_out_of_band() {
        let sr = 48_000.0;
        let mut bp = BandPass::new(60.0, 250.0, sr);
        let tone = |hz: f32| -> Vec<[f32; 2]> { (0..48_000).map(|i| { let v = (std::f32::consts::TAU * hz * i as f32 / sr).sin(); [v, v] }).collect() };
        let rms = |v: &[[f32; 2]]| (v.iter().skip(24_000).map(|p| p[0] * p[0]).sum::<f32>() / 24_000.0).sqrt();
        let inside = rms(&bp.process(&tone(120.0)));
        let mut bp2 = BandPass::new(60.0, 250.0, sr);
        let outside = rms(&bp2.process(&tone(2000.0)));
        assert!(inside > 0.5, "in-band rms {inside}");
        assert!(outside < 0.02, "out-of-band rms {outside}");
    }

    #[test]
    fn band_features_uses_only_the_range() {
        let mut f = FeatureFrame::default();
        f.bands = vec![0.0; 32];
        // Put energy only in the top bands (air).
        for b in f.bands.iter_mut().skip(26) {
            *b = 1.0;
        }
        f.onset_pulse = [0.1, 0.2, 0.9, 0.5];
        let low = band_features(&f, 20.0, 250.0);
        assert!(low.bass < 0.05);
        assert_eq!(low.onset_pulse[3], 0.1);
        let air = band_features(&f, 4000.0, 16000.0);
        assert!(air.bass > 1.0);
        assert_eq!(air.onset_pulse[3], 0.9);
    }
}
