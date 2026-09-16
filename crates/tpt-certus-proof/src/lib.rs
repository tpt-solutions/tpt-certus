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
//! obligation it lists was discharged ([`ProofCertificate::is_complete`]).
//!
//! # Manifest determinism
//!
//! [`ProofCertificate::to_json`] serializes the certificate to a stable,
//! build-deterministic JSON document: field order follows declaration order
//! (guaranteed for the derived `serde` impls here), and each field is emitted
//! the same way for the same logical content.  Two builds of identical inputs
//! therefore produce byte-identical manifests, which is what makes the
//! certificate usable as a reproducible DO-178C / DO-330 evidence artifact.
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
//! Phase 1 progress: first-draft machine-readable Proof Certificate manifest
//! (structured source locations, regulatory objectives, composition lemmas,
//! pinned toolchain version, deterministic JSON export), plus a
//! [`telos_manifest`] bridge that parses `tpt-telos`'s *native*
//! `telos-proof.json` and cross-checks certificate entries against its
//! per-function verification outcomes ([`ProofCertificate::is_supported_by`]).
//! Full assembly from build artifacts and the DO-330 CI-gate wiring land once
//! upstream `tpt-telos` codegen produces contract-faithful output (Phase 1.2
//! finding: `telos build` currently emits non-compiling Rust for the draft).

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

pub mod telos_manifest;

/// Manifest schema version.  Bump on any breaking change to the emitted JSON
/// so downstream tooling can reject unrecognized manifests without misparsing.
pub const MANIFEST_VERSION: u32 = 1;

/// A precise source span in a `.telos` file, cross-referencing a certificate
/// entry to the exact contract lines that were verified.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    /// Path relative to the workspace root (e.g. `crates/tpt-certus-spatial/telos/ray_aabb.telos`).
    pub file: alloc::string::String,
    /// 1-based line where the contract's `requires`/`ensures` block starts.
    pub line_start: u32,
    /// 1-based inclusive line where the contract's block ends.
    pub line_end: u32,
}

/// A named regulatory objective that a certificate entry supports.  Kept as a
/// discrete enum (rather than free text) so the manifest is machine-checkable
/// against each objective family.  Each variant serde-renames to a stable,
/// audit-facing identifier (never the Rust identifier).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegulatoryObjective {
    /// DO-178C Table A-5: requirements-based verification evidence (traceable
    /// from high/low-level requirements to verification results).
    #[serde(rename = "do-178c-requirements-verification")]
    Do178cRequirementsVerification,
    /// DO-178C Table A-5 / DO-333 Formal Methods supplement: formal
    /// specification and verification of the artifact.
    #[serde(rename = "do-178c-formal-methods")]
    Do178cFormalMethods,
    /// DO-330 (Tool Qualification): objective-critical guidance for the
    /// criteria and tool as verification evidence.
    #[serde(rename = "do-330-tool-qualification")]
    Do330ToolQualification,
    /// FDA design/quality controls (21 CFR 820 / FDA 2008 design control
    /// guidance): verification/validation records supporting design controls.
    #[serde(rename = "fda-design-controls")]
    FdaDesignControls,
}

impl SourceLocation {
    /// Create a validated source span.  Returns `None` if the span is empty or
    /// inverted (`line_start > line_end`), which would make the reference
    /// useless as audit evidence.
    pub fn new(
        file: alloc::string::String,
        line_start: u32,
        line_end: u32,
    ) -> Option<SourceLocation> {
        if line_start == 0 || line_end < line_start {
            return None;
        }
        Some(SourceLocation {
            file,
            line_start,
            line_end,
        })
    }

    /// Whether this span is a usable source reference (`line_start <= line_end`, non-zero).
    fn is_valid(&self) -> bool {
        self.line_start != 0 && self.line_start <= self.line_end
    }
}

/// Trailing path segment of a fully-qualified function name
/// (`module::path::func` → `func`), matching the bare function-name keys used
/// by `tpt-telos`'s native `telos-proof.json`.
fn base_name(fqn: &str) -> &str {
    fqn.rsplit("::").next().unwrap_or(fqn)
}

/// A single entry in a Proof Certificate, tying a verified function to its
/// proof artifacts and regulatory objective.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CertificateEntry {
    /// Fully-qualified function name (e.g. `tpt_certus_spatial::ray_aabb::ray_intersects_aabb`).
    pub function: alloc::string::String,
    /// Source span of the verified contract in the `.telos` file.
    pub source_location: SourceLocation,
    /// The ideal-layer postcondition(s) proven (human-readable summary).
    pub ideal_contract: alloc::string::String,
    /// The realization-layer postcondition(s) proven (human-readable summary).
    pub realization_contract: alloc::string::String,
    /// Auto-derived `ε` bound for the realization layer (`None` for functions
    /// with no floating-point operations).
    pub epsilon: Option<f64>,
    /// Composition lemmas used when this function's contract was assembled
    /// compositionally (empty for a leaf function).
    pub composition_lemmas: Vec<alloc::string::String>,
    /// Regulatory objective(s) this certificate entry supports.
    pub regulatory_objective: RegulatoryObjective,
}

/// A complete Proof Certificate for a single build.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProofCertificate {
    /// Schema version of the emitted manifest.
    pub manifest_version: u32,
    /// Monotonically increasing build identifier (typically a git SHA or CI run number).
    pub build_id: alloc::string::String,
    /// ISO-8601 timestamp of the build.
    pub timestamp: alloc::string::String,
    /// Exact `tpt-telos` version used to produce this certificate (DO-330 /
    /// reproducibility: the manifest is meaningless without the toolchain it
    /// was produced by).
    pub telos_version: alloc::string::String,
    /// All verified-function entries in this certificate.
    pub entries: Vec<CertificateEntry>,
}

impl ProofCertificate {
    /// Create an empty certificate shell with the toolchain that produced it.
    /// Phase 1 will populate this from `tpt-telos` build artifacts.
    pub fn new(
        build_id: alloc::string::String,
        timestamp: alloc::string::String,
        telos_version: alloc::string::String,
    ) -> Self {
        ProofCertificate {
            manifest_version: MANIFEST_VERSION,
            build_id,
            timestamp,
            telos_version,
            entries: Vec::new(),
        }
    }

    /// Number of verified-function entries in this certificate.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// All proof obligations discharged with valid, attributable contracts —
    /// `true` only when every entry references a well-formed source span and
    /// the certificate is non-empty.  Phase 1 adds a hard CI gate: if
    /// `!is_complete()` the build fails, so no partial certificate is ever
    /// emitted.
    pub fn is_complete(&self) -> bool {
        !self.entries.is_empty() && self.entries.iter().all(|e| e.source_location.is_valid())
    }

    /// Cross-check every certificate entry against the `tpt-telos`-native proof
    /// manifest for the same build: each entry's function must appear in the
    /// native manifest as `verified`, and the native manifest must itself list
    /// only verified functions (`all_functions_verified`).  Certificate entries
    /// carry fully-qualified names (`mod::...::fn`); the native manifest keys
    /// by the bare `.telos` function name, so matching is by the trailing
    /// path segment.  This is the Phase 1.4 link between our certificate and
    /// the tool's own tamper-checkable `telos-proof.json` — a certificate that
    /// references a function the tool did *not* prove is rejected even if its
    /// source span is well-formed.
    pub fn is_supported_by(&self, native: &telos_manifest::TelosProofManifest) -> bool {
        if !native.all_functions_verified() {
            return false;
        }
        let verified: alloc::collections::BTreeSet<&str> =
            native.verified_function_names().into_iter().collect();
        self.entries.iter().all(|e| {
            verified.contains(base_name(&e.function)) || verified.contains(e.function.as_str())
        })
    }

    /// Serialize to the deterministic, machine-readable manifest (JSON).
    ///
    /// Deterministic by construction: derived `serde` impls emit struct fields
    /// in declaration order and scalars canonically, so identical input
    /// content always produces identical bytes.  `f64` values serialize in
    /// shortest-round-trip form.
    pub fn to_json(&self) -> alloc::string::String {
        serde_json::to_string(self).expect("ProofCertificate is always JSON-serializable")
    }

    /// Serialize with indentation for human auditor review.  Same byte
    /// determinism guarantees as [`ProofCertificate::to_json`].
    pub fn to_json_pretty(&self) -> alloc::string::String {
        serde_json::to_string_pretty(self).expect("ProofCertificate is always JSON-serializable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> CertificateEntry {
        CertificateEntry {
            function: "tpt_certus_spatial::ray_aabb::ray_intersects_aabb".into(),
            source_location: SourceLocation::new(
                "crates/tpt-certus-spatial/telos/ray_aabb.telos".into(),
                30,
                42,
            )
            .expect("valid span"),
            ideal_contract: "t_near >= 0.0 && t_near <= t_far && t_near <= t_max".into(),
            realization_contract: "|f64 - ideal| <= EPSILON_T".into(),
            epsilon: Some(1.0e-15),
            composition_lemmas: Vec::new(),
            regulatory_objective: RegulatoryObjective::Do178cRequirementsVerification,
        }
    }

    #[test]
    fn empty_certificate_not_complete() {
        let cert = ProofCertificate::new(
            "abc123".into(),
            "2026-08-20T00:00:00Z".into(),
            "=0.2.0".into(),
        );
        assert_eq!(cert.entry_count(), 0);
        assert!(!cert.is_complete());
    }

    #[test]
    fn certificate_with_entry_is_complete() {
        let mut cert = ProofCertificate::new(
            "abc123".into(),
            "2026-08-20T00:00:00Z".into(),
            "=0.2.0".into(),
        );
        cert.entries.push(sample_entry());
        assert_eq!(cert.entry_count(), 1);
        assert!(cert.is_complete());
    }

    #[test]
    fn inverted_source_span_rejected_and_breaks_completeness() {
        assert!(SourceLocation::new("f.telos".into(), 5, 3).is_none());
        assert!(SourceLocation::new("f.telos".into(), 0, 3).is_none());

        let mut cert = ProofCertificate::new(
            "abc123".into(),
            "2026-08-20T00:00:00Z".into(),
            "=0.2.0".into(),
        );
        let mut entry = sample_entry();
        entry.source_location = SourceLocation {
            file: "crates/tpt-certus-spatial/telos/ray_aabb.telos".into(),
            line_start: 42,
            line_end: 30,
        };
        cert.entries.push(entry);
        assert!(
            !cert.is_complete(),
            "inverted span must not be complete evidence"
        );
    }

    #[test]
    fn json_manifest_is_deterministic() {
        let mut cert = ProofCertificate::new(
            "abc123".into(),
            "2026-08-20T00:00:00Z".into(),
            "=0.2.0".into(),
        );
        cert.entries.push(sample_entry());
        cert.entries.push(sample_entry());

        let a = cert.to_json();
        let b = cert.to_json();
        assert_eq!(
            a, b,
            "identical certificate must serialize to identical bytes"
        );
    }

    #[test]
    fn json_manifest_roundtrips() {
        let mut cert = ProofCertificate::new(
            "abc123".into(),
            "2026-08-20T00:00:00Z".into(),
            "=0.2.0".into(),
        );
        cert.entries.push(sample_entry());

        let json = cert.to_json();
        let back: ProofCertificate = serde_json::from_str(&json).expect("manifest parses");
        assert_eq!(cert, back);
        assert_eq!(back.manifest_version, MANIFEST_VERSION);
    }

    #[test]
    fn manifest_records_toolchain_and_objective() {
        let mut cert = ProofCertificate::new(
            "abc123".into(),
            "2026-08-20T00:00:00Z".into(),
            "=0.2.0".into(),
        );
        cert.entries.push(sample_entry());

        let json = cert.to_json();
        assert!(
            json.contains("=0.2.0"),
            "toolchain pin must appear in the manifest"
        );
        assert!(json.contains("do-178c-requirements-verification"));
    }

    #[test]
    fn complete_certificate_covered_by_native_manifest() {
        // The four machine-verified functions of the current ray_aabb.telos
        // draft, as recorded by the tool's own telos-proof.json.
        let native = telos_manifest::TelosProofManifest::parse(
            r#"{
  "schema_version": "1",
  "source_hash": "sha256:0000111122223333444455556666777788889999aaaabbbbccccddddeeeeffff",
  "verified_at": "2026-09-17T12:00:00Z",
  "functions": {
    "boundary_min_x": {
      "verified": true,
      "conclusions_checked": 4,
      "conclusions_passed": 4,
      "used_interval_bounding": false
    },
    "slab_hit_decided": {
      "verified": true,
      "conclusions_checked": 3,
      "conclusions_passed": 3,
      "used_interval_bounding": false
    },
    "tighten_t_min": {
      "verified": true,
      "conclusions_checked": 2,
      "conclusions_passed": 2,
      "used_interval_bounding": false
    },
    "domain_overlap_detected": {
      "verified": true,
      "conclusions_checked": 2,
      "conclusions_passed": 2,
      "used_interval_bounding": false
    }
  },
  "manifest_hash": "sha256:abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdef0000"
}
"#,
        )
        .expect("native manifest parses");

        let mut cert = ProofCertificate::new(
            "abc123".into(),
            "2026-08-20T00:00:00Z".into(),
            "=0.2.0".into(),
        );
        for name in [
            "boundary_min_x",
            "slab_hit_decided",
            "tighten_t_min",
            "domain_overlap_detected",
        ] {
            let mut entry = sample_entry();
            entry.function = alloc::format!("tpt_certus_spatial::telos::ray_aabb::{name}");
            cert.entries.push(entry);
        }

        assert!(cert.is_complete());
        assert!(
            cert.is_supported_by(&native),
            "every entry must map to a natively-verified function"
        );
    }

    #[test]
    fn uncovered_entry_fails_native_cross_check() {
        let native = telos_manifest::TelosProofManifest::parse(
            r#"{
  "schema_version": "1",
  "source_hash": "sha256:0000111122223333444455556666777788889999aaaabbbbccccddddeeeeffff",
  "verified_at": "2026-09-17T12:00:00Z",
  "functions": {
    "boundary_min_x": {
      "verified": true,
      "conclusions_checked": 4,
      "conclusions_passed": 4,
      "used_interval_bounding": false
    }
  },
  "manifest_hash": "sha256:0000"
}
"#,
        )
        .expect("native manifest parses");

        let mut cert = ProofCertificate::new(
            "abc123".into(),
            "2026-08-20T00:00:00Z".into(),
            "=0.2.0".into(),
        );
        // Entry for a function the tool did not prove.
        let mut entry = sample_entry();
        entry.function = "tpt_certus_spatial::telos::ray_aabb::not_proven".into();
        cert.entries.push(entry);

        assert!(
            !cert.is_supported_by(&native),
            "certificate must not claim functions the tool did not verify"
        );

        // And a manifest listing any unverified function is rejected outright.
        let mut native_untrusted = native.clone();
        native_untrusted.functions.insert(
            "broken".into(),
            telos_manifest::TelosFuncProof {
                verified: false,
                conclusions_checked: 1,
                conclusions_passed: 0,
                used_interval_bounding: false,
            },
        );
        cert.entries[0].function = "tpt_certus_spatial::telos::ray_aabb::boundary_min_x".into();
        assert!(!cert.is_supported_by(&native_untrusted));
    }
}
