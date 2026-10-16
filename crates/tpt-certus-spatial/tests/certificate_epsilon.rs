//! Per-build `ε` publication (Phase 1.4).
//!
//! The Proof Certificate must publish the realization `ε` the engine derived,
//! never a hand-written constant. The unit tests in `tpt-certus-proof` use a
//! hand-built summary, because that crate cannot depend on the artifact. This
//! test closes the gap from the artifact side: it derives the summary with
//! [`slab_realization`] and checks the certificate entry carries exactly that
//! derived value.

use tpt_certus_proof::realization::{slab_realization, Interval, InputDomain, ENGINE_ID};
use tpt_certus_proof::{CertificateEntry, RegulatoryObjective, SourceLocation};

fn guaranteed_hit_domain() -> InputDomain {
    let unit = [Interval::point(-1.0); 3];
    let top = [Interval::point(1.0); 3];
    InputDomain::new(
        unit,
        top,
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
    .expect("domain is bounded")
}

fn entry_for(domain: &InputDomain) -> CertificateEntry {
    let real = slab_realization(domain).expect("domain is bounded");
    CertificateEntry {
        function: "tpt_certus_spatial::ray_aabb::ray_intersects_aabb".into(),
        source_location: SourceLocation::new(
            "crates/tpt-certus-spatial/telos/ray_aabb.telos".into(),
            1,
            1,
        )
        .expect("valid span"),
        ideal_contract: "t_near >= 0.0 && t_near <= t_far && t_near <= t_max".into(),
        realization_contract: String::new(),
        epsilon: None,
        composition_lemmas: Vec::new(),
        regulatory_objective: RegulatoryObjective::Do178cRequirementsVerification,
        objective_rows: Vec::new(),
        native_support: Vec::new(),
        native_checks: 0,
        native_approximations: 0,
        realization: None,
    }
    .with_realization(real.summary())
}

#[test]
fn published_epsilon_equals_the_engine_derived_bound() {
    let domain = guaranteed_hit_domain();
    let real = slab_realization(&domain).expect("domain is bounded");
    let entry = entry_for(&domain);

    assert!(entry.epsilon_is_derived(), "epsilon must come from the engine");
    assert_eq!(entry.epsilon, Some(real.max_epsilon()));
    assert!(entry.epsilon.unwrap() > 0.0, "a derived bound over a non-point domain is positive");
}

#[test]
fn published_epsilon_is_per_domain_not_a_constant() {
    // A wider input domain must publish a wider ε. A hand-asserted constant
    // would publish the same value for both.
    let narrow = entry_for(&guaranteed_hit_domain());
    let wide_domain = InputDomain::new(
        [Interval::point(-1.0); 3],
        [Interval::point(1.0); 3],
        [
            Interval::new(-0.9, 0.9).unwrap(),
            Interval::new(-0.9, 0.9).unwrap(),
            Interval::new(-0.9, 0.9).unwrap(),
        ],
        [
            Interval::new(0.1, 1.0).unwrap(),
            Interval::new(0.1, 1.0).unwrap(),
            Interval::new(0.1, 1.0).unwrap(),
        ],
        Interval::new(1000.0, 1000.0).unwrap(),
    )
    .expect("domain is bounded");
    let wide = entry_for(&wide_domain);

    assert_ne!(narrow.epsilon, wide.epsilon);
    assert!(wide.epsilon.unwrap() > narrow.epsilon.unwrap());
}

#[test]
fn published_summary_records_the_engine_identity() {
    let entry = entry_for(&guaranteed_hit_domain());
    let summary = entry.realization.as_ref().expect("realization is attached");

    assert_eq!(summary.derived_by, ENGINE_ID);
    assert!(entry.realization_contract.contains(ENGINE_ID));
}
