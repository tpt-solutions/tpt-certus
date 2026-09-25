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

    /// Squared length (`x² + y² + z²`), left-to-right associated so the
    /// realization-layer mirror can reproduce the exact operation order.
    #[inline]
    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Euclidean length, via [`tpt_certus_math::sqrt`] (the workspace-wide
    /// core-only square root, shared with the realization mirror).
    #[inline]
    pub fn length(self) -> f64 {
        tpt_certus_math::sqrt(self.length_squared())
    }

    /// Unit vector in the same direction.  Returns [`Vector3::ZERO`] for the
    /// zero vector rather than producing NaN (callers that need "direction must
    /// be non-zero" enforce it through the `ray_intersects_aabb` preconditions).
    #[inline]
    pub fn normalized(self) -> Self {
        let len = self.length();
        if len == 0.0 {
            Vector3::ZERO
        } else {
            self / len
        }
    }

    /// Whether all three components are finite.
    #[inline]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Component-wise minimum.
    #[inline]
    pub fn component_min(self, rhs: Self) -> Self {
        Vector3 {
            x: self.x.min(rhs.x),
            y: self.y.min(rhs.y),
            z: self.z.min(rhs.z),
        }
    }

    /// Component-wise maximum.
    #[inline]
    pub fn component_max(self, rhs: Self) -> Self {
        Vector3 {
            x: self.x.max(rhs.x),
            y: self.y.max(rhs.y),
            z: self.z.max(rhs.z),
        }
    }

    /// Component-wise negation (also available as `-v`).
    #[inline]
    pub fn neg(self) -> Self {
        Vector3 {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

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

    /// The smallest AABB containing both `self` and `other`.
    pub fn union(&self, other: &AABB) -> AABB {
        AABB {
            min: self.min.component_min(other.min),
            max: self.max.component_max(other.max),
        }
    }

    /// Whether `p` lies inside the box, **boundary inclusive**.
    ///
    /// The inclusive convention is the one the Section 6 / Appendix A contract
    /// uses for "origin exactly on a boundary face counts as inside".
    #[inline]
    pub fn contains(&self, p: Vector3) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }

    /// Whether the two boxes overlap.  Touching faces count as overlapping
    /// (consistent with the inclusive [`AABB::contains`] convention).
    #[inline]
    pub fn overlaps(&self, other: &AABB) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
            && self.min.z <= other.max.z
            && other.min.z <= self.max.z
    }

    /// The intersection of the two boxes, or `None` when they are disjoint.
    pub fn intersection(&self, other: &AABB) -> Option<AABB> {
        let min = self.min.component_max(other.min);
        let max = self.max.component_min(other.max);
        if min.x <= max.x && min.y <= max.y && min.z <= max.z {
            Some(AABB { min, max })
        } else {
            None
        }
    }

    /// The box grown by `amount` on every side.
    ///
    /// Debug-asserts a finite, non-negative `amount` (a negative amount could
    /// invert the box, which [`AABB::new`]'s invariant forbids).
    pub fn expand(&self, amount: f64) -> AABB {
        debug_assert!(
            amount >= 0.0 && amount.is_finite(),
            "expand amount must be finite and non-negative"
        );
        AABB {
            min: Vector3::new(
                self.min.x - amount,
                self.min.y - amount,
                self.min.z - amount,
            ),
            max: Vector3::new(
                self.max.x + amount,
                self.max.y + amount,
                self.max.z + amount,
            ),
        }
    }

    /// Surface area of the box.
    pub fn surface_area(&self) -> f64 {
        let e = self.max - self.min;
        2.0 * (e.x * e.y + e.y * e.z + e.z * e.x)
    }

    /// Volume of the box (zero for a degenerate box).
    pub fn volume(&self) -> f64 {
        let e = self.max - self.min;
        e.x * e.y * e.z
    }

    /// Whether every component of `min` and `max` is finite.
    #[inline]
    pub fn is_finite(&self) -> bool {
        self.min.is_finite() && self.max.is_finite()
    }

    /// The smallest AABB containing every point, or `None` for an empty slice.
    pub fn from_points(points: &[Vector3]) -> Option<AABB> {
        let mut iter = points.iter();
        let first = *iter.next()?;
        let mut b = AABB {
            min: first,
            max: first,
        };
        for p in iter {
            b.min = b.min.component_min(*p);
            b.max = b.max.component_max(*p);
        }
        Some(b)
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

    /// The parameter `t` at which this ray meets `plane`, i.e. the `t` for which
    /// `self.point_at(t)` lies on the plane.
    ///
    /// Returns `None` when the ray is parallel to the plane (`dir · normal == 0`).
    /// The operation order (`((plane.origin - origin) · normal) / (dir · normal)`)
    /// is mirrored operation-for-operation by
    /// `tpt_certus_proof::realization::ray_plane_t_realization`, so the derived
    /// `ε` applies to this exact artifact.
    pub fn plane_t(&self, plane: &Plane) -> Option<f64> {
        let denominator = self.dir.dot(plane.normal);
        if denominator == 0.0 {
            None
        } else {
            Some((plane.origin - self.origin).dot(plane.normal) / denominator)
        }
    }

    /// Evaluate the ray at parameter `t`: `origin + t * dir`.
    pub fn point_at(&self, t: f64) -> Vector3 {
        self.origin + self.dir * t
    }
}

impl core::ops::Neg for Vector3 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        self.neg()
    }
}

/// An oriented plane: the set of points `p` with `(p - origin) · normal == 0`.
///
/// `Plane::new` treats `origin` as a point on the plane and `normal` only as a
/// direction (it is not required to be unit length).  A zero normal does not
/// define a plane and is rejected in debug builds; [`Plane::signed_distance`]
/// divides by `|normal|`, which is why the real-arithmetic contract for that
/// function requires a non-zero normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    /// A point on the plane.
    pub origin: Vector3,
    /// The plane's normal direction (any non-zero length).
    pub normal: Vector3,
}

impl Plane {
    /// Construct a plane through `origin` with normal direction `normal`.
    ///
    /// Debug-asserts that `normal` is non-zero and both vectors finite.
    pub fn new(origin: Vector3, normal: Vector3) -> Self {
        debug_assert!(origin.is_finite(), "plane origin must be finite");
        debug_assert!(
            normal.is_finite() && normal.length_squared() > 0.0,
            "plane normal must be finite and non-zero"
        );
        Plane { origin, normal }
    }

    /// Signed distance from `p` to the plane: positive on the side the normal
    /// points to.
    ///
    /// The operation order (`((p - origin) · normal) / |normal|`, with `|·|`
    /// from [`Vector3::length_squared`] + [`tpt_certus_math::sqrt`]) is mirrored
    /// operation-for-operation by
    /// `tpt_certus_proof::realization::point_plane_distance_realization`, so the
    /// derived `ε` applies to this exact artifact.
    pub fn signed_distance(&self, p: Vector3) -> f64 {
        (p - self.origin).dot(self.normal) / self.normal.length()
    }
}

/// A sphere: every point within `radius` of `centre`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sphere {
    pub centre: Vector3,
    pub radius: f64,
}

impl Sphere {
    /// Construct a sphere.  Debug-asserts a finite, non-negative radius.
    pub fn new(centre: Vector3, radius: f64) -> Self {
        debug_assert!(radius >= 0.0 && radius.is_finite(), "invalid radius");
        debug_assert!(centre.is_finite(), "sphere centre must be finite");
        Sphere { centre, radius }
    }

    /// Whether `p` is inside or on the sphere.  Uses squared distances only,
    /// so no square root (and therefore no rounding beyond `+`/`*`) is involved.
    #[inline]
    pub fn contains(&self, p: Vector3) -> bool {
        (p - self.centre).length_squared() <= self.radius * self.radius
    }

    /// The axis-aligned bounding box of the sphere.
    pub fn aabb(&self) -> AABB {
        AABB {
            min: Vector3::new(
                self.centre.x - self.radius,
                self.centre.y - self.radius,
                self.centre.z - self.radius,
            ),
            max: Vector3::new(
                self.centre.x + self.radius,
                self.centre.y + self.radius,
                self.centre.z + self.radius,
            ),
        }
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

    #[test]
    fn vector3_length_and_normalize() {
        let v = Vector3::new(3.0, 4.0, 0.0);
        assert!((v.length() - 5.0).abs() < 1e-12);
        assert_eq!(v.length_squared(), 25.0);
        let n = v.normalized();
        assert!((n.length() - 1.0).abs() < 1e-12);
        assert_eq!(Vector3::ZERO.normalized(), Vector3::ZERO);
    }

    #[test]
    fn vector3_component_min_max_and_neg() {
        let a = Vector3::new(-1.0, 5.0, 2.0);
        let b = Vector3::new(3.0, 1.0, -2.0);
        assert_eq!(a.component_min(b), Vector3::new(-1.0, 1.0, -2.0));
        assert_eq!(a.component_max(b), Vector3::new(3.0, 5.0, 2.0));
        assert_eq!(-a, Vector3::new(1.0, -5.0, -2.0));
        assert!(!Vector3::new(f64::NAN, 0.0, 0.0).is_finite());
    }

    #[test]
    fn aabb_union_contains_and_overlap() {
        let a = AABB::new(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(0.0, 0.0, 0.0));
        let b = AABB::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 1.0, 1.0));
        // Touching faces: containment and overlap are both inclusive.
        assert!(a.contains(Vector3::new(0.0, 0.0, 0.0)));
        assert!(a.overlaps(&b));
        assert_eq!(a.intersection(&b).expect("touching boxes intersect").volume(), 0.0);

        let far = AABB::new(Vector3::new(5.0, 5.0, 5.0), Vector3::new(6.0, 6.0, 6.0));
        assert!(!a.overlaps(&far));
        assert!(a.intersection(&far).is_none());
        assert_eq!(
            a.union(&far).min,
            Vector3::new(-1.0, -1.0, -1.0),
            "union covers both boxes"
        );
        assert_eq!(a.union(&far).max, Vector3::new(6.0, 6.0, 6.0));
    }

    #[test]
    fn aabb_from_points_expand_and_measures() {
        let points = [
            Vector3::new(-1.0, -1.0, -1.0),
            Vector3::new(1.0, 1.0, 1.0),
            Vector3::new(0.0, 0.0, 0.5),
        ];
        let b = AABB::from_points(&points).expect("non-empty");
        assert_eq!(b, AABB::new(points[0], points[1]));
        assert_eq!(b.surface_area(), 24.0);
        assert_eq!(b.volume(), 8.0);
        assert_eq!(b.expand(1.0).half_extents(), Vector3::new(2.0, 2.0, 2.0));
        assert!(AABB::from_points(&[]).is_none());
    }

    #[test]
    fn plane_signed_distance_and_ray_plane_t() {
        let plane = Plane::new(Vector3::ZERO, Vector3::new(0.0, 0.0, 2.0));
        assert!((plane.signed_distance(Vector3::new(0.0, 0.0, 3.0)) - 3.0).abs() < 1e-12);
        assert!((plane.signed_distance(Vector3::new(0.0, 0.0, -3.0)) + 3.0).abs() < 1e-12);
        assert_eq!(plane.signed_distance(Vector3::ZERO), 0.0);

        let ray = Ray::new(Vector3::new(-5.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0));
        let x_plane = Plane::new(Vector3::ZERO, Vector3::new(1.0, 0.0, 0.0));
        assert!((ray.plane_t(&x_plane).expect("not parallel") - 5.0).abs() < 1e-12);

        // Parallel ray: `dir · normal == 0` exactly, so there is no `t`.
        let parallel = Ray::new(Vector3::new(0.0, 1.0, 0.0), Vector3::new(0.0, 0.0, 1.0));
        assert!(parallel.plane_t(&x_plane).is_none());
    }

    #[test]
    fn sphere_contains_and_aabb() {
        let s = Sphere::new(Vector3::new(1.0, 0.0, 0.0), 2.0);
        assert!(s.contains(Vector3::new(1.0, 2.0, 0.0)), "on the surface");
        assert!(s.contains(Vector3::new(1.0, 0.0, 0.0)));
        assert!(!s.contains(Vector3::new(1.0, 2.5, 0.0)));
        assert_eq!(s.aabb(), AABB::new(Vector3::new(-1.0, -2.0, -2.0), Vector3::new(3.0, 2.0, 2.0)));
    }
}
