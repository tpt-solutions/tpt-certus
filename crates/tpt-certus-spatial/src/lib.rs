//! Core spatial engine: ray casting, spatial partitioning, and collision.
//!
//! [`tpt-certus-spatial`] is the central crate of the suite.  It houses the
//! **verified spatial algorithms** — ray-AABB intersection, BVH construction,
//! octree traversal, broad/narrow-phase collision — whose proof obligations
//! are discharged by [`tpt-telos`] at build time.
//!
//! # Architecture
//!
//! Verified functions are authored as `.telos` sources in the [`telos/`](../telos)
//! directory of this crate and compiled into `src/generated/` by `tpt-telos`.
//! Each generated function carries both an ideal-layer (QF_LRA) and a
//! realization-layer (f64 + auto-derived `ε`) contract; the build fails if
//! either proof obligation is not discharged.
//!
//! # Two-layer role
//!
//! * **Ideal layer:** `tpt-telos` proves that the slab method (and later BVH
//!   traversal, collision detection) satisfies the specification over exact
//!   rationals — no false negatives, correct boundary handling.
//! * **Realization layer:** auto-derived `ε` from interval arithmetic over the
//!   f64 operation graph gives a concrete numeric bound published per-build in
//!   the Proof Certificate.
//!
//! # Status
//!
//! Phase 0 scaffold only — no verified algorithms are present yet.  Phase 1
//! adds `ray_intersects_aabb` as the template for all later work.
//!
//! [`tpt-telos`]: https://docs.rs/tpt-telos

#![no_std]

extern crate alloc;

/// Result of a ray-AABB intersection.
///
/// `t_near` is the distance to the entry point; `t_far` is the distance to
/// the exit point.  `t_far` is intentionally **not** clipped to `t_max` — an
/// entry within the bounded domain constitutes a valid hit regardless of exit
/// distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intersection {
    /// Distance to the entry point (always `>= 0.0` when returned from a
    /// verified function).
    pub t_near: f64,
    /// Distance to the exit point.
    pub t_far: f64,
}

/// Placeholder for the Phase 1 verified `ray_intersects_aabb`.
///
/// Phase 0 provides a reference (non-verified) implementation that matches the
/// algorithm in `spec.txt` Section 6.  Phase 1 will replace this with the
/// `tpt-telos`-generated, two-layer-contract-backed version.
pub mod ray_aabb {
    use crate::Intersection;
    use tpt_certus_geometry::{Ray, AABB};

    /// Ray-AABB intersection via the slab method.
    ///
    /// # Preconditions (proven ideal-layer in Phase 1)
    /// * `aabb.min <= aabb.max` on all three axes
    /// * `t_max > 0.0`
    /// * ray direction is non-zero on at least one axis
    ///
    /// # Scope note
    /// NaN/finiteness of inputs is out of scope — malformed inputs are rejected
    /// upstream by `tpt-certus-geometry` construction invariants.
    pub fn ray_intersects_aabb(ray: Ray, aabb: AABB, t_max: f64) -> Option<Intersection> {
        let mut t_min = f64::NEG_INFINITY;
        let mut t_max_slab = f64::INFINITY;

        // X-axis slab
        if ray.dir.x != 0.0 {
            let tx1 = (aabb.min.x - ray.origin.x) / ray.dir.x;
            let tx2 = (aabb.max.x - ray.origin.x) / ray.dir.x;
            t_min = t_min.max(tx1.min(tx2));
            t_max_slab = t_max_slab.min(tx1.max(tx2));
        } else if ray.origin.x < aabb.min.x || ray.origin.x > aabb.max.x {
            return None;
        }

        // Y-axis slab
        if ray.dir.y != 0.0 {
            let ty1 = (aabb.min.y - ray.origin.y) / ray.dir.y;
            let ty2 = (aabb.max.y - ray.origin.y) / ray.dir.y;
            t_min = t_min.max(ty1.min(ty2));
            t_max_slab = t_max_slab.min(ty1.max(ty2));
        } else if ray.origin.y < aabb.min.y || ray.origin.y > aabb.max.y {
            return None;
        }

        // Z-axis slab
        if ray.dir.z != 0.0 {
            let tz1 = (aabb.min.z - ray.origin.z) / ray.dir.z;
            let tz2 = (aabb.max.z - ray.origin.z) / ray.dir.z;
            t_min = t_min.max(tz1.min(tz2));
            t_max_slab = t_max_slab.min(tz1.max(tz2));
        } else if ray.origin.z < aabb.min.z || ray.origin.z > aabb.max.z {
            return None;
        }

        if t_max_slab >= t_min && t_max_slab >= 0.0 {
            let t_near = t_min.max(0.0);
            if t_near <= t_max {
                Some(Intersection {
                    t_near,
                    t_far: t_max_slab,
                })
            } else {
                None
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ray_aabb::ray_intersects_aabb;
    use tpt_certus_geometry::{Ray, Vector3, AABB};

    fn unit_box() -> AABB {
        AABB::new(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0))
    }

    #[test]
    fn hit_along_x_axis() {
        let ray = Ray::new(Vector3::new(-5.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0));
        let hit = ray_intersects_aabb(ray, unit_box(), 100.0).unwrap();
        assert!((hit.t_near - 4.0).abs() < 1e-12);
    }

    #[test]
    fn miss_outside_box() {
        let ray = Ray::new(Vector3::new(-5.0, 5.0, 0.0), Vector3::new(1.0, 0.0, 0.0));
        assert!(ray_intersects_aabb(ray, unit_box(), 100.0).is_none());
    }

    #[test]
    fn origin_inside_box() {
        let ray = Ray::new(Vector3::new(0.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0));
        let hit = ray_intersects_aabb(ray, unit_box(), 100.0).unwrap();
        assert_eq!(hit.t_near, 0.0);
    }

    #[test]
    fn origin_on_boundary_face() {
        let ray = Ray::new(Vector3::new(-1.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0));
        let hit = ray_intersects_aabb(ray, unit_box(), 100.0).unwrap();
        assert_eq!(hit.t_near, 0.0);
    }

    #[test]
    fn parallel_and_outside() {
        let ray = Ray::new(Vector3::new(-5.0, 2.0, 0.0), Vector3::new(1.0, 0.0, 0.0));
        assert!(ray_intersects_aabb(ray, unit_box(), 100.0).is_none());
    }

    #[test]
    fn ray_behind_box_no_hit() {
        let ray = Ray::new(Vector3::new(5.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0));
        assert!(ray_intersects_aabb(ray, unit_box(), 100.0).is_none());
    }

    #[test]
    fn t_max_clips_entry() {
        let ray = Ray::new(Vector3::new(-5.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0));
        // t_near = 4.0 but t_max = 2.0 < 4.0 → miss
        assert!(ray_intersects_aabb(ray, unit_box(), 2.0).is_none());
    }
}
