//! Milestone 2 experiment: verification conditions for `ray_intersects_aabb`
//! in one sign case (all direction components strictly positive).
//!
//! Contract under test: the origin lies strictly inside the box and `t_max > 0`.
//! Then the ray hits, and the entry distance is exactly zero.
//!
//! Method, mirroring the compositional plan:
//! 1. Prove per-axis lemmas `t1_i ≤ 0` and `t2_i ≥ 0`, each as its own
//!    degree-2 problem (each uses the division fact `d·t = bound − o`).
//! 2. Add those lemmas as assumptions and prove the output claims.  With the
//!    lemmas in place every remaining step is linear.
//!
//! A negative control (origin outside the box) must *not* prove the claim.

extern crate alloc;

use tpt_certus_verify::ir::Graph;
use tpt_certus_verify::poly::Poly;
use tpt_certus_verify::rational::Rational;
use tpt_certus_verify::ray_aabb::{ray_aabb_graph, RayAabbGraph};
use tpt_certus_verify::refute::{Fact, Relation};
use tpt_certus_verify::vc::{prove, prove_lemma, Claim, VcError};

// Input layout, matching `ray_aabb_graph`.
const O: [u16; 3] = [0, 1, 2];
const D: [u16; 3] = [3, 4, 5];
const LO: [u16; 3] = [6, 7, 8];
const HI: [u16; 3] = [9, 10, 11];
const T_MAX: u16 = 12;

fn v(i: u16) -> Poly {
    Poly::var(i)
}

fn gt(p: Poly) -> Fact {
    Fact {
        poly: p,
        rel: Relation::Gt,
    }
}

/// `origin_i` strictly inside `(lo_i, hi_i)`, direction positive, `t_max > 0`.
fn inside_positive_assumptions(origin_inside: bool) -> alloc::vec::Vec<Fact> {
    let mut f = alloc::vec::Vec::new();
    for i in 0..3 {
        f.push(gt(v(D[i])));
        f.push(gt(v(HI[i]).sub(&v(LO[i])).unwrap()));
        if origin_inside {
            f.push(gt(v(O[i]).sub(&v(LO[i])).unwrap()));
            f.push(gt(v(HI[i]).sub(&v(O[i])).unwrap()));
        } else {
            // Origin left of the box on every axis: strictly below `lo`.
            f.push(gt(v(LO[i]).sub(&v(O[i])).unwrap()));
        }
    }
    f.push(gt(v(T_MAX)));
    f
}

fn ray_graph() -> RayAabbGraph {
    ray_aabb_graph()
}

fn zero() -> Rational {
    Rational::ZERO
}

/// Prove the per-axis lemmas and return the resulting assumption set.
fn with_axis_lemmas(
    g: &Graph,
    r: &RayAabbGraph,
    base: &[Fact],
) -> Result<alloc::vec::Vec<Fact>, VcError> {
    let mut assumptions = alloc::vec::Vec::from(base);
    for axis in 0..3 {
        let entry = prove_lemma(g, &assumptions, Claim::Le(r.t1[axis], zero()))?;
        assumptions.extend(entry);
        let exit = prove_lemma(g, &assumptions, Claim::Ge(r.t2[axis], zero()))?;
        assumptions.extend(exit);
    }
    Ok(assumptions)
}

#[test]
fn axis_lemmas_hold_in_the_positive_sign_case() {
    let r = ray_graph();
    let base = inside_positive_assumptions(true);
    let lemmas = with_axis_lemmas(&r.graph, &r, &base);
    assert!(lemmas.is_ok(), "per-axis lemmas must be provable: {lemmas:?}");
}

// Slow: each `refuted` pruning call runs an exact LP over every pair of facts,
// and this claim produces thousands of symbolic alternatives.  Run with
// `cargo test -p tpt-certus-verify --test vc_ray_aabb -- --ignored`.
#[ignore = "exceeds ~2 minutes; the search cost is the open Milestone 2 problem"]
#[test]
fn origin_inside_positive_direction_hits_at_zero() {
    let r = ray_graph();
    let base = inside_positive_assumptions(true);
    let assumptions = with_axis_lemmas(&r.graph, &r, &base).expect("lemmas");

    let hit = prove(&r.graph, &assumptions, Claim::Eq(r.hit, Rational::ONE));
    assert!(hit.is_ok(), "hit must be 1 on every path: {hit:?}");

    let t_near = prove(&r.graph, &assumptions, Claim::Eq(r.t_near, zero()));
    assert!(t_near.is_ok(), "t_near must be 0 on every path: {t_near:?}");
}

#[test]
fn negative_control_origin_outside_does_not_prove_hit_at_zero() {
    // The origin is left of the box, so the ray points away and "entry distance
    // is zero" is false.  Proven directly from the assumptions (no lemmas), the
    // prover must refuse.
    let r = ray_graph();
    let assumptions = inside_positive_assumptions(false);
    let claim = prove(&r.graph, &assumptions, Claim::Eq(r.t_near, zero()));
    assert!(claim.is_err(), "false claim must not be proven: {claim:?}");
}
