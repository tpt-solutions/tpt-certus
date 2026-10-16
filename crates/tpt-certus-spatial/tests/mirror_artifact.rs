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
//! Three input sources are used:
//!
//! * a deterministic grid over each declared domain (interval endpoints and
//!   midpoints per axis);
//! * eight named reference scenarios taken from the Section 6 contract
//!   (inside, on-face, grazing-edge, miss, parallel-outside, `t_max` clipping,
//!   behind-origin, axis-aligned entry).  The mirror cannot take a zero
//!   direction component, so scenarios with an exactly-parallel axis are
//!   checked against the artifact's own contract only;
//! * a seeded random sweep over random domains (splitmix64, fixed seed, so the
//!   run reproduces with no external RNG dependency).

mod common;

use common::SplitMix64;
use tpt_certus_geometry::{Ray, Vector3, AABB};
use tpt_certus_proof::realization::{
    slab_realization, Decision, Interval, InputDomain, Realization,
};
use tpt_certus_spatial::ray_aabb::ray_intersects_aabb;
use tpt_certus_spatial::Intersection;

/// Endpoints and midpoint of an interval, the sweep values for one axis.
fn samples(iv: Interval) -> [f64; 3] {
    let lo = iv.lo;
    let hi = iv.hi;
    [lo, 0.5 * (lo + hi), hi]
}

fn point3(v: [f64; 3]) -> Vector3 {
    Vector3::new(v[0], v[1], v[2])
}

/// Runs one concrete input through the artifact and checks it against the
/// derived realization.  Returns the artifact outcome.
fn check_one(
    real: &Realization,
    box_: AABB,
    origin: [f64; 3],
    dir: [f64; 3],
    t_max: f64,
) -> Option<Intersection> {
    let ray = Ray::new(point3(origin), point3(dir));
    let outcome = ray_intersects_aabb(ray, box_, t_max);
    let artifact_hit = outcome.is_some();

    assert!(
        real.decision.agrees_with_artifact(artifact_hit),
        "decision {:?} disagrees with artifact outcome {} for \
         origin {:?}, dir {:?}, t_max {}",
        real.decision,
        artifact_hit,
        origin,
        dir,
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
    outcome
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
                                let outcome =
                                    check_one(&real, box_, [ox, oy, oz], [dx, dy, dz], t_max);
                                checked += 1;
                                hits += usize::from(outcome.is_some());
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

// ---------------------------------------------------------------------------
// Named reference scenarios (Section 6 contract, unit box [-1, 1]^3).
// ---------------------------------------------------------------------------

fn unit_aabb() -> AABB {
    AABB::new(Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0))
}

/// Degenerate (single-point) domain for one concrete ray, so the mirror can be
/// evaluated on exactly the scenario's inputs.  Only valid when every direction
/// component is non-zero.
fn point_domain(origin: [f64; 3], dir: [f64; 3], t_max: f64) -> InputDomain {
    let pt = |v: f64| Interval::point(v);
    InputDomain::new(
        [pt(-1.0); 3],
        [pt(1.0); 3],
        origin.map(pt),
        dir.map(pt),
        pt(t_max),
    )
    .expect("scenario direction is non-zero on every axis")
}

/// Scenario with a non-zero direction on every axis: the artifact must match
/// the contract, and the mirror must agree with it on the same single input.
fn check_nonparallel(origin: [f64; 3], dir: [f64; 3], t_max: f64) -> Option<Intersection> {
    let domain = point_domain(origin, dir, t_max);
    let real = slab_realization(&domain).expect("point domain is bounded");
    check_one(&real, unit_aabb(), origin, dir, t_max)
}

fn assert_hit(outcome: Option<Intersection>, t_near: f64, t_far: f64) {
    let hit = outcome.expect("scenario must hit");
    assert!((hit.t_near - t_near).abs() < 1e-12, "t_near {} != {}", hit.t_near, t_near);
    assert!((hit.t_far - t_far).abs() < 1e-12, "t_far {} != {}", hit.t_far, t_far);
}

#[test]
fn scenario_origin_strictly_inside_hits_at_zero() {
    // Contract: origin strictly inside ⟹ t_near == 0.0. Exit at x = 1, t = 1.
    let hit = check_nonparallel([0.0, 0.0, 0.0], [1.0, 0.5, 0.25], 10.0)
        .expect("origin inside must hit");
    assert_eq!(hit.t_near, 0.0);
    assert!((hit.t_far - 1.0).abs() < 1e-12);
}

#[test]
fn scenario_origin_on_boundary_face_hits_at_zero() {
    // Contract: origin exactly on a face is treated as inside ⟹ t_near == 0.0.
    let hit = check_nonparallel([-1.0, 0.0, 0.0], [1.0, 0.5, 0.25], 10.0)
        .expect("origin on face must hit");
    assert_eq!(hit.t_near, 0.0);
}

#[test]
fn scenario_axis_aligned_entry_distance() {
    // Parallel on y and z, so the mirror is not applicable: artifact contract only.
    let hit = ray_intersects_aabb(
        Ray::new(Vector3::new(-5.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0)),
        unit_aabb(),
        100.0,
    );
    assert_hit(hit, 4.0, 6.0);
}

#[test]
fn scenario_grazing_edge_counts_as_hit() {
    // Ray passes exactly through the edge y = 1, z = 1. Boundaries are
    // inclusive, so this is a hit at the entry distance.
    let hit = ray_intersects_aabb(
        Ray::new(Vector3::new(-5.0, 1.0, 1.0), Vector3::new(1.0, 0.0, 0.0)),
        unit_aabb(),
        100.0,
    );
    assert_hit(hit, 4.0, 6.0);
}

#[test]
fn scenario_miss_beside_box() {
    let hit = ray_intersects_aabb(
        Ray::new(Vector3::new(-5.0, 1.5, 0.0), Vector3::new(1.0, 0.0, 0.0)),
        unit_aabb(),
        100.0,
    );
    assert!(hit.is_none());
}

#[test]
fn scenario_parallel_outside_slab_misses() {
    // Exactly parallel to x and z, and outside the y slab: no intersection.
    let hit = ray_intersects_aabb(
        Ray::new(Vector3::new(0.0, 2.0, 0.0), Vector3::new(0.0, 0.0, 1.0)),
        unit_aabb(),
        100.0,
    );
    assert!(hit.is_none());
}

#[test]
fn scenario_t_max_clips_entry_with_inclusive_bound() {
    // Entry at t = 4. t_max = 3 excludes it; t_max = 4 includes it (t_near <= t_max).
    let ray = Ray::new(Vector3::new(-5.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0));
    assert!(ray_intersects_aabb(ray, unit_aabb(), 3.0).is_none());
    assert_hit(ray_intersects_aabb(ray, unit_aabb(), 4.0), 4.0, 6.0);
}

#[test]
fn scenario_box_behind_origin_misses() {
    // Box lies entirely behind the ray. The mirror only decides misses via
    // `t_near > t_max`, so this is checked against the artifact contract alone.
    let hit = ray_intersects_aabb(
        Ray::new(Vector3::new(5.0, 0.0, 0.0), Vector3::new(1.0, 0.0, 0.0)),
        unit_aabb(),
        100.0,
    );
    assert!(hit.is_none());
}

// ---------------------------------------------------------------------------
// Seeded random sweep.
// ---------------------------------------------------------------------------

const SWEEP_SEED: u64 = 0x7A11_CE5E_ED00_0013;
const SWEEP_DOMAINS: usize = 300;
const SWEEP_POINTS_PER_DOMAIN: usize = 32;

/// Random domain: a point box around the unit scale, a random origin interval,
/// a random direction interval bounded away from zero on each axis (so the
/// domain is always accepted), and a random positive `t_max` interval.
fn random_domain(rng: &mut SplitMix64) -> (InputDomain, AABB) {
    let bmin: [f64; 3] = std::array::from_fn(|_| rng.range(-2.0, 0.0));
    let bmax: [f64; 3] = std::array::from_fn(|_| rng.range(0.0, 2.0));
    let origin: [Interval; 3] = std::array::from_fn(|_| {
        let c = rng.range(-4.0, 4.0);
        let w = rng.range(0.0, 1.0);
        Interval::new(c - w, c + w).unwrap()
    });
    let direction: [Interval; 3] = std::array::from_fn(|_| {
        let a = rng.range(0.1, 1.0);
        let b = a + rng.range(0.0, 0.5);
        if rng.unit() < 0.5 {
            Interval::new(a, b).unwrap()
        } else {
            Interval::new(-b, -a).unwrap()
        }
    });
    let c = rng.range(0.1, 20.0);
    let t_max = Interval::new(c, c + rng.range(0.0, 5.0)).unwrap();

    let domain = InputDomain::new(
        bmin.map(Interval::point),
        bmax.map(Interval::point),
        origin,
        direction,
        t_max,
    )
    .expect("random domain is bounded by construction");
    let box_ = AABB::new(
        Vector3::new(bmin[0], bmin[1], bmin[2]),
        Vector3::new(bmax[0], bmax[1], bmax[2]),
    );
    (domain, box_)
}

#[test]
fn seeded_random_sweep_agrees_with_artifact() {
    let mut rng = SplitMix64(SWEEP_SEED);
    let (mut hit_domains, mut miss_domains, mut indeterminate_domains) = (0usize, 0usize, 0usize);
    let mut checked = 0usize;

    for _ in 0..SWEEP_DOMAINS {
        let (domain, box_) = random_domain(&mut rng);
        let real = slab_realization(&domain).expect("random domain is bounded");
        match real.decision {
            Decision::GuaranteedHit => hit_domains += 1,
            Decision::GuaranteedMiss => miss_domains += 1,
            Decision::Indeterminate => indeterminate_domains += 1,
        }

        for _ in 0..SWEEP_POINTS_PER_DOMAIN {
            let origin: [f64; 3] = std::array::from_fn(|i| rng.in_interval(domain.origin[i]));
            let dir: [f64; 3] = std::array::from_fn(|i| rng.in_interval(domain.direction[i]));
            let t_max = rng.in_interval(domain.t_max);
            check_one(&real, box_, origin, dir, t_max);
            checked += 1;
        }
    }

    // The sweep must actually exercise every decision class, otherwise a
    // passing run says little about the mirror.
    assert_eq!(checked, SWEEP_DOMAINS * SWEEP_POINTS_PER_DOMAIN);
    assert!(hit_domains > 0, "sweep produced no guaranteed-hit domain");
    assert!(miss_domains > 0, "sweep produced no guaranteed-miss domain");
    assert!(indeterminate_domains > 0, "sweep produced no indeterminate domain");
}
