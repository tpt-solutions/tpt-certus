//! Mirror ↔ artifact equivalence cross-checks (Phase 1.3).
//!
//! The realization layer derives an `ε` envelope from an operation-graph
//! *mirror* of `ray_intersects_aabb` (`tpt_certus_proof::realization`).  These
//! tests evaluate the shipped artifact and the mirror on the same concrete
//! inputs and check the two obligations the derived bound publishes:
//!
//! 1. **Decision agreement** — for a decisive domain, the artifact's hit/miss
//!    outcome matches the derived [`Decision`] for every input in the domain
//!    (no false negatives, and no false positives in a guaranteed-miss domain).
//! 2. **Envelope containment** — the artifact's `t_near` / `t_far` lie inside
//!    the envelopes the mirror derived, so the published `ε` (the envelope
//!    width) bounds the artifact's deviation from the mirror.
//!
//! Inputs are a deterministic grid over each declared domain (interval
//! endpoints and midpoints per axis), so the sweep is reproducible without an
//! RNG dependency.

use tpt_certus_geometry::{Ray, Vector3, AABB};
use tpt_certus_proof::realization::{
    slab_realization, Decision, Interval, InputDomain, Realization,
};
use tpt_certus_spatial::ray_aabb::ray_intersects_aabb;

/// Endpoints and midpoint of an interval, the sweep values for one axis.
fn samples(iv: Interval) -> [f64; 3] {
    let lo = iv.lo;
    let hi = iv.hi;
    [lo, 0.5 * (lo + hi), hi]
}

fn point3(v: [f64; 3]) -> Vector3 {
    Vector3::new(v[0], v[1], v[2])
}

/// Build the declared domain, derive the realization, and sweep the grid.
/// Returns the number of concrete inputs checked and the artifact hit count.
fn cross_check(domain: &InputDomain, box_: AABB) -> (Realization, usize, usize) {
    let real = slab_realization(domain).expect("domain is bounded");
    let mut checked = 0usize;
    let mut hits = 0usize;

    for ox in samples(domain.origin[0]) {
        for oy in samples(domain.origin[1]) {
            for oz in samples(domain.origin[2]) {
                for dx in samples(domain.direction[0]) {
                    for dy in samples(domain.direction[1]) {
                        for dz in samples(domain.direction[2]) {
                            for t_max in samples(domain.t_max) {
                                let ray = Ray::new(point3([ox, oy, oz]), point3([dx, dy, dz]));
                                let outcome = ray_intersects_aabb(ray, box_, t_max);
                                let artifact_hit = outcome.is_some();
                                checked += 1;
                                hits += usize::from(artifact_hit);

                                assert!(
                                    real.decision.agrees_with_artifact(artifact_hit),
                                    "decision {:?} disagrees with artifact outcome {} for \
                                     origin {:?}, dir {:?}, t_max {}",
                                    real.decision,
                                    artifact_hit,
                                    [ox, oy, oz],
                                    [dx, dy, dz],
                                    t_max
                                );

                                if let Some(hit) = outcome {
                                    assert!(
                                        real.t_near_interval.contains(hit.t_near),
                                        "artifact t_near {} outside mirror envelope {:?}",
                                        hit.t_near,
                                        real.t_near_interval
                                    );
                                    assert!(
                                        real.t_far_interval.contains(hit.t_far),
                                        "artifact t_far {} outside mirror envelope {:?}",
                                        hit.t_far,
                                        real.t_far_interval
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    (real, checked, hits)
}

fn unit_box() -> (Interval, Interval, AABB) {
    let lo = Interval::point(-1.0);
    let hi = Interval::point(1.0);
    let aabb = AABB::new(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0));
    (lo, hi, aabb)
}

#[test]
fn guaranteed_hit_domain_never_misses_in_artifact() {
    let (lo, hi, box_) = unit_box();
    // Origin inside the box, every direction component positive and bounded
    // away from zero, generous t_max: every ray must hit.
    let domain = InputDomain::new(
        [lo; 3],
        [hi; 3],
        [
            Interval::new(-0.5, 0.5).unwrap(),
            Interval::new(-0.5, 0.5).unwrap(),
            Interval::new(-0.5, 0.5).unwrap(),
        ],
        [
            Interval::new(0.5, 1.0).unwrap(),
            Interval::new(0.5, 1.0).unwrap(),
            Interval::new(0.5, 1.0).unwrap(),
        ],
        Interval::new(1000.0, 1000.0).unwrap(),
    )
    .expect("domain is bounded");

    let (real, checked, hits) = cross_check(&domain, box_);
    assert_eq!(real.decision, Decision::GuaranteedHit, "domain must be decisive");
    assert!(checked > 0);
    assert_eq!(hits, checked, "every input in a guaranteed-hit domain must hit");
}

#[test]
fn guaranteed_miss_domain_never_hits_in_artifact() {
    let (lo, hi, box_) = unit_box();
    // Origin well behind the box on -x, so the entry distance is at least 8
    // for every input; t_max is capped below that, so every ray must miss.  The
    // mirror decides a miss only via `t_near > t_max`, so the domain is chosen
    // to satisfy that rule.
    let domain = InputDomain::new(
        [lo; 3],
        [hi; 3],
        [
            Interval::new(-10.0, -9.0).unwrap(),
            Interval::new(-0.5, 0.5).unwrap(),
            Interval::new(-0.5, 0.5).unwrap(),
        ],
        [
            Interval::new(0.5, 1.0).unwrap(),
            Interval::new(0.5, 1.0).unwrap(),
            Interval::new(0.5, 1.0).unwrap(),
        ],
        Interval::new(1.0, 5.0).unwrap(),
    )
    .expect("domain is bounded");

    let (real, checked, hits) = cross_check(&domain, box_);
    assert_eq!(real.decision, Decision::GuaranteedMiss, "domain must be decisive");
    assert!(checked > 0);
    assert_eq!(hits, 0, "no input in a guaranteed-miss domain may hit");
}

#[test]
fn straddling_domain_is_indeterminate_and_claims_nothing() {
    let (lo, hi, box_) = unit_box();
    // Origin spans both inside and outside the box along x, so some rays hit
    // and some miss: the mirror must not claim a decision, and the artifact is
    // still checked for envelope containment.
    let domain = InputDomain::new(
        [lo; 3],
        [hi; 3],
        [
            Interval::new(-3.0, 0.5).unwrap(),
            Interval::new(-0.5, 0.5).unwrap(),
            Interval::new(-0.5, 0.5).unwrap(),
        ],
        [
            Interval::new(0.5, 1.0).unwrap(),
            Interval::new(0.5, 1.0).unwrap(),
            Interval::new(0.5, 1.0).unwrap(),
        ],
        Interval::new(10.0, 20.0).unwrap(),
    )
    .expect("domain is bounded");

    let (real, checked, hits) = cross_check(&domain, box_);
    assert_eq!(real.decision, Decision::Indeterminate);
    assert!(checked > 0);
    assert!(hits > 0 && hits < checked, "a straddling domain must mix outcomes");
}
