//! Bridge to `tpt-telos`'s own proof manifest (`telos-proof.json`).
//!
//! `tpt-telos` already emits a hash-sealed verification manifest for every
//! compile run ([`TelosProofManifest`]): a SHA-256 fingerprint of the source
//! `.telos` file plus per-function verification outcomes.  This module parses
//! that native artifact so `tpt-certus-proof` can *consume and extend* it
//! rather than reimplement the hashing/sealing machinery (Phase 1.4 refocus).
//!
//! The native manifest does **not** carry source locations, regulatory
//! objectives, contract summaries, or composition lemmas — those are added by
//! this crate's [`ProofCertificate`].  The two are linked by function name and,
//! in CI, by `telos verify-manifest <manifest> <source>` (native integrity
//! check against the source bytes).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Per-function proof record as emitted by `tpt-telos` codegen
/// (`tpt-telos-codegen/src/proof.rs`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelosFuncProof {
    /// Whether the FM solver discharged every conclusion for this function.
    pub verified: bool,
    /// Number of conclusion checks inspected by the solver.
    #[serde(rename = "conclusions_checked")]
    pub conclusions_checked: usize,
    /// Number of conclusion checks that passed.
    #[serde(rename = "conclusions_passed")]
    pub conclusions_passed: usize,
    /// True when at least one constraint was proved via interval-arithmetic
    /// bounding rather than exact linear arithmetic.
    #[serde(rename = "used_interval_bounding")]
    pub used_interval_bounding: bool,
}

/// The complete proof manifest emitted by one `tpt-telos` compile run
/// (`telos-proof.json`), schema-identical to `tpt-telos-codegen`'s
/// [`TelosProofManifest`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelosProofManifest {
    /// Schema version for forward compatibility (current tool value: `"1"`).
    #[serde(rename = "schema_version")]
    pub schema_version: String,
    /// `"sha256:<hex>"` of the source `.telos` file bytes.
    #[serde(rename = "source_hash")]
    pub source_hash: String,
    /// ISO 8601 timestamp of when verification ran (UTC, seconds precision).
    #[serde(rename = "verified_at")]
    pub verified_at: String,
    /// Per-function outcomes, keyed by function name.
    pub functions: BTreeMap<String, TelosFuncProof>,
    /// SHA-256 of the JSON document with this field blanked — lets downstream
    /// tools detect tampering.
    #[serde(rename = "manifest_hash")]
    pub manifest_hash: String,
}

impl TelosProofManifest {
    /// Parse a native `telos-proof.json` document.
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Function names whose proofs fully discharged (emitted in sorted order
    /// for determinism).
    pub fn verified_function_names(&self) -> Vec<&str> {
        self.functions
            .iter()
            .filter(|(_, fp)| fp.verified)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Whether every recorded function was verified (empty manifest ⇒ `false`).
    /// This is the native equivalent of this crate's hard CI gate: a manifest
    /// that lists any unverified function must not be extended into a complete
    /// [`ProofCertificate`].
    pub fn all_functions_verified(&self) -> bool {
        !self.functions.is_empty() && self.functions.values().all(|fp| fp.verified)
    }

    /// Total conclusions checked across all functions.
    pub fn total_conclusions_checked(&self) -> usize {
        self.functions
            .values()
            .map(|fp| fp.conclusions_checked)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    // Literal shape written by `tpt-telos-codegen/src/proof.rs` for a run
    // over the Phase 0/1 smoke contract and the current ray_aabb draft
    // functions.
    fn sample_manifest() -> &'static str {
        r#"{
  "schema_version": "1",
  "source_hash": "sha256:0123abcdef0123abcdef0123abcdef0123abcdef0123abcdef0123abcdef0123",
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
    }
  },
  "manifest_hash": "sha256:ffff0000ffff0000ffff0000ffff0000ffff0000ffff0000ffff0000ffff0000"
}
"#
    }

    #[test]
    fn parses_native_manifest() {
        let m = TelosProofManifest::parse(sample_manifest()).expect("native manifest parses");
        assert_eq!(m.schema_version, "1");
        assert_eq!(m.functions.len(), 2);
        assert!(m.functions["boundary_min_x"].verified);
        assert_eq!(m.functions["slab_hit_decided"].conclusions_checked, 3);
    }

    #[test]
    fn verified_names_are_sorted_and_filtered() {
        let m = TelosProofManifest::parse(sample_manifest()).expect("parses");
        assert_eq!(
            m.verified_function_names(),
            vec!["boundary_min_x", "slab_hit_decided"]
        );
    }

    #[test]
    fn unverified_function_breaks_all_verified() {
        let mut m = TelosProofManifest::parse(sample_manifest()).expect("parses");
        m.functions.insert(
            "broken".into(),
            TelosFuncProof {
                verified: false,
                conclusions_checked: 1,
                conclusions_passed: 0,
                used_interval_bounding: false,
            },
        );
        assert!(!m.all_functions_verified());
        assert_eq!(
            m.verified_function_names(),
            vec!["boundary_min_x", "slab_hit_decided"]
        );
    }

    #[test]
    fn empty_manifest_is_not_all_verified() {
        let m = TelosProofManifest::parse(
            r#"{
  "schema_version": "1",
  "source_hash": "sha256:0123abcdef0123abcdef0123abcdef0123abcdef0123abcdef0123abcdef0123",
  "verified_at": "2026-09-17T12:00:00Z",
  "functions": {},
  "manifest_hash": "sha256:ffff"
}
"#,
        )
        .expect("parses");
        assert!(!m.all_functions_verified());
        assert_eq!(m.verified_function_names(), Vec::<&str>::new());
    }

    #[test]
    fn totals_accumulate_across_functions() {
        let m = TelosProofManifest::parse(sample_manifest()).expect("parses");
        assert_eq!(m.total_conclusions_checked(), 4 + 3);
    }
}
