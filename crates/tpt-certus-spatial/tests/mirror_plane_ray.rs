//! Mirror ↔ artifact cross-checks for the two further mirrored functions
//! (Phase 1.6): `Plane::signed_distance` and `Ray::plane_t`.
//!
//! Each derivation is engine-driven (`tpt_certus_proof::realization`), so
//! these checks ask the same question as `mirror_artifact.rs`: over random
//! bounded domains, does the shipped artifact's f64 result land inside the
//! envelope the engine derived for the mirror?  The check also requires the
//! domain-discipline gate to match the artifact's own `None` branch.

mod common;

use common::SplitMix64;
use tpt_certus_geometry::{Plane, Ray, Vector3};
use tpt_certus_proof::realization::{
    point_plane_distance_realization, ray_plane_t_realization, DomainError, GraphRealization,
    Interval, PlaneDistanceDomain, RayPlaneDomain,
};

const SEED: u64 = 0x91A4_E5ED_0017_0006;
const DOMAINS: usize = 300;
const POINTS_PER_DOMAIN: usize = 32;

fn v3(a: [f64; 3]) -> Vector3 {
    Vector3::new(a[0], a[1], a[2])
}

/// Random interval triple centred in `[c_lo, c_hi]` with half-width up to `w`.
fn triple(rng: &mut SplitMix64, c_lo: f64, c_hi: f64, w: f64) -> [Interval; 3] {
    std::array::from_fn(|_| {
        let half = rng.range(0.0, w);
        rng.around(c_lo, c_hi, half)
    })
}

fn sample3(rng: &mut SplitMix64, iv: &[Interval; 3]) -> [f64; 3] {
    std::array::from_fn(|i| rng.in_interval(iv[i]))
}

/// `Some(envelope width)` if the envelope contains the artifact value.
fn assert_in_envelope(real: &GraphRealization, name: &str, value: f64) {
    let q = real.quantity(name).expect("requested output present");
    assert!(
        q.interval.contains(value),
        "{name}: artifact {value} outside derived envelope {:?}",
        q.interval
    );
}

#[test]
fn point_plane_distance_envelope_contains_artifact() {
    let mut rng = SplitMix64(SEED);
    let (mut derived, mut refused) = (0usize, 0usize);

    for _ in 0..DOMAINS {
        // Normal components are bounded away from zero on every axis, so `|n|`
        // never spans zero and the domain is accepted.
        let normal: [Interval; 3] = std::array::from_fn(|_| {
            let sign = if rng.unit() < 0.5 { -1.0 } else { 1.0 };
            let a = rng.range(0.5, 2.0);
            let w = rng.range(0.0, 0.2);
            let c = sign * a;
            Interval::new(c - w, c + w).unwrap()
        });
        let point = triple(&mut rng, -3.0, 3.0, 1.0);
        let plane_point = triple(&mut rng, -3.0, 3.0, 1.0);

        let domain = PlaneDistanceDomain::new(point, plane_point, normal)
            .expect("finite domain");
        let real = match point_plane_distance_realization(&domain) {
            Ok(r) => r,
            Err(DomainError::DivisionSpansZero(_)) => {
                refused += 1;
                continue;
            }
            Err(e) => panic!("unexpected refusal: {e:?}"),
        };
        derived += 1;

        let eps = real.quantity("signed_distance").unwrap().epsilon;
        assert!(eps.is_finite() && eps >= 0.0, "epsilon must be a finite non-negative width");

        for _ in 0..POINTS_PER_DOMAIN {
            let p = sample3(&mut rng, &point);
            let q = sample3(&mut rng, &plane_point);
            let n = sample3(&mut rng, &normal);
            let value = Plane::new(v3(q), v3(n)).signed_distance(v3(p));
            assert_in_envelope(&real, "signed_distance", value);
        }
    }

    assert!(derived > 0, "sweep derived no envelope");
    assert!(refused == 0, "normal bounded away from zero must never be refused");
}

#[test]
fn ray_plane_t_envelope_and_parallel_gate_match_artifact() {
    let mut rng = SplitMix64(SEED ^ 0xFFFF);
    let (mut derived, mut refused) = (0usize, 0usize);

    for _ in 0..DOMAINS {
        let origin = triple(&mut rng, -3.0, 3.0, 1.0);
        // Direction and normal drawn from wide ranges so that some domains have
        // a denominator `d·n` that spans zero (refused) and some do not.
        let direction = triple(&mut rng, -1.0, 1.0, 0.3);
        let plane_point = triple(&mut rng, -3.0, 3.0, 1.0);
        let normal = triple(&mut rng, -1.0, 1.0, 0.3);

        let domain = RayPlaneDomain::new(origin, direction, plane_point, normal)
            .expect("finite domain");
        let real = match ray_plane_t_realization(&domain) {
            Ok(r) => r,
            Err(DomainError::DivisionSpansZero(_)) => {
                refused += 1;
                continue;
            }
            Err(e) => panic!("unexpected refusal: {e:?}"),
        };
        derived += 1;

        let name = &real.quantities[0].name;
        for _ in 0..POINTS_PER_DOMAIN {
            let o = sample3(&mut rng, &origin);
            let d = sample3(&mut rng, &direction);
            let q = sample3(&mut rng, &plane_point);
            let n = sample3(&mut rng, &normal);
            let t = Ray::new(v3(o), v3(d))
                .plane_t(&Plane::new(v3(q), v3(n)))
                .expect("a derived (denominator-excluding-zero) domain never hits the parallel branch");
            assert_in_envelope(&real, name, t);
        }
    }

    assert!(derived > 0, "sweep derived no envelope");
    assert!(refused > 0, "sweep never exercised the parallel-refusal gate");
}

#[test]
fn ray_plane_parallel_domain_is_refused_with_no_epsilon() {
    // Direction exactly orthogonal to the normal on every sample: the artifact
    // returns None, and the derivation must refuse rather than emit an ε.
    let domain = RayPlaneDomain::new(
        [Interval::point(0.0); 3],
        [
            Interval::new(1.0, 2.0).unwrap(),
            Interval::point(0.0),
            Interval::point(0.0),
        ],
        [Interval::point(5.0); 3],
        [
            Interval::new(0.0, 0.0).unwrap(),
            Interval::point(1.0),
            Interval::point(0.0),
        ],
    )
    .expect("finite domain");

    assert!(matches!(
        ray_plane_t_realization(&domain),
        Err(DomainError::DivisionSpansZero(_))
    ));
    assert!(Ray::new(Vector3::ZERO, Vector3::new(1.0, 0.0, 0.0))
        .plane_t(&Plane::new(Vector3::ZERO, Vector3::new(0.0, 1.0, 0.0)))
        .is_none());
}
