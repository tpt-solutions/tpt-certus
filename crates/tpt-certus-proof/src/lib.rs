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
//! Phase 1: the machine-readable Proof Certificate manifest (structured source
//! locations, regulatory objectives, composition lemmas, pinned toolchain
//! version, deterministic JSON export) is in place, together with **two native
//! evidence bridges**:
//!
//! * [`telos_manifest`] parses the hash-sealed `telos-proof.json` that
//!   `telos build` emits, cross-checked by
//!   [`ProofCertificate::is_supported_by`].
//! * [`verify_report`] parses `telos verify --json`, which covers contracts the
//!   codegen path cannot build (v0.2.0 fails on any function whose body contains
//!   an `if`), cross-checked by [`ProofCertificate::is_supported_by_report`].
//!
//! [`ProofCertificate::assemble_from_report`] builds a certificate directly from
//! a verify report plus the artifact-level entries, and refuses (with
//! [`AssemblyError`]) to emit anything that fails the hard gate.
//!
//! Per-build `ε` is whatever [`realization`] derived for the mirrored f64
//! operation graph — [`CertificateEntry::with_realization`] writes it, and
//! [`ProofCertificate::blocking_problems`] rejects any certificate whose
//! published bound is not exactly the derived one, so `ε` can never be
//! hand-asserted into the manifest.
//!
//! Still pending: wiring the gate into CI over the generated artifact (Phase 1.4)
//! and the full DO-330 objective mapping (Phase 2).

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

pub mod realization;
pub mod telos_manifest;
pub mod verify_report;

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

/// A DO-178C Table A-5 objective row that a certificate entry supports.
///
/// Table A-5 covers *verification of the outputs of the software coding and
/// integration process* — the objectives a per-function verification record can
/// actually speak to.  Variants are named by the objective's own wording (quoted
/// in each variant's documentation) rather than by a row number: assigning the
/// numeric row identifiers, and folding in the DO-333 formal-methods supplement
/// substitutions, is part of the Phase 2 DO-330 / DO-178C mapping review, and
/// inventing numbers here would put an unverified citation into a
/// certification-facing artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Do178cTableA5Row {
    /// "Source code complies with software code standards" — the verified
    /// contract is machine-checked, so compliance with the coding standard is
    /// evidenced by a tool rather than by review alone.
    #[serde(rename = "do-178c-table-a5-source-code-complies-with-standards")]
    SourceCodeCompliesWithStandards,
    /// "Source code is traceable to low-level requirements" — the entry names
    /// the `.telos` source span it covers and the native conclusions that
    /// discharge it, which *is* the traceability record.
    #[serde(rename = "do-178c-table-a5-source-code-traceable-to-low-level-requirements")]
    SourceCodeTraceableToLowLevelRequirements,
    /// "Source code is accurate and consistent" — the ideal-layer contract is
    /// discharged for the exact statement the code implements, and the
    /// realization layer bounds the difference between the executing f64
    /// artifact and that statement.
    #[serde(rename = "do-178c-table-a5-source-code-accurate-and-consistent")]
    SourceCodeAccurateAndConsistent,
    /// DO-333 formal-methods supplement to that table: the objective is met by
    /// formal specification and verification instead of by review or test, using
    /// a tool that itself has to be qualified (DO-330, Phase 2).
    #[serde(rename = "do-333-formal-methods-supplement-to-table-a5")]
    Do333FormalMethodsSupplement,
}

impl Do178cTableA5Row {
    /// The document this row belongs to.
    pub fn table_id(&self) -> &'static str {
        match self {
            Do178cTableA5Row::Do333FormalMethodsSupplement => {
                "DO-333 (supplement to DO-178C Table A-5)"
            }
            _ => "DO-178C Table A-5",
        }
    }

    /// The objective requirement this row's evidence speaks to, as recorded in
    /// the certificate (abbreviated from the table's own wording).
    pub fn objective(&self) -> &'static str {
        match self {
            Do178cTableA5Row::SourceCodeCompliesWithStandards => {
                "source code complies with software code standards"
            }
            Do178cTableA5Row::SourceCodeTraceableToLowLevelRequirements => {
                "source code is traceable to low-level requirements"
            }
            Do178cTableA5Row::SourceCodeAccurateAndConsistent => {
                "source code is accurate and consistent"
            }
            Do178cTableA5Row::Do333FormalMethodsSupplement => {
                "formal specification and verification of the artifact (DO-333 supplement)"
            }
        }
    }

    /// The rows a natively verified `.telos` contract entry supports by default:
    /// accuracy/consistency of the code against its specification, plus the
    /// traceability record the entry itself is.
    pub fn default_for_verified_contract() -> Vec<Self> {
        vec![
            Do178cTableA5Row::SourceCodeAccurateAndConsistent,
            Do178cTableA5Row::SourceCodeTraceableToLowLevelRequirements,
        ]
    }
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
/// proof artifacts, derived bound, and regulator-facing objectives.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CertificateEntry {
    /// Fully-qualified function name (e.g. `tpt_certus_spatial::ray_aabb::ray_intersects_aabb`)
    /// or the bare `.telos` function name.
    pub function: String,
    /// Source span of the verified contract in the `.telos` file.
    pub source_location: SourceLocation,
    /// The ideal-layer postcondition(s) proven (human-readable summary).
    pub ideal_contract: String,
    /// The realization-layer postcondition(s) proven (human-readable summary).
    pub realization_contract: String,
    /// The derived `ε` bound for the realization layer (`None` for functions with
    /// no floating-point operations).
    ///
    /// **Derived, never hand-asserted**: written by
    /// [`CertificateEntry::with_realization`] from
    /// [`realization::RealizationSummary::epsilon`], and
    /// [`ProofCertificate::blocking_problems`] rejects any certificate where the
    /// two disagree.
    pub epsilon: Option<f64>,
    /// Composition lemmas used when this function's contract was assembled
    /// compositionally (empty for a leaf function).
    pub composition_lemmas: Vec<String>,
    /// Regulatory objective this certificate entry supports.
    pub regulatory_objective: RegulatoryObjective,
    /// DO-178C Table A-5 objective rows this entry provides evidence for.
    pub objective_rows: Vec<Do178cTableA5Row>,
    /// Native `.telos` functions whose discharge supports this entry.  Empty when
    /// `function` is itself a native `.telos` function name; populated for
    /// artifact entries (e.g. the shipped `ray_intersects_aabb`) whose contract
    /// shapes are discharged by several native functions.
    pub native_support: Vec<String>,
    /// Conclusions the native tool inspected for this entry.
    pub native_checks: usize,
    /// Conclusions discharged by approximation rather than exact arithmetic.  A
    /// certificate refuses to emit entries with a non-zero count unless
    /// [`ProofCertificate::allow_approximations`] is set explicitly.
    pub native_approximations: usize,
    /// The derived realization bound, when this entry mirrors an f64 artifact.
    pub realization: Option<realization::RealizationSummary>,
}

impl CertificateEntry {
    /// Attach a derived realization summary.
    ///
    /// Sets `epsilon` and the human-readable `realization_contract` from the
    /// engine's own output, so the published bound is never hand-written and the
    /// engine identity travels with it.
    pub fn with_realization(mut self, summary: realization::RealizationSummary) -> Self {
        let mut contract = String::new();
        for (index, quantity) in summary.quantities.iter().enumerate() {
            if index > 0 {
                contract.push_str(", ");
            }
            contract.push_str(&alloc::format!(
                "|f64 {} − ideal {}| ≤ {}",
                quantity.name, quantity.name, quantity.epsilon
            ));
        }
        if let Some(decision) = summary.decision {
            contract.push_str(&alloc::format!("; hit/miss decision: {decision:?}"));
        }
        contract.push_str(&alloc::format!(" (derived by {})", summary.derived_by));
        self.epsilon = Some(summary.epsilon);
        self.realization_contract = contract;
        self.realization = Some(summary);
        self
    }

    /// Record the native `.telos` functions that discharge this entry's contract
    /// shapes, and how many conclusions the tool checked for them.
    pub fn with_native_support(
        mut self,
        functions: Vec<String>,
        checks: usize,
        approximations: usize,
    ) -> Self {
        self.native_support = functions;
        self.native_checks = checks;
        self.native_approximations = approximations;
        self
    }

    /// Record the DO-178C Table A-5 objective rows this entry provides evidence
    /// for.
    pub fn with_objective_rows(mut self, rows: Vec<Do178cTableA5Row>) -> Self {
        self.objective_rows = rows;
        self
    }

    /// Whether the published `ε` is exactly the derived one (and is present iff a
    /// realization is attached).  This is what makes "not hand-asserted"
    /// machine-checkable.
    pub fn epsilon_is_derived(&self) -> bool {
        match (&self.realization, self.epsilon) {
            (Some(summary), Some(epsilon)) => summary.epsilon == epsilon,
            (None, None) => true,
            _ => false,
        }
    }

    /// Whether this entry records native `tpt-telos` evidence: at least one
    /// inspected conclusion, or a declared list of native supporters.
    pub fn has_native_evidence(&self) -> bool {
        self.native_checks > 0 || !self.native_support.is_empty()
    }
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
    /// Whether conclusions discharged by interval-arithmetic *approximation*
    /// rather than exact arithmetic may be listed.  `false` (the default) makes
    /// the hard gate reject any entry with `native_approximations > 0`, so an
    /// approximation-discharged conclusion can never be presented as exact.
    pub allow_approximations: bool,
}

impl ProofCertificate {
    /// Create an empty certificate shell with the toolchain that produced it.
    /// Populate it with [`ProofCertificate::assemble_from_report`] (or by hand
    /// plus [`ProofCertificate::is_certifiable`]).
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
            allow_approximations: false,
        }
    }

    /// Number of verified-function entries in this certificate.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Opt in to listing conclusions the tool discharged by approximation,
    /// instead of failing the gate on them.
    pub fn allowing_approximations(mut self) -> Self {
        self.allow_approximations = true;
        self
    }

    /// Structural completeness: non-empty, every entry has a well-formed source
    /// span, every entry carries native evidence, and every published `ε` is the
    /// one the realization engine derived.
    ///
    /// This is the pure-structure half of the hard CI gate;
    /// [`ProofCertificate::is_certifiable`] adds the native cross-check and the
    /// approximation policy.
    pub fn is_complete(&self) -> bool {
        !self.entries.is_empty()
            && self.entries.iter().all(|e| {
                e.source_location.is_valid() && e.has_native_evidence() && e.epsilon_is_derived()
            })
    }

    /// Every reason this certificate must not be emitted.
    ///
    /// A certificate is only allowed to exist when this is empty: an empty
    /// certificate, an unusable source span, missing native evidence, a
    /// hand-asserted `ε`, or an approximation-discharged conclusion that the
    /// certificate has not explicitly opted into all appear here.
    pub fn blocking_problems(&self) -> Vec<String> {
        let mut problems: Vec<String> = Vec::new();
        if self.entries.is_empty() {
            problems.push("certificate has no entries".to_string());
        }
        for entry in &self.entries {
            if !entry.source_location.is_valid() {
                problems.push(alloc::format!(
                    "{}: source span {}..{} is not a usable reference",
                    entry.function,
                    entry.source_location.line_start,
                    entry.source_location.line_end
                ));
            }
            if !entry.has_native_evidence() {
                problems.push(alloc::format!(
                    "{}: no native tpt-telos verification record",
                    entry.function
                ));
            }
            if !entry.epsilon_is_derived() {
                problems.push(alloc::format!(
                    "{}: published epsilon is not the derived realization bound",
                    entry.function
                ));
            }
            if entry.native_approximations > 0 && !self.allow_approximations {
                problems.push(alloc::format!(
                    "{}: {} conclusion(s) discharged by approximation, which cannot be \
                     certified as exact",
                    entry.function, entry.native_approximations
                ));
            }
        }
        problems
    }

    /// The hard gate: structurally complete *and* free of blocking problems.
    pub fn is_certifiable(&self) -> bool {
        self.is_complete() && self.blocking_problems().is_empty()
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

    /// Whether an entry is covered by a native predicate over `.telos` function
    /// names: either the entry's own function name is native, or every declared
    /// `native_support` function is.
    ///
    /// The second form is how an *artifact* entry (e.g. the shipped
    /// `ray_intersects_aabb`, `Plane::signed_distance`, `Ray::plane_t`) is tied
    /// to the several `.telos` functions whose discharged shapes it relies on.
    fn entry_supported<F>(entry: &CertificateEntry, is_native: F) -> bool
    where
        F: Fn(&str) -> bool,
    {
        let direct = is_native(base_name(&entry.function)) || is_native(entry.function.as_str());
        let supported = !entry.native_support.is_empty()
            && entry
                .native_support
                .iter()
                .all(|name| is_native(base_name(name)));
        direct || supported
    }

    /// Cross-check every certificate entry against a `telos verify --json` report
    /// for the same source: the report must show every entry as passed, and each
    /// entry must be covered either directly or through its declared
    /// `native_support`.
    ///
    /// This is the bridge that works for *every* parseable contract, including
    /// ones the codegen path cannot build (`telos build` fails on functions whose
    /// bodies contain an `if`), and it is the native evidence the Phase 1
    /// certificate is assembled from.
    pub fn is_supported_by_report(&self, report: &verify_report::VerifyReport) -> bool {
        if !report.all_functions_passed() {
            return false;
        }
        self.entries
            .iter()
            .all(|e| Self::entry_supported(e, |name| report.is_verified(name)))
    }

    /// Assemble a certificate for one `.telos` source from the tool's own
    /// `telos verify --json` report, plus the artifact-level entries the caller
    /// supplies (the f64 mirrors with their derived `ε`).
    ///
    /// Hard gate: an empty report, any failed conclusion, a verified function
    /// without a usable source span, an entry the report does not support, or any
    /// [`ProofCertificate::blocking_problems`] entry makes this return `Err`.
    /// There is no partial or best-effort certificate.
    pub fn assemble_from_report(
        report: &verify_report::VerifyReport,
        build_id: String,
        timestamp: String,
        telos_version: String,
        artifact_entries: Vec<CertificateEntry>,
        allow_approximations: bool,
    ) -> Result<Self, AssemblyError> {
        if report.functions.is_empty() {
            return Err(AssemblyError::NoVerifiedFunctions);
        }
        if let Some(failing) = report.failing_function_names().first() {
            return Err(AssemblyError::UnverifiedFunction((*failing).to_string()));
        }

        let mut certificate = ProofCertificate::new(build_id, timestamp, telos_version);
        if allow_approximations {
            certificate = certificate.allowing_approximations();
        }

        for name in report.verified_function_names() {
            let (line_start, line_end) = report
                .source_span(name)
                .ok_or_else(|| AssemblyError::MissingSourceSpan(name.to_string()))?;
            let source_location = SourceLocation::new(report.file.clone(), line_start, line_end)
                .ok_or_else(|| AssemblyError::MissingSourceSpan(name.to_string()))?;
            certificate.entries.push(CertificateEntry {
                function: name.to_string(),
                source_location,
                ideal_contract: report.ensures_texts(name).join(" && "),
                realization_contract: String::new(),
                epsilon: None,
                composition_lemmas: Vec::new(),
                regulatory_objective: RegulatoryObjective::Do178cFormalMethods,
                objective_rows: Do178cTableA5Row::default_for_verified_contract(),
                native_support: Vec::new(),
                native_checks: report.check_count(name),
                native_approximations: report.approximation_count(name),
                realization: None,
            });
        }

        for entry in artifact_entries {
            if !Self::entry_supported(&entry, |name| report.is_verified(name)) {
                return Err(AssemblyError::UnsupportedEntry(entry.function));
            }
            certificate.entries.push(entry);
        }

        if let Some(problem) = certificate.blocking_problems().first() {
            return Err(AssemblyError::NotCertifiable(problem.clone()));
        }
        Ok(certificate)
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

/// Why a certificate could not be assembled from native tool output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssemblyError {
    /// The verification report contained no functions at all.
    NoVerifiedFunctions,
    /// A report entry (function or branch) failed to discharge.
    UnverifiedFunction(String),
    /// A verified function carried no usable source location, so it could not be
    /// referenced as audit evidence.
    MissingSourceSpan(String),
    /// An entry was not covered by the report (neither its own name nor its
    /// declared `native_support`).
    UnsupportedEntry(String),
    /// The assembled certificate is not certifiable; carries its first blocking
    /// problem.
    NotCertifiable(String),
}

impl AssemblyError {
    /// A one-line, audit-facing description.
    pub fn message(&self) -> String {
        match self {
            AssemblyError::NoVerifiedFunctions => {
                "verification report contains no functions".to_string()
            }
            AssemblyError::UnverifiedFunction(name) => {
                alloc::format!("proof obligation not discharged for {name}")
            }
            AssemblyError::MissingSourceSpan(name) => {
                alloc::format!("no usable source span reported for {name}")
            }
            AssemblyError::UnsupportedEntry(name) => {
                alloc::format!("{name} is not supported by the native verification report")
            }
            AssemblyError::NotCertifiable(problem) => {
                alloc::format!("certificate not certifiable: {problem}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A derived realization summary, as the engine produces for the ray-AABB
    /// slab test (hand-built here only because an entry must be able to carry a
    /// summary without re-running the derivation).
    fn derived_summary() -> realization::RealizationSummary {
        realization::RealizationSummary {
            derived_by: realization::ENGINE_ID.into(),
            quantities: vec![
                realization::QuantityBound {
                    name: "t_near".into(),
                    interval: realization::Interval::new(-1.0e-15, 1.0e-15).expect("valid"),
                    epsilon: 2.0e-15,
                },
                realization::QuantityBound {
                    name: "t_far".into(),
                    interval: realization::Interval::new(1.0, 1.0 + 2.0e-15).expect("valid"),
                    epsilon: 2.0e-15,
                },
            ],
            epsilon: 2.0e-15,
            decision: Some(realization::Decision::Indeterminate),
        }
    }

    /// A certificate entry for the shipped artifact: derived `ε`, the native
    /// `.telos` functions that discharge its contract shapes, and the objective
    /// rows it provides evidence for.
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
        .with_realization(derived_summary())
        .with_native_support(
            vec![
                "boundary_min_x".into(),
                "domain_overlap_detected".into(),
                "slab_hit_decided".into(),
                "tighten_t_min".into(),
            ],
            11,
            0,
        )
        .with_objective_rows(vec![
            Do178cTableA5Row::SourceCodeAccurateAndConsistent,
            Do178cTableA5Row::Do333FormalMethodsSupplement,
        ])
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
        // Entry for a function the tool did not prove: neither its own name nor
        // its declared native supporters appear in the manifest.
        let mut entry = sample_entry();
        entry.function = "tpt_certus_spatial::telos::ray_aabb::not_proven".into();
        entry.native_support = vec!["not_proven".into()];
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

    /// A compact but field-complete `telos verify --json` fixture with two
    /// verified functions, one of which is reported per branch.
    fn report_fixture() -> verify_report::VerifyReport {
        verify_report::VerifyReport::parse(
            r#"{
  "file": "crates/tpt-certus-spatial/telos/ray_aabb.telos",
  "passed": true,
  "functions": [
    {"func_name": "boundary_min_x", "all_passed": true, "checks": [
      {"description": "ensures: out.flag == 1", "passed": true, "is_ensures": true, "is_approximation": false, "counterexample": null, "or_group": null, "location": {"line": 81, "column": 17}}
    ]},
    {"func_name": "slab_hit_decided[branch 1]", "all_passed": true, "checks": [
      {"description": "ensures: out.flag == 1", "passed": true, "is_ensures": true, "is_approximation": false, "counterexample": null, "or_group": null, "location": {"line": 98, "column": 17}}
    ]}
  ]
}"#,
        )
        .expect("report parses")
    }

    #[test]
    fn epsilon_must_be_the_derived_bound() {
        let entry = sample_entry();
        assert!(entry.epsilon_is_derived());
        assert_eq!(entry.epsilon, Some(2.0e-15));
        assert!(entry.realization_contract.contains("t_near"));
        assert!(entry.realization_contract.contains("Indeterminate"));
        assert!(entry.realization_contract.contains(realization::ENGINE_ID));

        // A hand-asserted ε does not survive the gate.
        let mut hand_asserted = sample_entry();
        hand_asserted.epsilon = Some(1.0e-9);
        assert!(!hand_asserted.epsilon_is_derived());
        let mut cert = ProofCertificate::new("b".into(), "t".into(), "=0.2.0".into());
        cert.entries.push(hand_asserted);
        assert!(!cert.is_complete());
        assert!(!cert.is_certifiable());
        assert!(cert
            .blocking_problems()
            .iter()
            .any(|p| p.contains("not the derived")));

        // A realization attached without a published ε is equally invalid.
        let mut unpublished = sample_entry();
        unpublished.epsilon = None;
        assert!(!unpublished.epsilon_is_derived());
    }

    #[test]
    fn entries_without_native_evidence_or_with_approximations_block_the_gate() {
        let mut without_evidence = sample_entry();
        without_evidence.native_support = Vec::new();
        without_evidence.native_checks = 0;
        let mut cert = ProofCertificate::new("b".into(), "t".into(), "=0.2.0".into());
        cert.entries.push(without_evidence);
        assert!(!cert.is_certifiable());
        assert!(cert
            .blocking_problems()
            .iter()
            .any(|p| p.contains("no native tpt-telos")));

        let mut approximating = sample_entry();
        approximating.native_approximations = 1;
        let mut cert = ProofCertificate::new("b".into(), "t".into(), "=0.2.0".into());
        cert.entries.push(approximating);
        assert!(cert
            .blocking_problems()
            .iter()
            .any(|p| p.contains("approximation")));
        assert!(
            ProofCertificate::new("b".into(), "t".into(), "=0.2.0".into())
                .allowing_approximations()
                .allow_approximations
        );
    }

    #[test]
    fn objective_rows_name_the_table_and_the_requirement() {
        assert_eq!(
            Do178cTableA5Row::SourceCodeAccurateAndConsistent.table_id(),
            "DO-178C Table A-5"
        );
        assert!(Do178cTableA5Row::SourceCodeAccurateAndConsistent
            .objective()
            .contains("accurate and consistent"));
        assert!(Do178cTableA5Row::Do333FormalMethodsSupplement
            .table_id()
            .contains("DO-333"));
        assert_eq!(Do178cTableA5Row::default_for_verified_contract().len(), 2);
    }

    #[test]
    fn certificate_is_assembled_from_a_verify_report() {
        let report = report_fixture();
        let cert = ProofCertificate::assemble_from_report(
            &report,
            "abc123".into(),
            "2026-09-20T00:00:00Z".into(),
            "=0.2.0".into(),
            vec![sample_entry()],
            false,
        )
        .expect("assembles");

        assert_eq!(cert.entry_count(), 3, "two native entries plus the artifact");
        assert!(cert.is_complete());
        assert!(cert.is_certifiable());
        assert!(cert.blocking_problems().is_empty());
        assert!(cert.is_supported_by_report(&report));

        let native_entry = &cert.entries[0];
        assert_eq!(native_entry.function, "boundary_min_x");
        assert_eq!(native_entry.source_location.line_start, 81);
        assert_eq!(native_entry.source_location.line_end, 81);
        assert_eq!(native_entry.native_checks, 1);
        assert_eq!(native_entry.ideal_contract, "ensures: out.flag == 1");
        assert_eq!(native_entry.objective_rows.len(), 2);
        assert!(native_entry.epsilon.is_none());

        let json = cert.to_json().expect("serializes");
        assert_eq!(json, cert.to_json().expect("serializes"));
        assert!(json.contains("do-178c-table-a5-source-code-accurate-and-consistent"));
        assert!(json.contains("do-333-formal-methods-supplement-to-table-a5"));
    }

    #[test]
    fn assembly_fails_hard_on_unproven_unsupported_or_approximated_input() {
        let report = report_fixture();

        let mut unsupported = sample_entry();
        unsupported.native_support = vec!["not_proven".into()];
        assert_eq!(
            ProofCertificate::assemble_from_report(
                &report,
                "b".into(),
                "t".into(),
                "v".into(),
                vec![unsupported],
                false
            ),
            Err(AssemblyError::UnsupportedEntry(
                "tpt_certus_spatial::ray_aabb::ray_intersects_aabb".into()
            ))
        );

        let mut failing = report_fixture();
        failing.functions[0].all_passed = false;
        assert_eq!(
            ProofCertificate::assemble_from_report(
                &failing,
                "b".into(),
                "t".into(),
                "v".into(),
                Vec::new(),
                false
            ),
            Err(AssemblyError::UnverifiedFunction("boundary_min_x".into()))
        );

        let empty = verify_report::VerifyReport {
            file: "f.telos".into(),
            passed: true,
            functions: Vec::new(),
        };
        assert_eq!(
            ProofCertificate::assemble_from_report(
                &empty,
                "b".into(),
                "t".into(),
                "v".into(),
                Vec::new(),
                false
            ),
            Err(AssemblyError::NoVerifiedFunctions)
        );
        assert!(AssemblyError::NoVerifiedFunctions
            .message()
            .contains("no functions"));

        let mut approximated = report_fixture();
        approximated.functions[0].checks[0].is_approximation = true;
        assert!(matches!(
            ProofCertificate::assemble_from_report(
                &approximated,
                "b".into(),
                "t".into(),
                "v".into(),
                Vec::new(),
                false
            ),
            Err(AssemblyError::NotCertifiable(_))
        ));
        assert!(ProofCertificate::assemble_from_report(
            &approximated,
            "b".into(),
            "t".into(),
            "v".into(),
            Vec::new(),
            true
        )
        .is_ok());
    }
}
