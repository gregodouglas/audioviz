//! A tiny seeded generator (splitmix64) for things that must replay the same
//! way for the same track: the Sigil's arrangement, Crystal's poses, the
//! effects director's rolls.

pub struct Rng(pub u64);

impl Rng {
    pub fn seeded(parts: &[u64]) -> Self {
        let mut s = 0x9E37_79B9_7F4A_7C15u64;
        for p in parts {
            s = (s ^ p).wrapping_mul(0xBF58_476D_1CE4_E5B9).rotate_left(23);
        }
        Self(s)
    }

    /// Uniform in [0, 1).
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }

    /// An index below `n`.
    pub fn below(&mut self, n: usize) -> usize {
        ((self.next() * n as f32) as usize).min(n.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let (mut a, mut b) = (Rng::seeded(&[7, 9]), Rng::seeded(&[7, 9]));
        for _ in 0..8 {
            assert_eq!(a.next(), b.next());
        }
        let mut c = Rng::seeded(&[7, 10]);
        assert_ne!(a.next(), c.next());
    }
}
