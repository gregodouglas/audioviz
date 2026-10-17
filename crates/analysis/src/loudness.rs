//! ITU-R BS.1770 K-weighted loudness, momentary (400 ms) and short-term (3 s),
//! approximated with one-pole integrators instead of sliding windows.

use crate::follow;

#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    const fn new(b0: f32, b1: f32, b2: f32, a1: f32, a2: f32) -> Self {
        Self { b0, b1, b2, a1, a2, z1: 0.0, z2: 0.0 }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        // Transposed direct form II.
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// K-weighting pre-filter coefficients for 48 kHz (BS.1770-4 Table 1 and 2).
fn k_weighting_48k() -> [Biquad; 2] {
    [
        Biquad::new(1.535_124_86, -2.691_696_19, 1.198_392_81, -1.690_659_29, 0.732_480_77),
        Biquad::new(1.0, -2.0, 1.0, -1.990_047_45, 0.990_072_25),
    ]
}

pub struct Loudness {
    filters: [[Biquad; 2]; 2],
    acc: [f64; 2],
    count: u32,
    ms_m: [f32; 2],
    ms_s: [f32; 2],
    /// Momentary loudness, LUFS (floor -70).
    pub lufs_m: f32,
    /// Short-term loudness, LUFS.
    pub lufs_s: f32,
}

impl Loudness {
    pub fn new(sample_rate: u32) -> Self {
        if sample_rate != 48_000 {
            tracing_lite::warn("K-weighting coefficients are for 48 kHz; loudness will be approximate");
        }
        let f = k_weighting_48k();
        Self { filters: [f, f], acc: [0.0; 2], count: 0, ms_m: [0.0; 2], ms_s: [0.0; 2], lufs_m: -70.0, lufs_s: -70.0 }
    }

    #[inline]
    pub fn push(&mut self, l: f32, r: f32) {
        let yl = self.filters[0][0].process(l);
        let yl = self.filters[0][1].process(yl);
        let yr = self.filters[1][0].process(r);
        let yr = self.filters[1][1].process(yr);
        self.acc[0] += (yl * yl) as f64;
        self.acc[1] += (yr * yr) as f64;
        self.count += 1;
    }

    /// Call once per hop with the hop duration.
    pub fn hop(&mut self, dt: f32) {
        if self.count == 0 {
            return;
        }
        let n = self.count as f64;
        for c in 0..2 {
            let ms = (self.acc[c] / n) as f32;
            self.ms_m[c] = follow(self.ms_m[c], ms, dt, 0.4, 0.4);
            self.ms_s[c] = follow(self.ms_s[c], ms, dt, 3.0, 3.0);
            self.acc[c] = 0.0;
        }
        self.count = 0;
        self.lufs_m = to_lufs(self.ms_m[0] + self.ms_m[1]);
        self.lufs_s = to_lufs(self.ms_s[0] + self.ms_s[1]);
    }
}

fn to_lufs(sum_ms: f32) -> f32 {
    if sum_ms <= 1e-9 {
        return -70.0;
    }
    (-0.691 + 10.0 * sum_ms.log10()).max(-70.0)
}

// Tiny shim so this crate stays dependency-free; the app has real tracing.
mod tracing_lite {
    pub fn warn(msg: &str) {
        eprintln!("[audioviz-analysis] warn: {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_khz_sine_at_minus_20_dbfs_rms_per_channel() {
        // Identical sine in both channels, RMS -20 dBFS each. K-weighting has
        // +0.691 dB gain at 1 kHz (the -0.691 offset cancels it), so:
        // 10*log10(2 * 0.01) = -17.0 LUFS. (BS.1770: 0 dBFS mono sine = -3.01.)
        let mut lo = Loudness::new(48_000);
        let amp = 0.1_f32 * std::f32::consts::SQRT_2; // RMS 0.1
        for i in 0..(48_000 * 16) {
            let s = amp * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48_000.0).sin();
            lo.push(s, s);
            if i % 512 == 511 {
                lo.hop(512.0 / 48_000.0);
            }
        }
        assert!((lo.lufs_s - (-17.0)).abs() < 0.3, "lufs_s {}", lo.lufs_s);
        assert!((lo.lufs_m - (-17.0)).abs() < 0.3, "lufs_m {}", lo.lufs_m);
    }
}
