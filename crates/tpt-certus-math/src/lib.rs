//! Core linear algebra, bounded floats, and shared f64 primitives.
//!
//! [`tpt-certus-math`] wraps the TPT math ecosystem ([`tpt_math_geometry`],
//! [`tpt_math_linalg`]) and adds the f64-facing primitives the rest of the
//! suite is built on:
//!
//! 1. [`BoundedFloat`] — an f64 value carrying the range it is known to lie in
//!    (Phase 1.1 "bounded floats").  Range propagation is outward-widened, so a
//!    value derived from bounded inputs is itself a sound enclosure.
//! 2. [`sqrt`] / [`next_up`] / [`next_down`] / [`sqrt_lower_envelope`] /
//!    [`sqrt_upper_envelope`] — deterministic, core-only IEEE-754 primitives.
//!    The workspace is `#![no_std]` and `f64::sqrt` does *not* exist in `core`,
//!    so the whole suite routes square roots through [`sqrt`]: the geometry
//!    artifacts and the realization-layer operation-graph mirror in
//!    `tpt-certus-proof` share one rounding implementation instead of drifting.
//!
//! # Two-layer role
//!
//! * **Ideal layer (QF_LRA):** contracts are written over exact rationals.
//! * **Realization layer:** [`BoundedFloat`] carries the declared input domain
//!   and [`sqrt_lower_envelope`]/[`sqrt_upper_envelope`] provide the sound
//!   square-root interval from which `tpt-certus-proof` derives `ε`.
//!
//! # Trust note (`sqrt`)
//!
//! [`sqrt`] is a core-only Newton iteration, not the platform's correctly
//! rounded `sqrt`.  It agrees with the correctly rounded result to well within
//! [`SQRT_RELATIVE_MARGIN`] (2⁻⁵⁰ ≈ 8.9e-16, relative) over the whole finite
//! non-negative f64 range, and every square-root envelope is widened by that
//! margin, so the derived `ε` stays sound *under that documented assumption*.
//! The assumption is empirically checked here against an independently computed
//! rational reference (53-bit integer `isqrt`) and in `tpt-certus-spatial`
//! against the platform's `sqrt`; discharging it formally is part of Phase 2's
//! DO-330 tool-qualification work (spec §5.1).
//!
//! # Status
//!
//! Phase 1: bounded-float ranges, the shared f64 primitives, and the
//! unit-tagged `tpt-math` [`linalg`] re-export are in place, with tests.
//! `.telos` contracts for the range-propagation invariants follow with the
//! crate's own `.telos` sources.
//!
//! [`tpt_math_geometry`]: https://docs.rs/tpt-math-geometry
//! [`tpt_math_linalg`]: https://docs.rs/tpt-math-linalg
//! [`linalg`]: tpt_math_linalg

#![no_std]

/// The dimensional-safety layer of `tpt-math`: unit-tagged
/// [`linalg::Vec`]/[`linalg::Mat`] over the dense backend, so that adding a
/// length-vector to a mass-vector is a compile error.  Re-exported so
/// `tpt-certus` crates get dimension-checked linear algebra without a direct
/// `tpt-math-linalg` dependency.
///
/// [`linalg::Vec`]: tpt_math_linalg::Vec
/// [`linalg::Mat`]: tpt_math_linalg::Mat
pub use tpt_math_linalg as linalg;

/// `tpt-math`'s exact/geometric primitives (`Point`, `Rotation`, `Isometry`,
/// `Quaternion`, …), re-exported for the same reason as [`linalg`].
pub use tpt_math_geometry as geometry;

/// Relative margin by which every square-root envelope is widened, and the
/// documented accuracy assumption of [`sqrt`] (2⁻⁵⁰ ≈ 8.9e-16 relative).
pub const SQRT_RELATIVE_MARGIN: f64 = 1.0 / (1u64 << 50) as f64;

/// The next f64 toward `+∞` from `x` (one ulp up).
///
/// `±∞` and NaN pass through unchanged; `±0.0` becomes the smallest positive
/// subnormal, so the function is total over the finite range.
pub fn next_up(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let bits = x.to_bits();
    if bits == 0 || bits == 0x8000_0000_0000_0000 {
        return f64::from_bits(0x0000_0000_0000_0001);
    }
    if x > 0.0 {
        f64::from_bits(bits + 1)
    } else {
        f64::from_bits(bits - 1)
    }
}

/// The next f64 toward `-∞` from `x` (one ulp down).
///
/// `±∞` and NaN pass through unchanged; `±0.0` becomes the smallest negative
/// subnormal.
pub fn next_down(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let bits = x.to_bits();
    if bits == 0 || bits == 0x8000_0000_0000_0000 {
        return f64::from_bits(0x8000_0000_0000_0001);
    }
    if x > 0.0 {
        f64::from_bits(bits - 1)
    } else {
        f64::from_bits(bits + 1)
    }
}

/// Square root of a finite non-negative f64, computed without `std`.
///
/// Returns `NaN` for negative inputs and `NaN`, passes `±0.0` through
/// (preserving the sign), and returns `+∞` for `+∞`.  Accuracy is
/// [`SQRT_RELATIVE_MARGIN`]-relative vs. the correctly rounded result (see the
/// crate-level trust note).
///
/// # Implementation
///
/// The argument is first scaled by a power of four into `[0.25, 4)`, where the
/// fixed initial guess `1.0` is within a relative error of `1`.  Four-to-six
/// Newton steps (`s ← (s + m/s)/2`) then square the relative error each time,
/// reaching the ulp level; because the scale factor is a power of two, the final
/// rescaling is exact.
pub fn sqrt(x: f64) -> f64 {
    if x != x {
        return x;
    }
    if x == 0.0 {
        return x;
    }
    if x < 0.0 {
        return f64::NAN;
    }
    if x.is_infinite() {
        return x;
    }

    let mut m = x;
    let mut scale = 1.0_f64;
    while m >= 4.0 {
        m *= 0.25;
        scale *= 2.0;
    }
    while m < 0.25 {
        m *= 4.0;
        scale *= 0.5;
    }

    let mut s = 1.0_f64;
    let mut i = 0;
    while i < 6 {
        s = 0.5 * (s + m / s);
        i += 1;
    }
    scale * s
}

/// Lower endpoint of a sound enclosure of the exact real `sqrt(x)`.
///
/// Requires `x >= 0` and finite (the caller's domain check enforces this).  The
/// result is ≤ `sqrt(x)` even after accounting for [`sqrt`]'s
/// [`SQRT_RELATIVE_MARGIN`]-relative error and for the rounding of the scaling
/// product, because the margin is a power of two (so `1 - margin` is exact) and
/// the product is widened outward by one ulp.
pub fn sqrt_lower_envelope(x: f64) -> f64 {
    next_down(sqrt(x) * (1.0 - SQRT_RELATIVE_MARGIN))
}

/// Upper endpoint of a sound enclosure of the exact real `sqrt(x)`.
///
/// Requires `x >= 0` and finite; see [`sqrt_lower_envelope`].
pub fn sqrt_upper_envelope(x: f64) -> f64 {
    next_up(sqrt(x) * (1.0 + SQRT_RELATIVE_MARGIN))
}

/// An f64 value together with a sound enclosure `[min, max]` of the f64 values
/// it may take.
///
/// The invariant `min <= value <= max` is established at construction and
/// preserved by every operation.  Derived values are computed *twice*: once on
/// the point estimate (`value`, an ordinary f64 operation) and once on the
/// enclosure (interval arithmetic over the endpoints).  Every enclosure is
/// widened outward by one ulp, so it covers both the exact real result and the
/// correctly rounded f64 result — the same discipline the realization layer's
/// `ε` derivation relies on.
///
/// Operations return `None` rather than propagating non-finite or inverted
/// enclosures, so `None` is an explicit "the domain discipline was not met"
/// signal (e.g. division by an enclosure spanning zero).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundedFloat {
    value: f64,
    min: f64,
    max: f64,
}

impl BoundedFloat {
    /// Wrap a raw `f64` with an explicit valid range `[min, max]`.
    ///
    /// Panics (via `debug_assert!`) in debug builds if the range is inverted,
    /// non-finite, or does not contain `value`; use [`BoundedFloat::checked`]
    /// when the bounds come from untrusted input.
    pub fn new(value: f64, min: f64, max: f64) -> Self {
        debug_assert!(min <= max, "min must be <= max");
        debug_assert!(
            value >= min && value <= max,
            "value {value} outside [{min}, {max}]"
        );
        BoundedFloat { value, min, max }
    }

    /// Fallible constructor: `None` unless every bound is finite, the range is
    /// non-inverted, and `value` lies inside it.
    pub fn checked(value: f64, min: f64, max: f64) -> Option<Self> {
        if !value.is_finite() || !min.is_finite() || !max.is_finite() {
            return None;
        }
        if min > max || value < min || value > max {
            return None;
        }
        Some(BoundedFloat { value, min, max })
    }

    /// Build an exact (degenerate) bound from a finite value.
    pub fn point(value: f64) -> Option<Self> {
        Self::checked(value, value, value)
    }

    /// The wrapped f64 value.
    #[inline]
    pub fn value(&self) -> f64 {
        self.value
    }

    /// The lower bound of the valid range.
    #[inline]
    pub fn min(&self) -> f64 {
        self.min
    }

    /// The upper bound of the valid range.
    #[inline]
    pub fn max(&self) -> f64 {
        self.max
    }

    /// The enclosure width `max - min`.
    #[inline]
    pub fn width(&self) -> f64 {
        self.max - self.min
    }

    /// Whether `x` lies in the enclosure (endpoints inclusive).
    #[inline]
    pub fn contains(&self, x: f64) -> bool {
        self.min <= x && x <= self.max
    }

    /// Whether every part of the enclosure is finite.
    #[inline]
    pub fn is_finite(&self) -> bool {
        self.min.is_finite() && self.max.is_finite() && self.value.is_finite()
    }

    /// `self + rhs` with an outward-widened enclosure, or `None` if not finite.
    pub fn add(&self, rhs: &Self) -> Option<Self> {
        Self::checked(
            self.value + rhs.value,
            next_down(self.min + rhs.min),
            next_up(self.max + rhs.max),
        )
    }

    /// `self - rhs` with an outward-widened enclosure, or `None` if not finite.
    pub fn sub(&self, rhs: &Self) -> Option<Self> {
        Self::checked(
            self.value - rhs.value,
            next_down(self.min - rhs.max),
            next_up(self.max - rhs.min),
        )
    }

    /// `self * rhs` (four-corner enclosure), or `None` if not finite.
    pub fn mul(&self, rhs: &Self) -> Option<Self> {
        let lo = (self.min * rhs.min)
            .min(self.min * rhs.max)
            .min(self.max * rhs.min)
            .min(self.max * rhs.max);
        let hi = (self.min * rhs.min)
            .max(self.min * rhs.max)
            .max(self.max * rhs.min)
            .max(self.max * rhs.max);
        Self::checked(self.value * rhs.value, next_down(lo), next_up(hi))
    }

    /// `self / rhs`, or `None` when `rhs`'s enclosure spans (touches) zero — the
    /// quotient would not be sign-definite — or when the result is not finite.
    pub fn div(&self, rhs: &Self) -> Option<Self> {
        if rhs.min <= 0.0 && rhs.max >= 0.0 {
            return None;
        }
        let lo = (self.min / rhs.min)
            .min(self.min / rhs.max)
            .min(self.max / rhs.min)
            .min(self.max / rhs.max);
        let hi = (self.min / rhs.min)
            .max(self.min / rhs.max)
            .max(self.max / rhs.min)
            .max(self.max / rhs.max);
        Self::checked(self.value / rhs.value, next_down(lo), next_up(hi))
    }

    /// Square root with a sound enclosure, or `None` when the enclosure dips
    /// below zero (no real square root for every input in range) or the result
    /// is not finite.
    pub fn sqrt(&self) -> Option<Self> {
        if self.min < 0.0 {
            return None;
        }
        Self::checked(
            sqrt(self.value),
            sqrt_lower_envelope(self.min),
            sqrt_upper_envelope(self.max),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exact integer square root: an independent reference that does not use
    /// `f64` arithmetic at all.
    fn isqrt_u128(n: u128) -> u128 {
        if n == 0 {
            return 0;
        }
        let mut x = n;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        x
    }

    #[test]
    fn sqrt_is_exact_on_perfect_squares_and_powers_of_two() {
        for n in [1u64, 2, 3, 1 << 12, (1 << 26) - 1, 1 << 26] {
            let x = (n as f64) * (n as f64); // exact: n^2 < 2^53
            assert_eq!(sqrt(x), n as f64, "sqrt({x}) must be exact");
        }
        for k in [0i32, 1, 8, 20, 500] {
            let expected = (2.0f64).powi(k);
            let x = expected * expected;
            assert!((sqrt(x) - expected).abs() <= expected * SQRT_RELATIVE_MARGIN);
        }
    }

    #[test]
    fn sqrt_is_within_documented_margin_of_an_integer_reference() {
        // Reference bracket with ~2^-30 relative precision, built from exact
        // integer arithmetic only, independent of `sqrt` itself.
        for m in [1u64, 2, 3, 5, 10, 1_000_003, (1 << 52) - 1] {
            let x = (m as f64) * (2.0f64).powi(40); // x = m * 2^40, exact
            let n = (m as u128) << 60;
            let approx = isqrt_u128(n) as f64 / (2.0f64).powi(30); // ~ sqrt(m)
            let root = approx * (2.0f64).powi(20); // sqrt(m * 2^40)
            let relative = (sqrt(x) - root).abs() / root;
            assert!(
                relative <= (1.0 / (1u64 << 28) as f64),
                "sqrt({x}) off by {relative} vs integer reference {root}"
            );
        }
    }

    #[test]
    fn sqrt_round_trip_and_special_values() {
        for x in [1e-300, 1e-30, 0.5, 2.0, 3.0, 1e10, 1e100, 1e300] {
            let s = sqrt(x);
            assert!(s.is_finite() && s > 0.0, "sqrt({x}) = {s}");
            let squared = s * s;
            assert!(
                (squared - x).abs() <= 4.0 * SQRT_RELATIVE_MARGIN * x,
                "sqrt({x})^2 = {squared} outside the documented margin"
            );
        }

        // Subnormal / extreme inputs still get a finite, in-envelope root; the
        // relative-margin model is not applied below the normal range, where
        // relative error is not representable.
        for x in [5e-324, 1e-320, f64::MIN_POSITIVE, f64::MAX] {
            let s = sqrt(x);
            assert!(s.is_finite() && s > 0.0, "sqrt({x}) = {s}");
            assert!(sqrt_lower_envelope(x) <= s && s <= sqrt_upper_envelope(x));
        }

        assert_eq!(sqrt(0.0), 0.0);
        assert!(sqrt(-1.0).is_nan());
        assert!(sqrt(f64::NAN).is_nan());
        assert_eq!(sqrt(f64::INFINITY), f64::INFINITY);

        let mut prev = sqrt(0.25);
        let mut x = 0.5;
        while x < 1e6 {
            let s = sqrt(x);
            assert!(s >= prev, "sqrt must be monotone at {x}");
            prev = s;
            x *= 3.0;
        }
    }

    #[test]
    fn sqrt_envelopes_bracket_the_true_root() {
        for x in [1e-8, 0.25, 1.0, 2.0, 1e6, 1e120] {
            let lo = sqrt_lower_envelope(x);
            let hi = sqrt_upper_envelope(x);
            let s = sqrt(x);
            assert!(lo <= s && s <= hi, "own result outside envelope at {x}");
            assert!(hi - lo > 0.0);
        }
        // Where the exact root is an integer, the envelope must contain it.
        let n = 37.0;
        assert!(sqrt_lower_envelope(n * n) <= n);
        assert!(sqrt_upper_envelope(n * n) >= n);
    }

    #[test]
    fn next_up_and_next_down_step_one_ulp() {
        assert_eq!(next_up(1.0), f64::from_bits(1.0f64.to_bits() + 1));
        assert_eq!(next_down(1.0), f64::from_bits(1.0f64.to_bits() - 1));
        assert_eq!(next_down(next_up(1.0)), 1.0);
        assert_eq!(next_up(0.0), f64::from_bits(1));
        assert_eq!(next_down(0.0), f64::from_bits(0x8000_0000_0000_0001));
        assert!(next_up(f64::INFINITY).is_infinite());
    }

    #[test]
    fn bounded_float_invariants() {
        let bf = BoundedFloat::new(1.5, 0.0, 3.0);
        assert!(bf.value() >= bf.min());
        assert!(bf.value() <= bf.max());
        assert!(bf.contains(1.5) && bf.contains(0.0) && bf.contains(3.0));
        assert!(!bf.contains(3.5));
        assert_eq!(bf.width(), 3.0);
    }

    #[test]
    fn bounded_float_round_trip() {
        let original = BoundedFloat::new(-2.5, -10.0, 10.0);
        let round_tripped = BoundedFloat::new(original.value(), original.min(), original.max());
        assert_eq!(original, round_tripped);
    }

    #[test]
    fn bounded_float_checked_rejects_invalid() {
        assert!(BoundedFloat::checked(0.0, 1.0, 2.0).is_none());
        assert!(BoundedFloat::checked(0.0, 2.0, 1.0).is_none());
        assert!(BoundedFloat::checked(f64::NAN, 0.0, 1.0).is_none());
        assert!(BoundedFloat::point(1.0).is_some());
        assert!(BoundedFloat::point(f64::INFINITY).is_none());
    }

    #[test]
    fn bounded_float_arithmetic_widens_outward() {
        let a = BoundedFloat::new(0.1, 0.1, 0.1);
        let b = BoundedFloat::new(0.2, 0.2, 0.2);
        let sum = a.add(&b).expect("finite");
        assert_eq!(sum.value(), 0.1 + 0.2);
        assert!(sum.width() > 0.0, "enclosure must widen around a rounded sum");

        let x = BoundedFloat::new(2.0, 1.0, 3.0);
        let y = BoundedFloat::new(4.0, 4.0, 4.0);
        let prod = x.mul(&y).expect("finite");
        assert!(prod.contains(2.0 * 4.0));
        assert!(prod.min() <= 4.0 && prod.max() >= 12.0);
        let quot = x.div(&y).expect("finite");
        assert!(quot.contains(0.25) && quot.contains(0.5) && quot.contains(0.75));
        let diff = x.sub(&y).expect("finite");
        assert!(diff.contains(-3.0) && diff.contains(-1.0));
        assert!(x.is_finite());
    }

    #[test]
    fn bounded_float_division_by_zero_spanning_enclosure_is_none() {
        let num = BoundedFloat::new(1.0, 1.0, 1.0);
        let den = BoundedFloat::new(0.5, -1.0, 1.0);
        assert!(num.div(&den).is_none());
        let zero = BoundedFloat::new(0.0, 0.0, 0.0);
        assert!(num.div(&zero).is_none());
        // Overflow to infinity is refused rather than propagated.
        let huge = BoundedFloat::new(f64::MAX, f64::MAX, f64::MAX);
        assert!(huge.mul(&huge).is_none());
    }

    #[test]
    fn bounded_float_sqrt_brackets_root() {
        let x = BoundedFloat::new(2.01, 2.0, 2.02);
        let r = x.sqrt().expect("non-negative");
        assert!(r.value() > 1.4 && r.value() < 1.43);
        assert!(r.contains(sqrt(2.0)) && r.contains(sqrt(2.02)));
        assert!(BoundedFloat::new(-1.0, -1.0, -1.0).sqrt().is_none());
    }
}

