//! Proof-certificate assembly, error-bound bookkeeping, and audit-log export.
//!
//! [`tpt-certus-proof`] is the crate that regulators and auditors actually
//! consume.  It assembles, per build, a **Proof Certificate** containing:
//!
//! 1. The ideal-layer and realization-layer (with auto-derived `ε`) contract
//!    for every verified function reachable from the build.
//! 2. The composition lemmas used (for structures verified compositionally).
//! 3. A machine-readable manifest cross-referencing certificate entries to
//!    source locations and to the specific regulatory objective they support
//!    (e.g. DO-178C Table A-5, FDA design-control verification records).
//!
//! # Hard CI gate
//!
//! If any proof obligation fails, the build fails: an unproven contract is
//! never emitted into a Proof Certificate.  There is no partial or best-effort
//! certificate — the certificate's existence is itself evidence that every
//! obligation it lists was discharged.
//!
//! # Two-layer role
//!
//! This crate does not itself carry `tpt-telos` contracts over spatial
//! algorithms — it *consumes* the proof artifacts that `tpt-telos` and the
//! verified crates produce and assembles them into the regulatory-grade
//! certificate.
//!
//! # Status
//!
//! Phase 0 scaffold only — certificate assembly, DO-330 manifest format, and
//! CI gate wiring are added in Phase 1.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

/// A single entry in a Proof Certificate, tying a verified function to its
/// proof artifacts and regulatory objective.
#[derive(Clone, Debug, PartialEq)]
pub struct CertificateEntry {
    /// Fully-qualified function name (e.g. `tpt_certus_spatial::ray_aabb::ray_intersects_aabb`).
    pub function: alloc::string::String,
    /// Source file and line range in the `.telos` source.
    pub source_location: alloc::string::String,
    /// The ideal-layer postcondition(s) proven (human-readable summary).
    pub ideal_contract: alloc::string::String,
    /// The realization-layer postcondition(s) proven (human-readable summary).
    pub realization_contract: alloc::string::String,
    /// Auto-derived `ε` bound (may be `None` if the function has no
    /// floating-point operations).
    pub epsilon: Option<f64>,
    /// Regulatory objective(s) this certificate entry supports (e.g.
    /// `"DO-178C Table A-5 req-based-test"`).
    pub regulatory_objective: alloc::string::String,
}

/// A complete Proof Certificate for a single build.
#[derive(Clone, Debug, PartialEq)]
pub struct ProofCertificate {
    /// Monotonically increasing build identifier (typically a git SHA or CI run number).
    pub build_id: alloc::string::String,
    /// ISO-8601 timestamp of the build.
    pub timestamp: alloc::string::String,
    /// All verified-function entries in this certificate.
    pub entries: Vec<CertificateEntry>,
}

impl ProofCertificate {
    /// Create an empty certificate shell.  Phase 1 will populate this from
    /// `tpt-telos` build artifacts.
    pub fn new(build_id: alloc::string::String, timestamp: alloc::string::String) -> Self {
        ProofCertificate {
            build_id,
            timestamp,
            entries: Vec::new(),
        }
    }

    /// Number of verified-function entries in this certificate.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// All proof obligations discharged — `true` only when the certificate is
    /// complete.  Phase 1 adds a hard gate: if `!is_complete()` the build
    /// fails.
    pub fn is_complete(&self) -> bool {
        !self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_certificate_not_complete() {
        let cert = ProofCertificate::new("abc123".into(), "2026-08-20T00:00:00Z".into());
        assert_eq!(cert.entry_count(), 0);
        assert!(!cert.is_complete());
    }

    #[test]
    fn certificate_with_entry_is_complete() {
        let mut cert = ProofCertificate::new("abc123".into(), "2026-08-20T00:00:00Z".into());
        cert.entries.push(CertificateEntry {
            function: "tpt_certus_spatial::ray_aabb::ray_intersects_aabb".into(),
            source_location: "telos/ray_aabb.telos:1".into(),
            ideal_contract: "t_near >= 0.0 && t_near <= t_far && t_near <= t_max".into(),
            realization_contract: "|f64 - ideal| <= EPSILON_T".into(),
            epsilon: Some(1.1102230246251565e-16),
            regulatory_objective: "DO-178C Table A-5".into(),
        });
        assert_eq!(cert.entry_count(), 1);
        assert!(cert.is_complete());
    }
}
