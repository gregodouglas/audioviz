//! The bands' filterbank (ADR-0006 step 2): one fourth-order gammatone filter
//! per band, the ear's own filter shape, run on the samples. A band answers
//! as fast as its width allows (the lowest, about 20 Hz wide, peaks some
//! 20 ms after a kick; the FFT it replaces held a 171 ms window), and its
//! envelope is smooth without a window: the filter is built by turning the
//! band down to 0 Hz and low-passing it four times, so the magnitude of what
//! comes out carries no ripple at the band's frequency.
//!
//! The bands are spaced evenly on `ln(1 + f / corner)`: logarithmic well
//! above the corner (as notes are), close to even in hertz below it, so the
//! lowest bands are wide enough to be quick.

/// Band edges `(lo, hi)` in Hz, low to high: `n` bands from `lo_hz` to
/// `hi_hz`, even on `ln(1 + f / corner_hz)`.
pub fn band_edges(n: usize, lo_hz: f32, hi_hz: f32, corner_hz: f32) -> Vec<(f32, f32)> {
    let warp = |f: f32| (1.0 + f / corner_hz).ln();
    let unwarp = |w: f32| corner_hz * (w.exp() - 1.0);
    let (w0, w1) = (warp(lo_hz), warp(hi_hz));
    (0..n).map(|b| (unwarp(w0 + (w1 - w0) * b as f32 / n as f32), unwarp(w0 + (w1 - w0) * (b + 1) as f32 / n as f32))).collect()
}

/// Where a band sits: the middle of its edges on the warped scale.
pub fn band_centre_hz(edges: (f32, f32), corner_hz: f32) -> f32 {
    let warp = |f: f32| (1.0 + f / corner_hz).ln();
    corner_hz * ((0.5 * (warp(edges.0) + warp(edges.1))).exp() - 1.0)
}

struct Gammatone {
    /// Turns the phasor one sample on (cos, sin of the centre frequency's step).
    step: (f32, f32),
    phasor: (f32, f32),
    /// One minus the stages' pole.
    k: f32,
    stages: [(f32, f32); 4],
}

pub struct FilterBank {
    bands: Vec<Gammatone>,
}

impl FilterBank {
    /// One filter per band, centred on it, its 3 dB width the band's width
    /// (neighbours cross 3 dB down).
    pub fn new(edges: &[(f32, f32)], corner_hz: f32, sample_rate: u32) -> Self {
        let sr = sample_rate as f32;
        let bands = edges
            .iter()
            .map(|&(lo, hi)| {
                let fc = band_centre_hz((lo, hi), corner_hz).min(sr * 0.45);
                // A fourth-order gammatone's 3 dB width is 0.87 b.
                let b = (hi - lo) / 0.87;
                let w = std::f32::consts::TAU * fc / sr;
                Gammatone { step: (w.cos(), w.sin()), phasor: (1.0, 0.0), k: 1.0 - (-std::f32::consts::TAU * b / sr).exp(), stages: [(0.0, 0.0); 4] }
            })
            .collect();
        Self { bands }
    }

    pub fn len(&self) -> usize {
        self.bands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bands.is_empty()
    }

    /// Runs every band over `samples` (mono), then writes each band's mean
    /// power over them into `out`, on the FFT's scale: a full-scale sine at
    /// a band's centre reads 1. (The mean, not the last sample: where two
    /// partials in a band beat, the envelope dips deeply for an instant.)
    pub fn process(&mut self, samples: &[f32], out: &mut [f32]) {
        for (g, o) in self.bands.iter_mut().zip(out.iter_mut()) {
            let (c, s) = g.step;
            let (mut pr, mut pi) = g.phasor;
            let k = g.k;
            let mut st = g.stages;
            let mut sum = 0.0f32;
            for &x in samples {
                // Down to 0 Hz: x times the phasor's conjugate.
                let (mut re, mut im) = (x * pr, -x * pi);
                for y in st.iter_mut() {
                    y.0 += k * (re - y.0);
                    y.1 += k * (im - y.1);
                    re = y.0;
                    im = y.1;
                }
                sum += re * re + im * im;
                (pr, pi) = (pr * c - pi * s, pr * s + pi * c);
            }
            // Keep the phasor on the unit circle.
            let norm = (pr * pr + pi * pi).sqrt().max(1e-6);
            g.phasor = (pr / norm, pi / norm);
            g.stages = st;
            // A sine of amplitude A leaves A/2 at 0 Hz.
            *o = 4.0 * sum / samples.len().max(1) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_cover_the_range_and_widen() {
        let e = band_edges(32, 30.0, 16_000.0, 100.0);
        assert!((e[0].0 - 30.0).abs() < 0.01 && (e[31].1 - 16_000.0).abs() < 1.0);
        assert!(e.windows(2).all(|w| (w[0].1 - w[1].0).abs() < 1e-3 && w[1].1 - w[1].0 > w[0].1 - w[0].0));
        // Four bands under 140 Hz, each at least 20 Hz wide.
        assert_eq!(e.iter().filter(|b| b.1 <= 140.0).count(), 4, "{e:?}");
        assert!(e[0].1 - e[0].0 >= 20.0);
    }

    #[test]
    fn a_sine_reads_one_in_its_band_and_little_two_away() {
        let sr = 48_000;
        let edges = band_edges(32, 30.0, 16_000.0, 100.0);
        for b in [1usize, 12, 24] {
            let fc = band_centre_hz(edges[b], 100.0);
            let mut fb = FilterBank::new(&edges, 100.0, sr);
            let x: Vec<f32> = (0..sr as usize).map(|i| (std::f32::consts::TAU * fc * i as f32 / sr as f32).sin()).collect();
            let mut p = vec![0.0; 32];
            fb.process(&x, &mut p);
            assert!((p[b] - 1.0).abs() < 0.05, "band {b} at {fc} Hz reads {}", p[b]);
            let db = |v: f32| 10.0 * v.log10();
            // Neighbours cross 3 dB down at the shared edge, so a sine at a
            // band's centre is about 10 dB down in the next band.
            assert!(db(p[b - 1]) > -14.0 && db(p[b - 1]) < -6.0, "neighbour {}", db(p[b - 1]));
            assert!(db(p[b + 2]) < -20.0, "two away {}", db(p[b + 2]));
        }
    }
}
