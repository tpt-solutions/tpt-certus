#![allow(dead_code)] // each test binary uses a subset of these helpers
//! Shared test helpers for the mirror ↔ artifact cross-checks.

use tpt_certus_proof::realization::Interval;

/// splitmix64: a tiny, well-distributed, fully deterministic generator.  The
/// fixed seed makes every run reproduce the same inputs, with no RNG crate.
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.unit() * (hi - lo)
    }

    /// Uniform point inside an interval.
    pub fn in_interval(&mut self, iv: Interval) -> f64 {
        self.range(iv.lo, iv.hi)
    }

    /// A centred interval `[c - w, c + w]`.
    pub fn around(&mut self, c_lo: f64, c_hi: f64, w: f64) -> Interval {
        let c = self.range(c_lo, c_hi);
        Interval::new(c - w, c + w).expect("ordered interval")
    }
}
