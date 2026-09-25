//! Bridge to `tpt-telos`'s machine-readable verification report
//! (`telos verify --json`).
//!
//! `tpt-telos` emits two native artifacts and this crate consumes both:
//!
//! * [`crate::telos_manifest`] — the hash-sealed `telos-proof.json` that
//!   `telos build` writes next to the generated crate.  It is only produced for
//!   contracts the codegen path can build (v0.2.0 fails on any function whose
//!   body contains an `if`; see `todo.md` Phase 1.2).
//! * this module — the per-function, per-check report from
//!   `telos verify --json`, which works for *every* parseable contract
//!   (including `crates/tpt-certus-spatial/telos/ray_aabb.telos`) and carries
//!   the source locations and the "discharged by interval-arithmetic
//!   approximation rather than exact arithmetic" flag a certificate needs.
//!
//! Neither artifact is trusted blindly: per-function `all_passed` is the gate
//! (it already folds in disjunction-group semantics, where each disjunct of an
//! `||`-joined conclusion is checked separately), and the approximation flag is
//! surfaced so a certificate can refuse to present an
//! approximation-discharged conclusion as exact.
//!
//! # What the report does *not* contain
//!
//! Source hashes, timestamps, regulatory objectives, contract summaries, and the
//! derived `ε` bound.  Hashing/sealing comes from `telos-proof.json` where
//! available; the rest is added by [`ProofCertificate`](crate::ProofCertificate).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// A source location reported by `telos verify --json` (1-based line/column).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyLocation {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub column: u32,
}

/// One conclusion check inside a verified function.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerifyCheck {
    /// Human-readable conclusion text (e.g. `ensures: out.flag == 1`).
    pub description: String,
    /// Whether the solver discharged this conclusion.
    pub passed: bool,
    /// Whether the conclusion came from an `ensures` clause (as opposed to an
    /// automatically maintained `invariant`).
    pub is_ensures: bool,
    /// Whether the conclusion was discharged by interval-arithmetic bounding
    /// rather than exact arithmetic — an *approximation* a certificate must not
    /// present as an exact result (DO-330 provenance).
    pub is_approximation: bool,
    /// Counterexample assignments for a failed conclusion (`None` when passed).
    pub counterexample: Option<BTreeMap<String, serde_json::Value>>,
    /// Index of the disjunction group this check belongs to, for `||`-joined
    /// conclusions that are checked one disjunct at a time.
    pub or_group: Option<u32>,
    /// Where in the `.telos` source the conclusion is written.
    pub location: Option<VerifyLocation>,
}

/// One function (or one branch of a function) from the report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerifyFunction {
    /// Report name, e.g. `slab_hit_decided[branch 1]`; branch suffixes are
    /// stripped by [`VerifyReport::base_function_name`].
    pub func_name: String,
    /// Whether every conclusion for this function/branch was discharged
    /// (disjunction groups are already folded into this flag by the tool).
    pub all_passed: bool,
    /// The individual conclusions inspected.
    pub checks: Vec<VerifyCheck>,
}

/// The complete `telos verify --json` report for one `.telos` source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerifyReport {
    /// The source path as passed to the tool.
    pub file: String,
    /// Tool-level verdict.
    pub passed: bool,
    /// Per-function results.
    pub functions: Vec<VerifyFunction>,
}

impl VerifyReport {
    /// Parse a `telos verify --json` document.
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Strip a report entry's `[branch N]` suffix, leaving the `.telos` function
    /// name.
    pub fn base_function_name(report_name: &str) -> &str {
        match report_name.split_once('[') {
            Some((base, _)) => base.trim_end(),
            None => report_name,
        }
    }

    /// Whether the report is non-empty and every entry passed.
    pub fn all_functions_passed(&self) -> bool {
        !self.functions.is_empty() && self.functions.iter().all(|f| f.all_passed)
    }

    /// Names of the fully discharged functions, deduplicated and sorted (branch
    /// results fold into their base name).
    pub fn verified_function_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .functions
            .iter()
            .filter(|f| f.all_passed)
            .map(|f| Self::base_function_name(&f.func_name))
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Names of entries that failed to discharge, for error reporting.
    pub fn failing_function_names(&self) -> Vec<&str> {
        self.functions
            .iter()
            .filter(|f| !f.all_passed)
            .map(|f| f.func_name.as_str())
            .collect()
    }

    /// Total number of conclusions checked.
    pub fn total_checks(&self) -> usize {
        self.functions.iter().map(|f| f.checks.len()).sum()
    }

    /// Number of conclusions discharged by approximation rather than exact
    /// arithmetic.  Non-zero means a certificate must not describe those
    /// conclusions as exact.
    pub fn approximation_checks(&self) -> usize {
        self.functions
            .iter()
            .flat_map(|f| f.checks.iter())
            .filter(|c| c.is_approximation)
            .count()
    }

    /// Whether `name` (a bare `.telos` function name) was fully discharged.
    pub fn is_verified(&self, name: &str) -> bool {
        self.functions
            .iter()
            .any(|f| f.all_passed && Self::base_function_name(&f.func_name) == name)
    }

    /// The 1-based inclusive source span of every conclusion belonging to a base
    /// function name, or `None` when the report carries no locations for it.
    pub fn source_span(&self, base_name: &str) -> Option<(u32, u32)> {
        let mut span: Option<(u32, u32)> = None;
        for function in &self.functions {
            if Self::base_function_name(&function.func_name) != base_name {
                continue;
            }
            for check in &function.checks {
                if let Some(location) = check.location {
                    span = Some(match span {
                        None => (location.line, location.line),
                        Some((lo, hi)) => (lo.min(location.line), hi.max(location.line)),
                    });
                }
            }
        }
        span
    }

    /// The `ensures` conclusion texts for a base function name (deduplicated,
    /// report order).
    pub fn ensures_texts(&self, base_name: &str) -> Vec<String> {
        let mut texts: Vec<String> = Vec::new();
        for function in &self.functions {
            if Self::base_function_name(&function.func_name) != base_name {
                continue;
            }
            for check in &function.checks {
                if check.is_ensures && !texts.contains(&check.description) {
                    texts.push(check.description.clone());
                }
            }
        }
        texts
    }

    /// Number of conclusions recorded for a base function name.
    pub fn check_count(&self, base_name: &str) -> usize {
        self.functions
            .iter()
            .filter(|f| Self::base_function_name(&f.func_name) == base_name)
            .map(|f| f.checks.len())
            .sum()
    }

    /// Number of approximation-discharged conclusions for a base function name.
    pub fn approximation_count(&self, base_name: &str) -> usize {
        self.functions
            .iter()
            .filter(|f| Self::base_function_name(&f.func_name) == base_name)
            .flat_map(|f| f.checks.iter())
            .filter(|c| c.is_approximation)
            .count()
    }
}

impl VerifyCheck {
    /// A one-line provenance summary used in certificate contract strings.
    pub fn summary(&self) -> String {
        let mut out = self.description.clone();
        if self.is_approximation {
            out.push_str(" (approximation-discharged)");
        }
        if !self.passed {
            out.push_str(" (FAILED)");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed but field-complete excerpt of a real
    /// `telos verify --json crates/tpt-certus-spatial/telos/ray_aabb.telos`
    /// run, including one disjunction-group check that reports `passed: false`
    /// for a single disjunct while the enclosing function still passes.
    const SAMPLE: &str = r#"{
  "file": "crates/tpt-certus-spatial/telos/ray_aabb.telos",
  "passed": true,
  "functions": [
    {"func_name": "boundary_min_x", "all_passed": true, "checks": [
      {"description": "ensures: out.flag == 1", "passed": true, "is_ensures": true, "is_approximation": false, "counterexample": null, "or_group": null, "location": {"line": 81, "column": 17}},
      {"description": "invariant Box maintained: min_x <= max_x", "passed": true, "is_ensures": false, "is_approximation": false, "counterexample": null, "or_group": null, "location": {"line": 56, "column": 9}}
    ]},
    {"func_name": "slab_hit_decided[branch 0]", "all_passed": true, "checks": [
      {"description": "ensures: out.flag == 1", "passed": true, "is_ensures": true, "is_approximation": false, "counterexample": null, "or_group": null, "location": {"line": 98, "column": 17}}
    ]},
    {"func_name": "domain_overlap_detected[branch 0]", "all_passed": true, "checks": [
      {"description": "ensures: out.flag == 0 || out.flag == 1 [branch 0]", "passed": false, "is_ensures": true, "is_approximation": false, "counterexample": {"out.flag": 0, "out.flag'": 1}, "or_group": 0, "location": {"line": 154, "column": 17}},
      {"description": "ensures: out.flag == 0 || out.flag == 1 [branch 1]", "passed": true, "is_ensures": true, "is_approximation": false, "counterexample": null, "or_group": 0, "location": {"line": 154, "column": 17}}
    ]}
  ]
}"#;

    #[test]
    fn parses_a_real_report_and_folds_branches() {
        let report = VerifyReport::parse(SAMPLE).expect("report parses");
        assert_eq!(
            report.file,
            "crates/tpt-certus-spatial/telos/ray_aabb.telos"
        );
        assert!(report.passed);
        assert!(report.all_functions_passed());
        assert_eq!(
            report.verified_function_names(),
            vec![
                "boundary_min_x",
                "domain_overlap_detected",
                "slab_hit_decided"
            ]
        );
        assert_eq!(report.total_checks(), 4);
        assert_eq!(report.approximation_checks(), 0);
        assert!(report.failing_function_names().is_empty());
    }

    #[test]
    fn disjunction_group_failure_does_not_fail_the_function() {
        // The single failing disjunct above lives inside a passing or_group; the
        // tool already folded that into `all_passed`, and we must not "correct" it.
        let report = VerifyReport::parse(SAMPLE).expect("report parses");
        assert!(report.is_verified("domain_overlap_detected"));
        let failed: Vec<&VerifyCheck> = report
            .functions
            .iter()
            .flat_map(|f| f.checks.iter())
            .filter(|c| !c.passed)
            .collect();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].or_group, Some(0));
        assert!(failed[0].summary().contains("FAILED"));
        assert_eq!(failed[0].counterexample.as_ref().map(|c| c.len()), Some(2));
    }

    #[test]
    fn spans_texts_and_counts_are_per_base_function() {
        let report = VerifyReport::parse(SAMPLE).expect("report parses");
        assert_eq!(report.source_span("boundary_min_x"), Some((56, 81)));
        assert_eq!(report.source_span("slab_hit_decided"), Some((98, 98)));
        assert_eq!(report.source_span("missing"), None);
        assert_eq!(report.ensures_texts("slab_hit_decided").len(), 1);
        assert_eq!(report.ensures_texts("domain_overlap_detected").len(), 2);
        assert_eq!(report.check_count("boundary_min_x"), 2);
        assert_eq!(report.approximation_count("boundary_min_x"), 0);
        assert_eq!(VerifyReport::base_function_name("f[branch 12]"), "f");
        assert_eq!(VerifyReport::base_function_name("f"), "f");
    }

    #[test]
    fn failed_or_empty_reports_are_rejected() {
        let mut report = VerifyReport::parse(SAMPLE).expect("report parses");
        report.functions[0].all_passed = false;
        assert!(!report.all_functions_passed());
        assert_eq!(report.failing_function_names(), vec!["boundary_min_x"]);
        assert!(!report.is_verified("boundary_min_x"));

        let empty = VerifyReport {
            file: "empty.telos".to_string(),
            passed: true,
            functions: Vec::new(),
        };
        assert!(!empty.all_functions_passed());
    }

    #[test]
    fn approximation_flag_is_counted_and_surfaced() {
        let mut report = VerifyReport::parse(SAMPLE).expect("report parses");
        report.functions[0].checks[0].is_approximation = true;
        assert_eq!(report.approximation_checks(), 1);
        assert_eq!(report.approximation_count("boundary_min_x"), 1);
        assert!(report.functions[0].checks[0]
            .summary()
            .contains("approximation-discharged"));
    }
}
