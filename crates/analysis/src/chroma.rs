//! 12-bin pitch-class profile from the long STFT.

use crate::follow;

pub struct Chroma {
    /// Pitch class for each bin (or 255 to skip).
    class: Vec<u8>,
    /// Per-bin weight: de-emphasize very low and very high bins.
    weight: Vec<f32>,
    /// This hop's profile, normalized so the max is 1, unsmoothed.
    pub raw: [f32; 12],
    /// Smoothed, normalized so the max is 1.
    pub values: [f32; 12],
    /// Slow version for key/color decisions (tau ~3 s).
    pub slow: [f32; 12],
    /// Dominant pitch class of `slow`, 0 = C.
    pub dominant: u8,
}

impl Chroma {
    pub fn new(bins: usize, bin_hz: f32) -> Self {
        let mut class = vec![255u8; bins];
        let mut weight = vec![0.0f32; bins];
        for k in 1..bins {
            let f = k as f32 * bin_hz;
            if !(55.0..=5000.0).contains(&f) {
                continue;
            }
            let midi = 69.0 + 12.0 * (f / 440.0).log2();
            let pc = (midi.round() as i32).rem_euclid(12) as u8;
            class[k] = pc;
            // Favor the melodic/harmonic register; taper highs where harmonics smear.
            weight[k] = if f < 2000.0 { 1.0 } else { (2000.0 / f).sqrt() };
        }
        Self { class, weight, raw: [0.0; 12], values: [0.0; 12], slow: [0.0; 12], dominant: 0 }
    }

    pub fn process(&mut self, mag: &[f32], dt: f32, silent: bool) {
        let mut acc = [0.0f32; 12];
        if !silent {
            for ((&c, &w), &m) in self.class.iter().zip(&self.weight).zip(mag) {
                if c != 255 {
                    acc[c as usize] += m * w;
                }
            }
        }
        let max = acc.iter().cloned().fold(0.0f32, f32::max);
        if max > 1e-6 {
            for a in acc.iter_mut() {
                *a /= max;
            }
        }
        self.raw = acc;
        let mut best = (0u8, -1.0f32);
        for i in 0..12 {
            self.values[i] = follow(self.values[i], acc[i], dt, 0.05, 0.4);
            self.slow[i] = follow(self.slow[i], acc[i], dt, 1.5, 3.0);
            if self.slow[i] > best.1 {
                best = (i as u8, self.slow[i]);
            }
        }
        self.dominant = best.0;
    }
}

/// Hue (0..1) for a pitch class via the circle of fifths, so harmonically
/// close keys get close colors.
pub fn pitch_class_hue(pc: u8) -> f32 {
    ((pc as u32 * 7) % 12) as f32 / 12.0
}
