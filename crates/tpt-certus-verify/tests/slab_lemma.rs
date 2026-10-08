//! Milestone 0 spike: the single-axis slab ordering lemma.
//!
//! Along one axis the slab method computes entry and exit parameters `t1` and
//! `t2` from `d · t = bound − origin`, where `d` is the direction component.
//! For `d > 0` the entry must not exceed the exit whenever `lo ≤ hi`.
//!
//! Division is avoided by writing the defining equations multiplied out, so
//! the claim `t1 ≤ t2` becomes a degree-2 refutation of its negation
//! `t1 − t2 > 0`.  The product `d · (t1 − t2)` is the only nonlinear step.

extern crate alloc;

use tpt_certus_verify::poly::Poly;
use tpt_certus_verify::rational::Rational;
use tpt_certus_verify::refute::{check, CheckError, Fact, Problem, Refutation, Relation, Term};

// Variable indices.
const O: u16 = 0; // origin along the axis
const D: u16 = 1; // direction component
const T1: u16 = 2; // entry parameter
const T2: u16 = 3; // exit parameter
const LO: u16 = 4; // box minimum
const HI: u16 = 5; // box maximum

fn v(i: u16) -> Poly {
    Poly::var(i)
}

fn k(n: i128) -> Poly {
    Poly::constant(Rational::from_int(n))
}

/// Facts for the lemma `d > 0, d·t1 = lo − o, d·t2 = hi − o, lo ≤ hi ⊢ t1 ≤ t2`.
/// The last fact is the negated goal `t1 − t2 > 0`.
fn slab_problem() -> Problem {
    // d·t1 + o − lo = 0
    let e1 = v(D).mul(&v(T1)).unwrap().add(&v(O)).unwrap().sub(&v(LO)).unwrap();
    // d·t2 + o − hi = 0
    let e2 = v(D).mul(&v(T2)).unwrap().add(&v(O)).unwrap().sub(&v(HI)).unwrap();
    Problem {
        facts: alloc::vec![
            Fact { poly: v(D), rel: Relation::Gt },                              // 0: d > 0
            Fact { poly: e1, rel: Relation::Eq },                                // 1: E1
            Fact { poly: e2, rel: Relation::Eq },                                // 2: E2
            Fact { poly: v(HI).sub(&v(LO)).unwrap(), rel: Relation::Ge },        // 3: hi − lo ≥ 0
            Fact { poly: v(T1).sub(&v(T2)).unwrap(), rel: Relation::Gt },        // 4: t1 − t2 > 0 (negated goal)
        ],
    }
}

/// The certificate: `d(t1−t2) − E1 + E2 + (hi−lo) = 0`, a strict contradiction.
fn slab_certificate() -> Refutation {
    Refutation {
        terms: alloc::vec![
            Term { coeff: Rational::ONE, factors: alloc::vec![0, 4] }, // d · (t1 − t2) > 0
            Term { coeff: Rational::from_int(-1), factors: alloc::vec![1] }, // −E1
            Term { coeff: Rational::ONE, factors: alloc::vec![2] },    // +E2
            Term { coeff: Rational::ONE, factors: alloc::vec![3] },    // +(hi − lo)
        ],
    }
}

#[test]
fn entry_precedes_exit_for_positive_direction() {
    let problem = slab_problem();
    assert_eq!(check(&problem, &slab_certificate()), Ok(()));
}

#[test]
fn dropping_the_nonlinear_product_is_rejected() {
    // Without d·(t1−t2) the linear terms cannot cancel to a constant.
    let mut cert = slab_certificate();
    cert.terms.remove(0);
    assert_eq!(check(&slab_problem(), &cert), Err(CheckError::NotConstant));
}

#[test]
fn tampered_ordering_hypothesis_is_rejected() {
    // Zeroing the box-ordering coefficient leaves lo − hi, which is not constant.
    let mut cert = slab_certificate();
    cert.terms[3].coeff = Rational::ZERO;
    assert_eq!(check(&slab_problem(), &cert), Err(CheckError::NotConstant));
}

#[test]
fn negative_inequality_coefficient_is_rejected() {
    let mut cert = slab_certificate();
    cert.terms[3].coeff = Rational::from_int(-1);
    assert_eq!(check(&slab_problem(), &cert), Err(CheckError::NegativeCoefficient(3)));
}

#[test]
fn reversed_claim_is_not_provable_with_the_same_certificate() {
    // Claiming t2 ≤ t1 (negated goal t2 − t1 > 0) must not be certified by the
    // certificate written for t1 ≤ t2.
    let mut problem = slab_problem();
    problem.facts[4].poly = v(T2).sub(&v(T1)).unwrap();
    assert!(check(&problem, &slab_certificate()).is_err());
}

#[test]
fn unknown_fact_index_is_rejected() {
    let mut cert = slab_certificate();
    cert.terms[0].factors = alloc::vec![0, 99];
    assert_eq!(check(&slab_problem(), &cert), Err(CheckError::UnknownFact(99)));
}

#[test]
fn constant_contradiction_is_accepted() {
    // 0 > −1 is contradicted by the single fact −1 ≥ 0 written as a constant term.
    let problem = Problem {
        facts: alloc::vec![Fact { poly: k(-1), rel: Relation::Ge }],
    };
    let cert = Refutation {
        terms: alloc::vec![Term { coeff: Rational::ONE, factors: alloc::vec![0] }],
    };
    assert_eq!(check(&problem, &cert), Ok(()));
}

#[test]
fn search_rediscovers_a_verified_slab_certificate() {
    // The untrusted search must find some certificate, and the checker must accept it.
    let problem = slab_problem();
    let cert = tpt_certus_verify::search::find_refutation(&problem)
        .expect("degree-2 search finds the slab lemma");
    assert_eq!(check(&problem, &cert), Ok(()));
}

#[test]
fn search_finds_a_linear_contradiction() {
    // x ≥ 1 and −x ≥ 0 are jointly unsatisfiable.
    let problem = Problem {
        facts: alloc::vec![
            Fact { poly: v(0).sub(&k(1)).unwrap(), rel: Relation::Ge },
            Fact { poly: v(0).neg().unwrap(), rel: Relation::Ge },
        ],
    };
    let cert = tpt_certus_verify::search::find_refutation(&problem).expect("refutable");
    assert_eq!(check(&problem, &cert), Ok(()));
}

#[test]
fn search_does_not_refute_a_satisfiable_problem() {
    // x ≥ 0 alone has a model, so no refutation may be produced.
    let problem = Problem {
        facts: alloc::vec![Fact { poly: v(0), rel: Relation::Ge }],
    };
    assert!(tpt_certus_verify::search::find_refutation(&problem).is_none());
}

#[test]
fn search_does_not_refute_the_false_reversed_claim() {
    // t2 ≤ t1 does not follow from the slab facts, so nothing may be certified.
    let mut problem = slab_problem();
    problem.facts[4].poly = v(T2).sub(&v(T1)).unwrap();
    assert!(tpt_certus_verify::search::find_refutation(&problem).is_none());
}
