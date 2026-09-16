//! Core linear algebra, bounded floats, and verified vector/matrix operations.
//!
//! [`tpt-certus-math`] wraps the TPT math ecosystem ([`tpt_math_geometry`],
//! [`tpt_math_linalg`) and extends it with **bounded-float** types whose
//! ranges are tracked at the type level, enabling compile-time enforcement of
//! domain constraints that the [`tpt-telos`] ideal-layer contracts assume.
//!
//! # Two-layer role
//!
//! * **Ideal layer (QF_LRA):** contracts are written over exact rationals;
//!   `tpt-certus-math` provides the type scaffolding that lets generated code
//!   operate over bounded-representation f64 values whose deviations from the
//!   ideal are tracked.
//! * **Realization layer:** IEEE-754 rounding analysis over the bounded-float
//!   operations yields the auto-derived `ε` bound.
//!
//! # Status
//!
//! Phase 0 scaffold only — types and proof-certificate plumbing will be added
//! in Phase 1 alongside the `ray_intersects_aabb` proof of concept.
//!
//! [`tpt_math_geometry`]: https://docs.rs/tpt-math-geometry
//! [`tpt_math_linalg`]: https://docs.rs/tpt-math-linalg
//! [`tpt-telos`]: https://docs.rs/tpt-telos

#![no_std]

/// Placeholder bounded-float range marker.
///
/// In Phase 1 this will become a full range-tracked f64 wrapper whose
/// `Min`/`Max` bounds are verified at build time. The placeholder below
/// demonstrates that the crate compiles and the workspace lints are wired.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundedFloat {
    value: f64,
    min: f64,
    max: f64,
}

impl BoundedFloat {
    /// Wrap a raw `f64` with an explicit valid range `[min, max]`.
    ///
    /// Phase 1 will prove `min <= max` and `min <= value <= max` via a
    /// `tpt-telos` postcondition; Phase 0 asserts only at runtime.
    pub fn new(value: f64, min: f64, max: f64) -> Self {
        debug_assert!(min <= max, "min must be <= max");
        debug_assert!(
            value >= min && value <= max,
            "value {value} outside [{min}, {max}]"
        );
        BoundedFloat { value, min, max }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_float_invariants() {
        let bf = BoundedFloat::new(1.5, 0.0, 3.0);
        assert!(bf.value() >= bf.min());
        assert!(bf.value() <= bf.max());
    }

    #[test]
    fn bounded_float_round_trip() {
        let original = BoundedFloat::new(-2.5, -10.0, 10.0);
        let round_tripped = BoundedFloat::new(original.value(), original.min(), original.max());
        assert_eq!(original, round_tripped);
    }
}
