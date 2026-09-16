//! 3D geometric primitives: [`AABB`], [`Vector3`], [`Ray`], and related types.
//!
//! Builds on [`tpt_eng_geometry`] and [`tpt_eng_mesh`] to provide the domain
//! objects that [`tpt-certus-spatial`] operates over.  Each primitive carries
//! construction invariants (e.g. `AABB.min <= AABB.max`) that will be proven
//! by [`tpt-telos`] ideal-layer contracts once `.telos` sources are authored
//! (Phase 1).
//!
//! # Two-layer role
//!
//! In the ideal layer these are exact real-valued primitives; in the realization
//! layer their construction gates guarantee `min <= max` in f64 without gaps
//! at boundary faces, closing the boundary-case issues identified in
//! `spec.txt` Appendix A.
//!
//! # Status
//!
//! Phase 0 scaffold — the types below prove the crate compiles and the
//! dependency wiring is correct.  Full contract coverage comes in Phase 1.

#![no_std]

/// A 3-component f64 vector used throughout `tpt-certus`.
///
/// Phase 1 will prove norm-related properties (non-zero direction, bounded
/// magnitude) via `tpt-telos` contracts.  Phase 0 exposes only the minimal
/// construction and operator API.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vector3 {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Vector3 { x, y, z }
    }

    /// A zero vector.
    pub const ZERO: Self = Vector3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Dot product.
    #[inline]
    pub fn dot(self, rhs: Self) -> f64 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    /// Cross product.
    #[inline]
    pub fn cross(self, rhs: Self) -> Self {
        Vector3 {
            x: self.y * rhs.z - self.z * rhs.y,
            y: self.z * rhs.x - self.x * rhs.z,
            z: self.x * rhs.y - self.y * rhs.x,
        }
    }
}

impl core::ops::Add for Vector3 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Vector3 {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl core::ops::Sub for Vector3 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Vector3 {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl core::ops::Mul<f64> for Vector3 {
    type Output = Self;
    #[inline]
    fn mul(self, s: f64) -> Self {
        Vector3 {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
        }
    }
}

impl core::ops::Div<f64> for Vector3 {
    type Output = Self;
    #[inline]
    fn div(self, s: f64) -> Self {
        Vector3 {
            x: self.x / s,
            y: self.y / s,
            z: self.z / s,
        }
    }
}

/// Axis-aligned bounding box: `min <= max` on every component.
///
/// The invariant `min.x <= max.x && min.y <= max.y && min.z <= max.z` is
/// a construction invariant proven in the ideal layer by `tpt-telos` in
/// Phase 1.  Phase 0 asserts it at runtime only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AABB {
    pub min: Vector3,
    pub max: Vector3,
}

impl AABB {
    /// Construct a new AABB.  Panics (via `debug_assert!`) in debug mode if
    /// the `min <= max` invariant is violated.
    pub fn new(min: Vector3, max: Vector3) -> Self {
        debug_assert!(min.x <= max.x, "AABB min.x > max.x");
        debug_assert!(min.y <= max.y, "AABB min.y > max.y");
        debug_assert!(min.z <= max.z, "AABB min.z > max.z");
        AABB { min, max }
    }

    /// The centre of the box.
    pub fn centre(&self) -> Vector3 {
        Vector3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }

    /// Half-extents: `(max - min) / 2` on each axis.
    pub fn half_extents(&self) -> Vector3 {
        Vector3::new(
            (self.max.x - self.min.x) * 0.5,
            (self.max.y - self.min.y) * 0.5,
            (self.max.z - self.min.z) * 0.5,
        )
    }
}

/// A ray defined by an origin and a direction vector.
///
/// The direction is required to be non-zero on at least one axis — a
/// precondition enforced by the ideal-layer contract in Phase 1.  Phase 0
/// documents the requirement without proving it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vector3,
    pub dir: Vector3,
}

impl Ray {
    pub fn new(origin: Vector3, dir: Vector3) -> Self {
        Ray { origin, dir }
    }

    /// Evaluate the ray at parameter `t`: `origin + t * dir`.
    pub fn point_at(&self, t: f64) -> Vector3 {
        self.origin + self.dir * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector3_basic_ops() {
        let a = Vector3::new(1.0, 2.0, 3.0);
        let b = Vector3::new(4.0, 5.0, 6.0);
        assert_eq!(a.dot(b), 32.0);
        assert_eq!(a.cross(b), Vector3::new(-3.0, 6.0, -3.0),);
    }

    #[test]
    fn aabb_centre_and_half_extents() {
        let aabb = AABB::new(Vector3::new(-1.0, -2.0, -3.0), Vector3::new(1.0, 2.0, 3.0));
        assert_eq!(aabb.centre(), Vector3::ZERO);
        assert_eq!(aabb.half_extents(), Vector3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn ray_point_at() {
        let ray = Ray::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 0.0));
        let p = ray.point_at(3.0);
        assert_eq!(p, Vector3::new(3.0, 3.0, 0.0));
    }
}
