//! Mono sample ring and windowed real FFTs at several resolutions.

use std::sync::Arc;

use realfft::{RealFftPlanner, RealToComplex, num_complex::Complex};

/// Fixed-capacity ring of mono samples. `copy_last` unrolls the newest `n`.
pub struct Ring {
    buf: Vec<f32>,
    pos: usize,
}

impl Ring {
    pub fn new(capacity: usize) -> Self {
        Self { buf: vec![0.0; capacity], pos: 0 }
    }

    #[inline]
    pub fn push(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos = (self.pos + 1) % self.buf.len();
    }

    /// Copy the most recent `out.len()` samples into `out`, oldest first.
    pub fn copy_last(&self, out: &mut [f32]) {
        let n = out.len().min(self.buf.len());
        let cap = self.buf.len();
        // Start index of the oldest requested sample.
        let start = (self.pos + cap - n) % cap;
        let first = (cap - start).min(n);
        out[..first].copy_from_slice(&self.buf[start..start + first]);
        if first < n {
            out[first..n].copy_from_slice(&self.buf[..n - first]);
        }
    }
}

/// One FFT resolution. After `compute`, `mag[k]` is the amplitude of bin `k`
/// normalized so a full-scale sine reads ~1.0, and `power[k] = mag[k]^2`.
pub struct Stft {
    pub n: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    window_gain: f32,
    input: Vec<f32>,
    output: Vec<Complex<f32>>,
    pub mag: Vec<f32>,
    pub power: Vec<f32>,
}

impl Stft {
    pub fn new(n: usize) -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(n);
        let window: Vec<f32> = (0..n)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())
            .collect();
        let wsum: f32 = window.iter().sum();
        let input = fft.make_input_vec();
        let output = fft.make_output_vec();
        let bins = n / 2 + 1;
        Self { n, fft, window, window_gain: 2.0 / wsum, input, output, mag: vec![0.0; bins], power: vec![0.0; bins] }
    }

    pub fn bins(&self) -> usize {
        self.mag.len()
    }

    pub fn bin_hz(&self, sample_rate: u32) -> f32 {
        sample_rate as f32 / self.n as f32
    }

    pub fn compute(&mut self, ring: &Ring) {
        ring.copy_last(&mut self.input);
        for (x, w) in self.input.iter_mut().zip(&self.window) {
            *x *= w;
        }
        if self.fft.process(&mut self.input, &mut self.output).is_err() {
            return;
        }
        let g = self.window_gain;
        for ((m, p), c) in self.mag.iter_mut().zip(self.power.iter_mut()).zip(&self.output) {
            let mag = (c.re * c.re + c.im * c.im).sqrt() * g;
            *m = mag;
            *p = mag * mag;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_unrolls_in_order() {
        let mut r = Ring::new(8);
        for i in 0..11 {
            r.push(i as f32);
        }
        let mut out = [0.0; 5];
        r.copy_last(&mut out);
        assert_eq!(out, [6.0, 7.0, 8.0, 9.0, 10.0]);
    }

    #[test]
    fn full_scale_sine_reads_one() {
        let n = 2048;
        let sr = 48_000.0;
        let mut ring = Ring::new(n);
        let bin = 100;
        let f = bin as f32 * sr / n as f32;
        for i in 0..n {
            ring.push((2.0 * std::f32::consts::PI * f * i as f32 / sr).sin());
        }
        let mut s = Stft::new(n);
        s.compute(&ring);
        assert!((s.mag[bin] - 1.0).abs() < 0.05, "mag {}", s.mag[bin]);
    }
}
