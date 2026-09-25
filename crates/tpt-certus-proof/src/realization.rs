//! Realization-layer `ε` derivation: sound IEEE-754 error bounds over an
//! operation graph.
//!
//! `tpt-telos` verifies the **ideal layer** over exact integer/real atoms and
//! performs no IEEE-754 rounding analysis (its IR is QF_LRA; interval
//! arithmetic exists only for nonlinear *integer* products).  Deriving a sound
//! per-build `ε` bound for the *executing* f64 artifact is therefore
//! `tpt-certus`'s own responsibility (spec §5.1).  This module does it with
//! outward-rounded interval arithmetic over an explicit mirror of the
//! artifact's operation graph.
//!
//! # Soundness model
//!
//! Every IEEE-754 basic operation (`+`, `-`, `*`, `/`) is *correctly rounded*:
//! the stored result differs from the exact real result by at most ½ ulp under
//! round-to-nearest-even ([`sqrt`] is accurate to
//! [`SQRT_RELATIVE_MARGIN`] relative — see `tpt-certus-math`).  Evaluating the
//! mirrored graph on intervals and widening every intermediate endpoint one ulp
//! **outward** ([`Interval::widen_outward`]) therefore yields, by induction over
//! the graph, an envelope `[lo, hi]` that contains
//!
//! * the exact (ideal) result of the real-number algorithm, and
//! * the rounded (f64) result of the shipped artifact,
//!
//! for every input inside the declared domain.  Consequently
//!
//! ```text
//! |f64 result − ideal result| ≤ hi − lo =: ε
//! ```
//!
//! `ε` is reported per output quantity ([`QuantityBound::epsilon`]) and
//! published per build in the Proof Certificate, never hand-asserted.
//!
//! # Generality
//!
//! [`OpGraph`] is a *generic* engine: operations are declared as nodes
//! ([`OpKind`]) and evaluated once.  Three artifacts are mirrored with it
//! today — the ray-AABB slab test ([`slab_realization`], with a hit/miss
//! [`Decision`]), point→plane signed distance
//! ([`point_plane_distance_realization`]), and ray→plane parameter
//! ([`ray_plane_t_realization`]) — which is the Phase 1 exit criterion that the
//! `ε` mechanism generalizes rather than being hand-tuned to one example.
//!
//! # Mirror ↔ artifact discipline
//!
//! A mirror must reproduce the artifact's operation *order* exactly.  Each
//! artifact documents its mirror in its own doc comment
//! (`tpt-certus-geometry::Plane::signed_distance`,
//! `tpt-certus-geometry::Ray::plane_t`,
//! `tpt-certus-spatial::ray_aabb::ray_intersects_aabb`), and
//! `crates/tpt-certus-spatial/tests/mirror_artifact.rs` pins the correspondence
//! over the reference scenarios plus randomized inputs.  Tool-verifying that
//! equivalence (rather than test-pinning it) is Phase 3 audit work.
//!
//! # Domain discipline
//!
//! `ε` is only meaningful over a *bounded, decisive* domain.  A non-finite
//! input, a divisor whose enclosure spans zero, a negative square root, or an
//! intermediate that overflows to infinity all make the derivation return a
//! [`DomainError`] instead of a bound — no `ε` is ever emitted for such a
//! domain.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

pub use tpt_certus_math::{
    next_down, next_up, sqrt_lower_envelope, sqrt_upper_envelope, SQRT_RELATIVE_MARGIN,
};

/// Identity of the engine that produced a bound (DO-330 provenance: an auditor
/// can tell exactly which tool and which widening discipline produced `ε`).
pub const ENGINE_ID: &str =
    "tpt-certus-proof realization v1 (outward-rounded interval arithmetic)";

/// A binary IEEE-754 interval `[lo, hi]` with the invariant `lo <= hi`.
///
/// Endpoints are stored exactly; every operation widens *outward* by one ulp per
/// endpoint, so the interval always contains both the exact real result and the
/// correctly rounded f64 result of the mirrored operation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Interval {
    pub lo: f64,
    pub hi: f64,
}

impl Interval {
    /// Build an interval from raw bounds.  Returns `None` if `lo > hi` or either
    /// bound is NaN (a NaN endpoint would make the interval meaningless).
    pub fn new(lo: f64, hi: f64) -> Option<Self> {
        if lo.is_nan() || hi.is_nan() || lo > hi {
            return None;
        }
        Some(Interval { lo, hi })
    }

    /// A degenerate (point) interval.
    pub fn point(x: f64) -> Self {
        Interval { lo: x, hi: x }
    }

    /// The interval width `hi - lo`.
    pub fn width(&self) -> f64 {
        self.hi - self.lo
    }

    /// Whether both endpoints are finite.
    pub fn is_finite(&self) -> bool {
        self.lo.is_finite() && self.hi.is_finite()
    }

    /// Whether `x` lies within the interval, endpoints inclusive.
    pub fn contains(&self, x: f64) -> bool {
        self.lo <= x && x <= self.hi
    }

    /// Whether `0` lies outside the interval, so division by it is
    /// sign-definite.
    pub fn excludes_zero(&self) -> bool {
        self.lo > 0.0 || self.hi < 0.0
    }

    /// Widen both endpoints one ulp toward `-∞`.
    pub fn widen_down(&self) -> Self {
        Interval {
            lo: next_down(self.lo),
            hi: next_down(self.hi),
        }
    }

    /// Widen both endpoints one ulp toward `+∞`.
    pub fn widen_up(&self) -> Self {
        Interval {
            lo: next_up(self.lo),
            hi: next_up(self.hi),
        }
    }

    /// Widen both endpoints one ulp outward (down for `lo`, up for `hi`).  Used
    /// after an operation whose exact result is within ½ ulp of the stored one.
    pub fn widen_outward(&self) -> Self {
        Interval {
            lo: next_down(self.lo),
            hi: next_up(self.hi),
        }
    }

    /// Interval addition, widened outward.
    pub fn add(&self, rhs: &Self) -> Self {
        Interval {
            lo: next_down(self.lo + rhs.lo),
            hi: next_up(self.hi + rhs.hi),
        }
    }

    /// Interval subtraction, widened outward.
    pub fn sub(&self, rhs: &Self) -> Self {
        Interval {
            lo: next_down(self.lo - rhs.hi),
            hi: next_up(self.hi - rhs.lo),
        }
    }

    /// Interval negation, widened outward.
    pub fn neg(&self) -> Self {
        Interval {
            lo: next_down(-self.hi),
            hi: next_up(-self.lo),
        }
    }

    /// Interval multiplication (four-corner evaluation), widened outward.
    pub fn mul(&self, rhs: &Self) -> Self {
        let lo = next_down(
            (self.lo * rhs.lo)
                .min(self.lo * rhs.hi)
                .min(self.hi * rhs.lo)
                .min(self.hi * rhs.hi),
        );
        let hi = next_up(
            (self.lo * rhs.lo)
                .max(self.lo * rhs.hi)
                .max(self.hi * rhs.lo)
                .max(self.hi * rhs.hi),
        );
        Interval { lo, hi }
    }

    /// Interval division, widened outward.
    ///
    /// The divisor must be sign-definite (`0` not in `[rhs.lo, rhs.hi]`,
    /// inclusive).  A divisor whose interval touches zero returns the whole line
    /// `(-∞, +∞)` — sound, and rejected by [`OpGraph::evaluate`] via
    /// [`DomainError::DivisionSpansZero`] so no `ε` is derived from it.
    pub fn div(&self, rhs: &Self) -> Self {
        if !rhs.excludes_zero() {
            return Interval {
                lo: f64::NEG_INFINITY,
                hi: f64::INFINITY,
            };
        }
        let lo = next_down(
            (self.lo / rhs.lo)
                .min(self.lo / rhs.hi)
                .min(self.hi / rhs.lo)
                .min(self.hi / rhs.hi),
        );
        let hi = next_up(
            (self.lo / rhs.lo)
                .max(self.lo / rhs.hi)
                .max(self.hi / rhs.lo)
                .max(self.hi / rhs.hi),
        );
        Interval { lo, hi }
    }

    /// Element-wise min.
    ///
    /// `min`/`max` of two IEEE-754 values is *exact* (it returns one of the two
    /// inputs unchanged), so no ulp widening is required for soundness.
    pub fn min(&self, rhs: &Self) -> Self {
        Interval {
            lo: self.lo.min(rhs.lo),
            hi: self.hi.min(rhs.hi),
        }
    }

    /// Element-wise max (exact; see [`Interval::min`]).
    pub fn max(&self, rhs: &Self) -> Self {
        Interval {
            lo: self.lo.max(rhs.lo),
            hi: self.hi.max(rhs.hi),
        }
    }
}

/// The operation performed by an [`OpNode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    /// A declared input slot (`a`/`b`/`value` unused); bound by [`EvalDomain`].
    Input,
    /// The literal `value`, taken exactly.
    Constant,
    /// `a + b`.
    Add,
    /// `a - b`.
    Sub,
    /// `a * b`.
    Mul,
    /// `a / b` (requires `b` sign-definite; see [`Interval::div`]).
    Div,
    /// `-a`.
    Neg,
    /// `min(a, b)` (exact, no widening).
    Min,
    /// `max(a, b)` (exact, no widening).
    Max,
    /// `sqrt(a)` (requires `a ≥ 0`; widened by [`SQRT_RELATIVE_MARGIN`]).
    Sqrt,
}

/// One node of an [`OpGraph`]: an operation plus the indices of its operands.
///
/// Node indices are assigned in construction order and an operand must always
/// refer to an *earlier* index, which keeps evaluation a single forward pass.
#[derive(Clone, Debug, PartialEq)]
pub struct OpNode {
    /// Human/audit-facing name (e.g. `t_near`, `N_dot_dir`).
    pub label: String,
    /// The operation.
    pub op: OpKind,
    /// First operand index (unused for [`OpKind::Input`]/[`OpKind::Constant`]).
    pub a: usize,
    /// Second operand index (used by binary ops only).
    pub b: usize,
    /// The literal for [`OpKind::Constant`] (unused otherwise).
    pub value: f64,
}

/// A mirror of an artifact's f64 operation graph, evaluated in interval
/// arithmetic to derive `ε`.
///
/// The graph is built once ([`OpGraph::new`] + the builder methods) and then
/// evaluated against a declared domain ([`OpGraph::evaluate`]).  Nothing here is
/// artifact-specific: the same engine derives the bound for every mirrored
/// function, which is what makes the mechanism general rather than hand-tuned.
#[derive(Clone, Debug, PartialEq)]
pub struct OpGraph {
    nodes: Vec<OpNode>,
    input_count: usize,
}

impl Default for OpGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl OpGraph {
    /// An empty graph.
    pub fn new() -> Self {
        OpGraph {
            nodes: Vec::new(),
            input_count: 0,
        }
    }

    /// Declare an input slot.  Inputs are bound positionally by
    /// [`EvalDomain::inputs`] (in declaration order).
    pub fn input(&mut self, label: &str) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(OpNode {
            label: label.to_string(),
            op: OpKind::Input,
            a: 0,
            b: 0,
            value: 0.0,
        });
        self.input_count += 1;
        idx
    }

    /// A literal constant taken exactly (no rounding, no widening).
    pub fn constant(&mut self, label: &str, value: f64) -> usize {
        self.push_binary(OpKind::Constant, label, 0, 0, value)
    }

    /// `a + b`.
    pub fn add(&mut self, label: &str, a: usize, b: usize) -> usize {
        self.push_binary(OpKind::Add, label, a, b, 0.0)
    }

    /// `a - b`.
    pub fn sub(&mut self, label: &str, a: usize, b: usize) -> usize {
        self.push_binary(OpKind::Sub, label, a, b, 0.0)
    }

    /// `a * b`.
    pub fn mul(&mut self, label: &str, a: usize, b: usize) -> usize {
        self.push_binary(OpKind::Mul, label, a, b, 0.0)
    }

    /// `a / b`.
    pub fn div(&mut self, label: &str, a: usize, b: usize) -> usize {
        self.push_binary(OpKind::Div, label, a, b, 0.0)
    }

    /// `min(a, b)`.
    pub fn min(&mut self, label: &str, a: usize, b: usize) -> usize {
        self.push_binary(OpKind::Min, label, a, b, 0.0)
    }

    /// `max(a, b)`.
    pub fn max(&mut self, label: &str, a: usize, b: usize) -> usize {
        self.push_binary(OpKind::Max, label, a, b, 0.0)
    }

    /// `-a`.
    pub fn neg(&mut self, label: &str, a: usize) -> usize {
        self.push_binary(OpKind::Neg, label, a, a, 0.0)
    }

    /// `sqrt(a)`.
    pub fn sqrt(&mut self, label: &str, a: usize) -> usize {
        self.push_binary(OpKind::Sqrt, label, a, a, 0.0)
    }

    fn push_binary(&mut self, op: OpKind, label: &str, a: usize, b: usize, value: f64) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(OpNode {
            label: label.to_string(),
            op,
            a,
            b,
            value,
        });
        idx
    }

    /// Total node count (inputs included).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of declared input slots.
    pub fn input_count(&self) -> usize {
        self.input_count
    }

    /// The label of a node, or `None` if the index is out of range.
    pub fn node_label(&self, index: usize) -> Option<&str> {
        self.nodes.get(index).map(|n| n.label.as_str())
    }

    /// Evaluate the mirrored graph over `domain`, returning the envelope and
    /// derived `ε` for each node index in `outputs`.
    ///
    /// Fails (only) for domains the artifact's contract does not cover: a
    /// non-finite input, an operand index that does not precede the node, a
    /// divisor whose enclosure spans zero, a negative square root, or an
    /// intermediate that overflows to infinity.  A failure means *no* `ε` is
    /// emitted for that domain — never a wider-but-conservative bound.
    pub fn evaluate(
        &self,
        domain: &EvalDomain,
        outputs: &[usize],
    ) -> Result<GraphRealization, DomainError> {
        if domain.inputs.len() != self.input_count {
            return Err(DomainError::InputCountMismatch {
                expected: self.input_count,
                found: domain.inputs.len(),
            });
        }

        let mut bounds: Vec<Interval> = Vec::with_capacity(self.nodes.len());
        let mut trace: Vec<(String, Interval)> = Vec::with_capacity(self.nodes.len());
        let mut next_input = 0usize;

        for node in &self.nodes {
            let value = match node.op {
                OpKind::Input => {
                    let (name, iv) = &domain.inputs[next_input];
                    if !iv.is_finite() {
                        return Err(DomainError::NotFinite(name.clone()));
                    }
                    next_input += 1;
                    *iv
                }
                OpKind::Constant => {
                    if !node.value.is_finite() {
                        return Err(DomainError::NotFinite(node.label.clone()));
                    }
                    Interval::point(node.value)
                }
                _ => {
                    let a = bounds.get(node.a).copied().ok_or_else(|| {
                        DomainError::ForwardReference(format!("{}->a", node.label))
                    })?;
                    let b = bounds.get(node.b).copied().ok_or_else(|| {
                        DomainError::ForwardReference(format!("{}->b", node.label))
                    })?;
                    match node.op {
                        OpKind::Add => a.add(&b),
                        OpKind::Sub => a.sub(&b),
                        OpKind::Mul => a.mul(&b),
                        OpKind::Neg => a.neg(),
                        OpKind::Min => a.min(&b),
                        OpKind::Max => a.max(&b),
                        OpKind::Div => {
                            let quotient = a.div(&b);
                            if !quotient.is_finite() {
                                return Err(DomainError::DivisionSpansZero(node.label.clone()));
                            }
                            quotient
                        }
                        OpKind::Sqrt => {
                            if a.lo < 0.0 {
                                return Err(DomainError::NegativeSqrt(node.label.clone()));
                            }
                            Interval {
                                lo: sqrt_lower_envelope(a.lo),
                                hi: sqrt_upper_envelope(a.hi),
                            }
                        }
                        OpKind::Input | OpKind::Constant => unreachable!("handled above"),
                    }
                }
            };

            if !value.is_finite() {
                return Err(DomainError::UnboundedResult(node.label.clone()));
            }
            trace.push((node.label.clone(), value));
            bounds.push(value);
        }

        let mut quantities = Vec::with_capacity(outputs.len());
        for &index in outputs {
            let bound = bounds
                .get(index)
                .copied()
                .ok_or_else(|| DomainError::ForwardReference(format!("output index {index}")))?;
            let name = self
                .nodes
                .get(index)
                .map(|n| n.label.clone())
                .ok_or_else(|| DomainError::ForwardReference(format!("output index {index}")))?;
            quantities.push(QuantityBound {
                name,
                interval: bound,
                epsilon: bound.width(),
            });
        }

        Ok(GraphRealization {
            derived_by: ENGINE_ID.to_string(),
            domain: domain.clone(),
            quantities,
            decision: None,
            op_trace: trace,
        })
    }
}

/// The declared input envelope for a graph evaluation: one interval per input
/// slot, in [`OpGraph::input`] declaration order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvalDomain {
    pub inputs: Vec<(String, Interval)>,
}

impl EvalDomain {
    /// Build a domain, rejecting any non-finite input up front (finiteness of
    /// inputs is part of the ideal contract's scope note).
    pub fn new(inputs: Vec<(String, Interval)>) -> Result<Self, DomainError> {
        for (name, iv) in &inputs {
            if !iv.is_finite() {
                return Err(DomainError::NotFinite(name.clone()));
            }
        }
        Ok(EvalDomain { inputs })
    }

    /// The interval bound to `name`, if any.
    pub fn get(&self, name: &str) -> Option<Interval> {
        self.inputs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, iv)| *iv)
    }
}

/// Why a realization was refused.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DomainError {
    /// An input interval is not finite (NaN or infinite).
    NotFinite(String),
    /// A direction component's interval spans (touches) zero, so a slab division
    /// would not be sign-definite.
    DirectionSpansZero(String),
    /// The domain does not bind exactly one interval per graph input slot.
    InputCountMismatch {
        /// Inputs declared by the graph.
        expected: usize,
        /// Intervals supplied by the domain.
        found: usize,
    },
    /// A node referenced an operand index that does not precede it.
    ForwardReference(String),
    /// A division whose divisor interval spans (touches) zero: the quotient is
    /// not sign-definite, so the mirror does not model the artifact's
    /// parallel/fallback path.
    DivisionSpansZero(String),
    /// A square root of an interval that dips below zero.
    NegativeSqrt(String),
    /// An intermediate overflowed to infinity, so `ε` would not be finite.
    UnboundedResult(String),
}

/// One output quantity of a graph evaluation: its envelope and derived `ε`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuantityBound {
    /// Output name (the graph node's label).
    pub name: String,
    /// Sound envelope of the quantity: contains both the exact real result and
    /// the shipped f64 result over the declared domain.
    pub interval: Interval,
    /// `|f64 result − ideal result| ≤ epsilon`; by construction
    /// `interval.width()`.
    pub epsilon: f64,
}

/// The certificate-bound result of evaluating a mirrored operation graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphRealization {
    /// Engine identity — what produced these bounds (DO-330 provenance).
    pub derived_by: String,
    /// The declared input envelope the bounds are valid over.
    pub domain: EvalDomain,
    /// Envelope + `ε` per requested output quantity.
    pub quantities: Vec<QuantityBound>,
    /// Hit/miss classification, for mirrors that model a decision (`None` for
    /// pure numeric mirrors).
    pub decision: Option<Decision>,
    /// Every node's envelope in evaluation order (audit trail of the mirrored
    /// operation graph).
    pub op_trace: Vec<(String, Interval)>,
}

impl GraphRealization {
    /// The bound for a named output quantity.
    pub fn quantity(&self, name: &str) -> Option<&QuantityBound> {
        self.quantities.iter().find(|q| q.name == name)
    }

    /// The largest `ε` across all outputs: the scalar a certificate can publish
    /// as "the" bound for this function over this domain.
    pub fn max_epsilon(&self) -> f64 {
        self.quantities
            .iter()
            .map(|q| q.epsilon)
            .fold(0.0_f64, f64::max)
    }

    /// The certificate-facing summary of this evaluation.
    pub fn summary(&self) -> RealizationSummary {
        RealizationSummary {
            derived_by: self.derived_by.clone(),
            quantities: self.quantities.clone(),
            epsilon: self.max_epsilon(),
            decision: self.decision,
        }
    }
}

/// Whether the realization decision is decisive over the whole domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    /// The artifact returns a hit for every input in the domain.
    GuaranteedHit,
    /// The artifact returns a miss for every input in the domain.
    GuaranteedMiss,
    /// The envelope straddles the decision boundary: both outcomes occur, so the
    /// domain carries no hit/miss obligation.
    Indeterminate,
}

impl Decision {
    /// Whether the artifact is guaranteed to return `Some` over the domain.
    pub fn guarantees_hit(&self) -> bool {
        matches!(self, Decision::GuaranteedHit)
    }

    /// Whether the artifact is guaranteed to return `None` over the domain.
    pub fn guarantees_miss(&self) -> bool {
        matches!(self, Decision::GuaranteedMiss)
    }

    /// The artifact obligation implied by this classification.
    ///
    /// `Some(true)` — the artifact must return `Some` for every input in the
    /// domain; `Some(false)` — it must return `None`; `None` — the envelope is
    /// inconclusive, so nothing is claimed.
    pub fn artifact_obligation(&self) -> Option<bool> {
        match self {
            Decision::GuaranteedHit => Some(true),
            Decision::GuaranteedMiss => Some(false),
            Decision::Indeterminate => None,
        }
    }

    /// The spec §6 realization-layer no-false-negative obligation for one
    /// observed artifact outcome: a decisive domain must agree with the
    /// artifact, while [`Decision::Indeterminate`] claims nothing and therefore
    /// always agrees.
    ///
    /// This is the predicate the mirror↔artifact cross-checks
    /// (`tpt-certus-spatial/tests/mirror_artifact.rs`) evaluate for every
    /// concrete input inside a declared domain.
    pub fn agrees_with_artifact(&self, artifact_hit: bool) -> bool {
        match self.artifact_obligation() {
            Some(expected) => expected == artifact_hit,
            None => true,
        }
    }
}

/// The subset of a realization result that a Proof Certificate publishes.
///
/// `epsilon` is *derived* (the maximum over the output quantities), never
/// hand-asserted, and `derived_by` names the engine so an auditor can identify
/// the tool and widening discipline that produced it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RealizationSummary {
    /// Engine identity ([`ENGINE_ID`]).
    pub derived_by: String,
    /// Envelope + `ε` per output quantity.
    pub quantities: Vec<QuantityBound>,
    /// The published per-build bound: `max(quantities[*].epsilon)`.
    pub epsilon: f64,
    /// The hit/miss obligation, for mirrors that model a decision.
    pub decision: Option<Decision>,
}

/// Axis names, in the order the artifact processes them.
const AXES: [&str; 3] = ["x", "y", "z"];

/// The declared f64 operating envelope for the ray-AABB realization.
///
/// `box_min`/`box_max`/`origin` are per-axis coordinate intervals, `direction`
/// the per-axis ray direction intervals (sign-definite, bounded away from zero),
/// and `t_max` the bounded entry distance used by the hit gate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputDomain {
    pub box_min: [Interval; 3],
    pub box_max: [Interval; 3],
    pub origin: [Interval; 3],
    pub direction: [Interval; 3],
    pub t_max: Interval,
}

impl InputDomain {
    /// Validates the domain: all intervals finite, and each direction component
    /// sign-definite away from zero (`0` excluded).  Returns `None` if any check
    /// fails.
    pub fn new(
        box_min: [Interval; 3],
        box_max: [Interval; 3],
        origin: [Interval; 3],
        direction: [Interval; 3],
        t_max: Interval,
    ) -> Option<Self> {
        let d = InputDomain {
            box_min,
            box_max,
            origin,
            direction,
            t_max,
        };
        if d.check_bounded().is_err() {
            return None;
        }
        Some(d)
    }

    /// Rejects any domain where an input is non-finite or a direction spans
    /// zero, so that the mirrored division is sign-definite and bounded.
    ///
    /// This is the explicit contract-level domain gate; the graph engine would
    /// *also* refuse a zero-spanning divisor at a [`OpKind::Div`] node, so the
    /// domain discipline is enforced twice (once as a declared precondition, once
    /// as an evaluation invariant).
    pub fn check_bounded(&self) -> Result<(), DomainError> {
        for (axis, d) in AXES.iter().zip(self.direction.iter()) {
            if !d.is_finite() {
                return Err(DomainError::NotFinite(format!("direction.{axis}")));
            }
            if !d.excludes_zero() {
                return Err(DomainError::DirectionSpansZero(axis.to_string()));
            }
        }
        for (name, arr) in [
            ("box_min", &self.box_min),
            ("box_max", &self.box_max),
            ("origin", &self.origin),
        ] {
            for (i, iv) in arr.iter().enumerate() {
                if !iv.is_finite() {
                    return Err(DomainError::NotFinite(format!("{name}.{}", AXES[i])));
                }
            }
        }
        if !self.t_max.is_finite() {
            return Err(DomainError::NotFinite("t_max".to_string()));
        }
        Ok(())
    }

    /// The engine-facing domain.  Input order matches [`slab_graph`]'s
    /// declaration order (all `box_min` axes, then `box_max`, `origin`,
    /// `direction`).  `t_max` is not an operand of any mirrored quantity: the
    /// artifact only compares it against `t_near` to decide hit/miss, which the
    /// [`Decision`] classifier models.
    fn eval_domain(&self) -> Result<EvalDomain, DomainError> {
        let mut inputs: Vec<(String, Interval)> = Vec::with_capacity(12);
        for (name, arr) in [
            ("box_min", &self.box_min),
            ("box_max", &self.box_max),
            ("origin", &self.origin),
            ("direction", &self.direction),
        ] {
            for (i, iv) in arr.iter().enumerate() {
                inputs.push((format!("{name}.{}", AXES[i]), *iv));
            }
        }
        EvalDomain::new(inputs)
    }
}

/// The mirrored ray-AABB slab graph plus the node indices of its outputs.
struct SlabGraph {
    graph: OpGraph,
    t_min: usize,
    t_far: usize,
    t_near: usize,
}

/// Build the operation-graph mirror of
/// `tpt_certus_spatial::ray_aabb::ray_intersects_aabb`.
///
/// Operation-for-operation correspondence (per axis, in the artifact's own
/// order):
///
/// ```text
/// t1       = (box_min − origin) / direction
/// t2       = (box_max − origin) / direction
/// axis_min = min(t1, t2)                                // folds into t_min
/// axis_max = max(t1, t2)                                // folds into t_max_slab
/// t_min    = max(axis_min_x, axis_min_y, axis_min_z)
/// t_far    = min(axis_max_x, axis_max_y, axis_max_z)    // artifact: t_max_slab
/// t_near   = max(t_min, 0)
/// ```
///
/// The artifact seeds `t_min = −∞` / `t_max_slab = +∞` and folds with
/// `max`/`min`; those seeds are the identity elements of the folds, so this
/// graph starts each fold at axis `x` instead of carrying an infinite (and
/// therefore non-finite, domain-rejected) accumulator.
fn slab_graph() -> SlabGraph {
    let mut graph = OpGraph::new();

    let mut box_min = [0usize; 3];
    let mut box_max = [0usize; 3];
    let mut origin = [0usize; 3];
    let mut direction = [0usize; 3];
    for i in 0..3 {
        box_min[i] = graph.input(&format!("box_min.{}", AXES[i]));
    }
    for i in 0..3 {
        box_max[i] = graph.input(&format!("box_max.{}", AXES[i]));
    }
    for i in 0..3 {
        origin[i] = graph.input(&format!("origin.{}", AXES[i]));
    }
    for i in 0..3 {
        direction[i] = graph.input(&format!("direction.{}", AXES[i]));
    }

    let mut axis_min = [0usize; 3];
    let mut axis_max = [0usize; 3];
    for i in 0..3 {
        let axis = AXES[i];
        let t1_num = graph.sub(&format!("t1_num_{axis}"), box_min[i], origin[i]);
        let t2_num = graph.sub(&format!("t2_num_{axis}"), box_max[i], origin[i]);
        let t1 = graph.div(&format!("t1_{axis}"), t1_num, direction[i]);
        let t2 = graph.div(&format!("t2_{axis}"), t2_num, direction[i]);
        axis_min[i] = graph.min(&format!("axis_min_{axis}"), t1, t2);
        axis_max[i] = graph.max(&format!("axis_max_{axis}"), t1, t2);
    }

    let mut t_min = axis_min[0];
    let mut t_far = axis_max[0];
    for i in 1..3 {
        t_min = graph.max(&format!("t_min_through_{}", AXES[i]), t_min, axis_min[i]);
        t_far = graph.min(&format!("t_far_through_{}", AXES[i]), t_far, axis_max[i]);
    }

    let zero = graph.constant("zero", 0.0);
    let t_near = graph.max("t_near", t_min, zero);

    SlabGraph {
        graph,
        t_min,
        t_far,
        t_near,
    }
}

/// The certificate-bound result of the ray-AABB realization derivation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Realization {
    /// Engine identity — what produced this bound (DO-330 provenance).
    pub derived_by: String,
    /// The declared input envelope this bound is valid over.
    pub input_domain: InputDomain,
    /// Envelope for the entry distance `t_near`.
    pub t_near_interval: Interval,
    /// Envelope for the unclipped exit distance `t_far` (the artifact's
    /// `t_max_slab`).
    pub t_far_interval: Interval,
    /// `ε` for the entry distance: `|f64 t_near − ideal t_near| ≤ ε`.
    pub epsilon_t_near: f64,
    /// `ε` for the exit distance.
    pub epsilon_t_far: f64,
    /// Hit/miss classification over the whole domain.
    pub decision: Decision,
    /// Every node's envelope in evaluation order (audit trail of the mirrored
    /// operation graph).
    pub op_trace: Vec<(String, Interval)>,
}

impl Realization {
    /// The published scalar bound: the larger of the two per-quantity `ε`s.
    pub fn max_epsilon(&self) -> f64 {
        self.epsilon_t_near.max(self.epsilon_t_far)
    }

    /// The certificate-facing summary of this derivation.
    pub fn summary(&self) -> RealizationSummary {
        RealizationSummary {
            derived_by: self.derived_by.clone(),
            quantities: vec![
                QuantityBound {
                    name: "t_near".to_string(),
                    interval: self.t_near_interval,
                    epsilon: self.epsilon_t_near,
                },
                QuantityBound {
                    name: "t_far".to_string(),
                    interval: self.t_far_interval,
                    epsilon: self.epsilon_t_far,
                },
            ],
            epsilon: self.max_epsilon(),
            decision: Some(self.decision),
        }
    }
}

/// Derive the realization-layer bound (`ε`) for the ray-AABB artifact over a
/// declared bounded domain.
///
/// The envelope mirrors the shipped
/// `tpt_certus_spatial::ray_aabb::ray_intersects_aabb` slab method (see
/// [`slab_graph`]); `ε` for each returned distance is the width of that
/// quantity's envelope, i.e. the guaranteed `|f64 − ideal|` bound over the
/// domain.  Width is deliberately *conservative*: the hull of a division whose
/// exact result range is disconnected still contains it.
///
/// # Decision soundness
///
/// * `GuaranteedHit` — `t_far.lo ≥ t_min.hi ≥ t_min(input)` and
///   `t_far.lo ≥ 0` for every input in the domain, so the artifact always takes
///   the hit branch; `t_near.hi ≤ t_max.lo ≤ t_max(input)` keeps the entry inside
///   the bounded domain.  Hence `Some` for every input.
/// * `GuaranteedMiss` — `t_near.lo > t_max.hi ≥ t_max(input)`, so
///   `t_near(input) > t_max(input)` and the artifact returns `None` on both
///   paths.
/// * `Indeterminate` — the envelope straddles the boundary; no hit/miss claim
///   is made (and [`Decision::agrees_with_artifact`] imposes no obligation).
pub fn slab_realization(domain: &InputDomain) -> Result<Realization, DomainError> {
    domain.check_bounded()?;

    let slab = slab_graph();
    let evaluated =
        slab.graph
            .evaluate(&domain.eval_domain()?, &[slab.t_near, slab.t_far, slab.t_min])?;
    debug_assert_eq!(evaluated.quantities.len(), 3, "three requested outputs");

    let t_near = evaluated.quantities[0].interval;
    let t_far = evaluated.quantities[1].interval;
    let t_min = evaluated.quantities[2].interval;

    let gate_overlap = t_far.lo >= t_min.hi && t_far.lo >= 0.0;
    let entry_within_t_max = t_near.hi <= domain.t_max.lo;
    let decision = if gate_overlap && entry_within_t_max {
        Decision::GuaranteedHit
    } else if t_near.lo > domain.t_max.hi {
        Decision::GuaranteedMiss
    } else {
        Decision::Indeterminate
    };

    Ok(Realization {
        derived_by: ENGINE_ID.to_string(),
        input_domain: domain.clone(),
        t_near_interval: t_near,
        t_far_interval: t_far,
        epsilon_t_near: t_near.width(),
        epsilon_t_far: t_far.width(),
        decision,
        op_trace: evaluated.op_trace,
    })
}

/// Declared envelope for the point→plane signed-distance realization.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaneDistanceDomain {
    /// The query point `p`.
    pub point: [Interval; 3],
    /// A point `q` on the plane.
    pub plane_point: [Interval; 3],
    /// The plane's normal `n` (any non-zero length).
    pub normal: [Interval; 3],
}

impl PlaneDistanceDomain {
    /// Validate finiteness.  Returns `None` for a non-finite component.
    pub fn new(
        point: [Interval; 3],
        plane_point: [Interval; 3],
        normal: [Interval; 3],
    ) -> Option<Self> {
        let d = PlaneDistanceDomain {
            point,
            plane_point,
            normal,
        };
        if d.check_bounded().is_err() {
            return None;
        }
        Some(d)
    }

    /// Finiteness of every component.  A normal that spans zero is *not*
    /// rejected here: [`point_plane_distance_realization`] refuses it at the
    /// mirrored division instead ([`DomainError::DivisionSpansZero`]), keeping
    /// the domain gate tied to the artifact's own `|n| == 0` hazard.
    pub fn check_bounded(&self) -> Result<(), DomainError> {
        for (name, arr) in [
            ("point", &self.point),
            ("plane_point", &self.plane_point),
            ("normal", &self.normal),
        ] {
            for (i, iv) in arr.iter().enumerate() {
                if !iv.is_finite() {
                    return Err(DomainError::NotFinite(format!("{name}.{}", AXES[i])));
                }
            }
        }
        Ok(())
    }

    fn eval_domain(&self) -> Result<EvalDomain, DomainError> {
        let mut inputs: Vec<(String, Interval)> = Vec::with_capacity(9);
        for (name, arr) in [
            ("point", &self.point),
            ("plane_point", &self.plane_point),
            ("normal", &self.normal),
        ] {
            for (i, iv) in arr.iter().enumerate() {
                inputs.push((format!("{name}.{}", AXES[i]), *iv));
            }
        }
        EvalDomain::new(inputs)
    }
}

/// Derive `ε` for `tpt_certus_geometry::Plane::signed_distance`:
/// `((p − q) · n) / |n|`.
///
/// Mirrors the artifact's operation order exactly — component-wise subtraction,
/// the left-associated dot product `((d·n)₀ + (d·n)₁) + (d·n)₂`, the same
/// association for `|n|²`, then the shared `tpt_certus_math::sqrt` and one
/// division.  This is the second of the "at least two further functions" the
/// Phase 1 exit criteria require, and evidence that the derivation is
/// engine-driven rather than hand-tuned to the slab test.
pub fn point_plane_distance_realization(
    domain: &PlaneDistanceDomain,
) -> Result<GraphRealization, DomainError> {
    domain.check_bounded()?;

    let graph = plane_distance_graph();
    graph
        .graph
        .evaluate(&domain.eval_domain()?, &[graph.distance, graph.length])
}

/// The mirrored operation graph for `Plane::signed_distance`.
struct PlaneDistanceGraph {
    graph: OpGraph,
    /// The `signed_distance` output node.
    distance: usize,
    /// The `|normal|` output node (published so an auditor can see the
    /// intermediate envelope as well).
    length: usize,
}

fn plane_distance_graph() -> PlaneDistanceGraph {
    let mut graph = OpGraph::new();
    let mut point = [0usize; 3];
    let mut plane_point = [0usize; 3];
    let mut normal = [0usize; 3];
    for i in 0..3 {
        point[i] = graph.input(&format!("point.{}", AXES[i]));
    }
    for i in 0..3 {
        plane_point[i] = graph.input(&format!("plane_point.{}", AXES[i]));
    }
    for i in 0..3 {
        normal[i] = graph.input(&format!("normal.{}", AXES[i]));
    }

    // `(p - q).dot(n)`, in the artifact's left-to-right association.
    let delta = [
        graph.sub("delta_x", point[0], plane_point[0]),
        graph.sub("delta_y", point[1], plane_point[1]),
        graph.sub("delta_z", point[2], plane_point[2]),
    ];
    let terms = [
        graph.mul("delta_x_times_n_x", delta[0], normal[0]),
        graph.mul("delta_y_times_n_y", delta[1], normal[1]),
        graph.mul("delta_z_times_n_z", delta[2], normal[2]),
    ];
    let partial = graph.add("dot_xy", terms[0], terms[1]);
    let dot = graph.add("dot", partial, terms[2]);

    // `n.length()`: the same association on `n·n`, then the shared sqrt.
    let squares = [
        graph.mul("n_x_squared", normal[0], normal[0]),
        graph.mul("n_y_squared", normal[1], normal[1]),
        graph.mul("n_z_squared", normal[2], normal[2]),
    ];
    let len_partial = graph.add("length_squared_xy", squares[0], squares[1]);
    let length_squared = graph.add("length_squared", len_partial, squares[2]);
    let length = graph.sqrt("normal_length", length_squared);

    let distance = graph.div("signed_distance", dot, length);

    PlaneDistanceGraph {
        graph,
        distance,
        length,
    }
}

/// Declared envelope for the ray→plane parameter realization.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RayPlaneDomain {
    /// The ray origin `o`.
    pub origin: [Interval; 3],
    /// The ray direction `d`.
    pub direction: [Interval; 3],
    /// A point `q` on the plane.
    pub plane_point: [Interval; 3],
    /// The plane's normal `n` (any non-zero length).
    pub normal: [Interval; 3],
}

impl RayPlaneDomain {
    /// Validate finiteness.  Returns `None` for a non-finite component.
    pub fn new(
        origin: [Interval; 3],
        direction: [Interval; 3],
        plane_point: [Interval; 3],
        normal: [Interval; 3],
    ) -> Option<Self> {
        let d = RayPlaneDomain {
            origin,
            direction,
            plane_point,
            normal,
        };
        if d.check_bounded().is_err() {
            return None;
        }
        Some(d)
    }

    /// Finiteness of every component.  A `dir · normal` that spans zero is
    /// refused by the mirrored division ([`DomainError::DivisionSpansZero`]),
    /// exactly matching the artifact's parallel-ray `None` branch.
    pub fn check_bounded(&self) -> Result<(), DomainError> {
        for (name, arr) in [
            ("origin", &self.origin),
            ("direction", &self.direction),
            ("plane_point", &self.plane_point),
            ("normal", &self.normal),
        ] {
            for (i, iv) in arr.iter().enumerate() {
                if !iv.is_finite() {
                    return Err(DomainError::NotFinite(format!("{name}.{}", AXES[i])));
                }
            }
        }
        Ok(())
    }

    fn eval_domain(&self) -> Result<EvalDomain, DomainError> {
        let mut inputs: Vec<(String, Interval)> = Vec::with_capacity(12);
        for (name, arr) in [
            ("origin", &self.origin),
            ("direction", &self.direction),
            ("plane_point", &self.plane_point),
            ("normal", &self.normal),
        ] {
            for (i, iv) in arr.iter().enumerate() {
                inputs.push((format!("{name}.{}", AXES[i]), *iv));
            }
        }
        EvalDomain::new(inputs)
    }
}

/// Derive `ε` for `tpt_certus_geometry::Ray::plane_t`:
/// `((q − o) · n) / (d · n)`.
///
/// Third mirrored artifact, with the artifact's exact operation order
/// (numerator `(q − o) · n` and denominator `d · n`, each left-associated, then
/// one division).  Here the domain discipline is entirely engine-enforced: a
/// direction parallel to the plane makes the denominator span zero and the whole
/// derivation is refused with no `ε`.
pub fn ray_plane_t_realization(
    domain: &RayPlaneDomain,
) -> Result<GraphRealization, DomainError> {
    domain.check_bounded()?;

    let graph = ray_plane_graph();
    graph.graph.evaluate(&domain.eval_domain()?, &[graph.t])
}

/// The mirrored operation graph for `Ray::plane_t`.
struct RayPlaneGraph {
    graph: OpGraph,
    /// The `t` output node.
    t: usize,
}

fn ray_plane_graph() -> RayPlaneGraph {
    let mut graph = OpGraph::new();
    let mut origin = [0usize; 3];
    let mut direction = [0usize; 3];
    let mut plane_point = [0usize; 3];
    let mut normal = [0usize; 3];
    for i in 0..3 {
        origin[i] = graph.input(&format!("origin.{}", AXES[i]));
    }
    for i in 0..3 {
        direction[i] = graph.input(&format!("direction.{}", AXES[i]));
    }
    for i in 0..3 {
        plane_point[i] = graph.input(&format!("plane_point.{}", AXES[i]));
    }
    for i in 0..3 {
        normal[i] = graph.input(&format!("normal.{}", AXES[i]));
    }

    // Numerator: `(q - o).dot(n)`.
    let delta = [
        graph.sub("delta_x", plane_point[0], origin[0]),
        graph.sub("delta_y", plane_point[1], origin[1]),
        graph.sub("delta_z", plane_point[2], origin[2]),
    ];
    let num_terms = [
        graph.mul("delta_x_times_n_x", delta[0], normal[0]),
        graph.mul("delta_y_times_n_y", delta[1], normal[1]),
        graph.mul("delta_z_times_n_z", delta[2], normal[2]),
    ];
    let num_partial = graph.add("numerator_xy", num_terms[0], num_terms[1]);
    let numerator = graph.add("numerator", num_partial, num_terms[2]);

    // Denominator: `d.dot(n)`.
    let den_terms = [
        graph.mul("dir_x_times_n_x", direction[0], normal[0]),
        graph.mul("dir_y_times_n_y", direction[1], normal[1]),
        graph.mul("dir_z_times_n_z", direction[2], normal[2]),
    ];
    let den_partial = graph.add("denominator_xy", den_terms[0], den_terms[1]);
    let denominator = graph.add("denominator", den_partial, den_terms[2]);

    let t = graph.div("t", numerator, denominator);

    RayPlaneGraph { graph, t }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iv(lo: f64, hi: f64) -> Interval {
        Interval::new(lo, hi).expect("valid interval")
    }

    fn sample_domain() -> InputDomain {
        // A ray whose envelope crosses the box with positive, bounded
        // directions; the entry is inside the t_max bound for every input.
        InputDomain::new(
            [iv(1.0, 2.0), iv(1.0, 2.0), iv(1.0, 2.0)],
            [iv(3.0, 4.0), iv(3.0, 4.0), iv(3.0, 4.0)],
            [iv(-10.0, 0.0), iv(-10.0, 0.0), iv(-10.0, 0.0)],
            [iv(1.0, 2.0), iv(1.0, 2.0), iv(1.0, 2.0)],
            iv(50.0, 60.0),
        )
        .expect("valid domain")
    }

    #[test]
    fn point_interval_contains_point() {
        let p = Interval::point(2.5);
        assert!(p.contains(2.5));
        assert_eq!(p.width(), 0.0);
    }

    #[test]
    fn new_rejects_inverse_and_nan() {
        assert!(Interval::new(2.0, 1.0).is_none());
        assert!(Interval::new(f64::NAN, 1.0).is_none());
    }

    #[test]
    fn addition_widens_outward() {
        // 1/10 and 2/10 are not exact in binary; the envelope must strictly
        // contain the exact real sum around the rounded one.
        let a = iv(0.1, 0.1);
        let b = iv(0.2, 0.2);
        let s = a.add(&b);
        assert!(s.contains(0.1f64 + 0.2f64));
        assert!(s.lo < 0.30000000000000004 || s.hi > 0.30000000000000004);
    }

    #[test]
    fn division_brackets_one_third() {
        let q = iv(1.0, 1.0).div(&iv(3.0, 3.0));
        assert!(q.contains(1.0 / 3.0));
        assert!(q.is_finite());
    }

    #[test]
    fn division_spans_zero_is_whole_line() {
        let q = iv(1.0, 2.0).div(&iv(-1.0, 1.0));
        assert!(q.lo == f64::NEG_INFINITY && q.hi == f64::INFINITY);
        assert!(!q.is_finite());
    }

    #[test]
    fn division_by_negative_denominator() {
        let q = iv(1.0, 2.0).div(&iv(-2.0, -1.0));
        assert!(q.contains(-2.0));
        assert!(q.contains(-1.0));
        assert!(q.lo <= -2.0 && q.hi >= -0.5);
    }

    #[test]
    fn multiply_four_corners() {
        let p = iv(-2.0, 3.0).mul(&iv(4.0, 5.0));
        assert!(p.contains(-8.0));
        assert!(p.contains(15.0));
        assert!(p.lo <= -8.0 && p.hi >= 15.0);
    }

    #[test]
    fn min_max_hull() {
        let a = iv(-5.0, -1.0);
        let b = iv(-3.0, -2.0);
        let lo = a.min(&b);
        let hi = a.max(&b);
        assert!(lo.lo <= -5.0 && lo.hi <= -1.0);
        assert!(hi.lo >= -3.0 && hi.hi >= -1.0);
    }

    #[test]
    fn sample_domain_yields_bounded_epsilon_and_never_a_false_miss() {
        let r = slab_realization(&sample_domain()).expect("bounded domain");
        // Hull-based interval division makes the canonical sample genuinely
        // inconclusive, but the engine must never claim a guaranteed miss for a
        // domain the artifact hits, and must still bound the envelope with a
        // finite, strictly positive ε.
        assert_ne!(r.decision, Decision::GuaranteedMiss);
        assert!(r.decision.agrees_with_artifact(true));
        assert!(r.epsilon_t_near.is_finite());
        assert!(r.epsilon_t_far.is_finite());
        assert!(r.epsilon_t_near > 0.0);
        assert!(r.t_near_interval.is_finite());
        assert!(r.t_far_interval.is_finite());
        assert_eq!(r.max_epsilon(), r.epsilon_t_near.max(r.epsilon_t_far));
        assert!(r.t_near_interval.contains(0.0) || r.t_near_interval.lo >= 0.0);
    }

    #[test]
    fn graph_evaluates_and_bounds_a_small_expression() {
        // (a - b) / 2 with a, b in [0, 1] → hull [-0.5, 0.5] (plus rounding).
        let mut g = OpGraph::new();
        let a = g.input("a");
        let b = g.input("b");
        let c = g.constant("c", 2.0);
        let d = g.sub("a_minus_b", a, b);
        let q = g.div("q", d, c);
        let domain = EvalDomain::new(vec![
            ("a".to_string(), iv(0.0, 1.0)),
            ("b".to_string(), iv(0.0, 1.0)),
        ])
        .expect("finite domain");

        let out = g.evaluate(&domain, &[q]).expect("bounded");
        let bound = out.quantity("q").expect("named output");
        assert!(bound.interval.lo <= -0.5 && bound.interval.hi >= 0.5);
        assert!(bound.epsilon > 0.0 && bound.epsilon.is_finite());
        assert_eq!(bound.epsilon, out.max_epsilon());
        assert_eq!(out.derived_by, ENGINE_ID);
        assert_eq!(out.op_trace.len(), g.node_count());
        assert!(out.decision.is_none(), "pure numeric mirror has no decision");
        assert_eq!(out.summary().epsilon, out.max_epsilon());
    }

    #[test]
    fn graph_rejects_zero_spanning_divisor_and_negative_sqrt() {
        let mut g = OpGraph::new();
        let a = g.input("a");
        let b = g.input("b");
        let q = g.div("quotient", a, b);
        let domain = EvalDomain::new(vec![
            ("a".to_string(), iv(1.0, 2.0)),
            ("b".to_string(), iv(-1.0, 1.0)),
        ])
        .expect("finite domain");
        assert!(matches!(
            g.evaluate(&domain, &[q]),
            Err(DomainError::DivisionSpansZero(name)) if name == "quotient"
        ));

        let mut g2 = OpGraph::new();
        let x = g2.input("x");
        let root = g2.sqrt("root", x);
        let negative = EvalDomain::new(vec![("x".to_string(), iv(-1.0, 2.0))]).expect("finite");
        assert!(matches!(
            g2.evaluate(&negative, &[root]),
            Err(DomainError::NegativeSqrt(name)) if name == "root"
        ));
    }

    #[test]
    fn graph_rejects_non_finite_input_and_unbounded_result() {
        assert!(matches!(
            EvalDomain::new(vec![("x".to_string(), iv(f64::NEG_INFINITY, 1.0))]),
            Err(DomainError::NotFinite(name)) if name == "x"
        ));

        let mut g = OpGraph::new();
        let a = g.input("a");
        let b = g.input("b");
        let product = g.mul("product", a, b);
        let huge = EvalDomain::new(vec![
            ("a".to_string(), iv(1e300, 1e300)),
            ("b".to_string(), iv(1e300, 1e300)),
        ])
        .expect("finite domain");
        assert!(matches!(
            g.evaluate(&huge, &[product]),
            Err(DomainError::UnboundedResult(name)) if name == "product"
        ));
    }

    #[test]
    fn graph_requires_one_interval_per_input_and_valid_outputs() {
        let mut g = OpGraph::new();
        let a = g.input("a");
        let b = g.input("b");
        let sum = g.add("sum", a, b);

        let short = EvalDomain::new(vec![("a".to_string(), iv(0.0, 1.0))]).expect("finite");
        assert!(matches!(
            g.evaluate(&short, &[sum]),
            Err(DomainError::InputCountMismatch {
                expected: 2,
                found: 1
            })
        ));

        let full = EvalDomain::new(vec![
            ("a".to_string(), iv(0.0, 1.0)),
            ("b".to_string(), iv(0.0, 1.0)),
        ])
        .expect("finite");
        assert!(matches!(
            g.evaluate(&full, &[99]),
            Err(DomainError::ForwardReference(_))
        ));
        assert_eq!(
            g.evaluate(&full, &[sum]).expect("bounded").quantities[0].name,
            "sum"
        );
        assert_eq!(g.node_label(sum), Some("sum"));
        assert_eq!(g.node_label(99), None);
        assert_eq!(g.input_count(), 2);
        assert_eq!(g.node_count(), 3);
        assert_eq!(OpGraph::default().node_count(), 0);
        assert_eq!(full.get("a"), Some(iv(0.0, 1.0)));
        assert_eq!(full.get("missing"), None);
    }

    #[test]
    fn decision_is_decisive_where_the_envelope_is() {
        // Origin at the centre of the unit box with an axis-aligned unit
        // direction and t_max = 1: every quantity is a point interval plus the
        // outward ulp widening, so the hit/miss classification is decidable.
        let d = InputDomain::new(
            [iv(-1.0, -1.0); 3],
            [iv(1.0, 1.0); 3],
            [iv(0.0, 0.0); 3],
            [iv(1.0, 1.0); 3],
            iv(1.0, 1.0),
        )
        .expect("valid domain");
        let r = slab_realization(&d).expect("bounded");

        assert_eq!(r.decision, Decision::GuaranteedHit);
        assert!(r.decision.guarantees_hit() && !r.decision.guarantees_miss());
        assert_eq!(r.decision.artifact_obligation(), Some(true));
        assert!(r.decision.agrees_with_artifact(true));
        assert!(
            !r.decision.agrees_with_artifact(false),
            "a GuaranteedHit domain that misses is a false negative"
        );
        assert!(r.t_near_interval.contains(0.0));
    }

    #[test]
    fn decision_guarantees_miss_when_entry_is_beyond_the_bounded_domain() {
        // Box at [2, 3]^3, origin at (−5, −5, −5), unit direction: the entry
        // distance is in [7, 8] while t_max = 1, so no input can hit.
        let d = InputDomain::new(
            [iv(2.0, 2.0); 3],
            [iv(3.0, 3.0); 3],
            [iv(-5.0, -5.0); 3],
            [iv(1.0, 1.0); 3],
            iv(1.0, 1.0),
        )
        .expect("valid domain");
        let r = slab_realization(&d).expect("bounded");

        assert_eq!(r.decision, Decision::GuaranteedMiss);
        assert!(r.decision.guarantees_miss());
        assert_eq!(r.decision.artifact_obligation(), Some(false));
        assert!(r.decision.agrees_with_artifact(false));
        assert!(
            !r.decision.agrees_with_artifact(true),
            "a GuaranteedMiss domain that hits would break the envelope"
        );
        assert!(r.t_near_interval.lo > 1.0);
    }

    #[test]
    fn indeterminate_domains_impose_no_obligation() {
        let r = slab_realization(&sample_domain()).expect("bounded");
        assert_eq!(r.decision, Decision::Indeterminate);
        assert_eq!(r.decision.artifact_obligation(), None);
        assert!(r.decision.agrees_with_artifact(true));
        assert!(r.decision.agrees_with_artifact(false));
    }

    #[test]
    fn slab_domain_rejects_zero_spanning_direction_and_non_finite_input() {
        assert!(InputDomain::new(
            [iv(-1.0, -1.0); 3],
            [iv(1.0, 1.0); 3],
            [iv(0.0, 0.0); 3],
            [iv(-1.0, 1.0), iv(1.0, 1.0), iv(1.0, 1.0)],
            iv(1.0, 1.0),
        )
        .is_none());

        let bad = InputDomain {
            box_min: [iv(-1.0, -1.0); 3],
            box_max: [iv(1.0, 1.0); 3],
            origin: [iv(0.0, 0.0); 3],
            direction: [iv(-1.0, 1.0), iv(1.0, 1.0), iv(1.0, 1.0)],
            t_max: iv(1.0, 1.0),
        };
        assert!(matches!(
            bad.check_bounded(),
            Err(DomainError::DirectionSpansZero(axis)) if axis == "x"
        ));

        let infinite = InputDomain {
            t_max: iv(f64::INFINITY, f64::INFINITY),
            direction: [iv(1.0, 1.0); 3],
            ..bad
        };
        assert!(matches!(
            infinite.check_bounded(),
            Err(DomainError::NotFinite(name)) if name == "t_max"
        ));
    }

    /// A wide plane-distance domain: `p ∈ [1,2]³`, plane through the origin,
    /// `n ∈ [1,2] × [−1,1] × [3,4]` (non-zero on every axis collectively).
    fn plane_domain() -> PlaneDistanceDomain {
        PlaneDistanceDomain::new(
            [iv(1.0, 2.0); 3],
            [iv(0.0, 0.0); 3],
            [iv(1.0, 2.0), iv(-1.0, 1.0), iv(3.0, 4.0)],
        )
        .expect("valid domain")
    }

    /// A wide ray→plane domain: ray from `(-5, -5, -5)`, direction `[1,2]³`,
    /// plane `x = 0` (so `dir · n = dir.x ∈ [1, 2]`, sign-definite).
    fn ray_plane_domain() -> RayPlaneDomain {
        RayPlaneDomain::new(
            [iv(-5.0, -5.0); 3],
            [iv(1.0, 2.0); 3],
            [iv(0.0, 0.0); 3],
            [iv(1.0, 1.0), iv(0.0, 0.0), iv(0.0, 0.0)],
        )
        .expect("valid domain")
    }

    #[test]
    fn derivation_generalizes_beyond_the_slab_test() {
        // Phase 1 exit criterion 4: the same engine derives ε for more than one
        // function, so the mechanism is not hand-tuned to ray-AABB.
        let slab = slab_realization(&sample_domain()).expect("bounded");
        let plane = point_plane_distance_realization(&plane_domain()).expect("bounded");
        let ray_plane = ray_plane_t_realization(&ray_plane_domain()).expect("bounded");

        for derived_by in [
            slab.derived_by.as_str(),
            plane.derived_by.as_str(),
            ray_plane.derived_by.as_str(),
        ] {
            assert_eq!(derived_by, ENGINE_ID);
        }
        assert!(plane.max_epsilon() > 0.0 && plane.max_epsilon().is_finite());
        assert!(ray_plane.max_epsilon() > 0.0 && ray_plane.max_epsilon().is_finite());
        assert!(plane.quantity("signed_distance").is_some());
        assert!(plane.quantity("normal_length").is_some());
        assert!(ray_plane.quantity("t").is_some());
        assert_eq!(plane.summary().derived_by, ENGINE_ID);
        assert_eq!(ray_plane.summary().decision, None);
        assert!(slab.summary().decision.is_some());
    }

    #[test]
    fn plane_distance_envelope_contains_artifact_samples() {
        let realization = point_plane_distance_realization(&plane_domain()).expect("bounded");
        let bound = realization
            .quantity("signed_distance")
            .expect("named output");

        for (p, q, n) in [
            ((1.5, 1.5, 1.5), (0.0, 0.0, 0.0), (1.5, 0.5, 3.5)),
            ((1.0, 2.0, 1.0), (0.0, 0.0, 0.0), (2.0, -1.0, 4.0)),
            ((2.0, 1.0, 2.0), (0.0, 0.0, 0.0), (1.0, 1.0, 3.0)),
        ] {
            // The artifact's own expression order (see
            // `tpt_certus_geometry::Plane::signed_distance`).
            let delta = (p.0 - q.0, p.1 - q.1, p.2 - q.2);
            let dot = delta.0 * n.0 + delta.1 * n.1 + delta.2 * n.2;
            let length = tpt_certus_math::sqrt(n.0 * n.0 + n.1 * n.1 + n.2 * n.2);
            let artifact = dot / length;
            assert!(
                bound.interval.contains(artifact),
                "artifact value {artifact} outside envelope {:?}",
                bound.interval
            );
        }
    }

    #[test]
    fn epsilon_is_pure_rounding_for_a_point_domain() {
        // With point inputs the envelope width is rounding only, which is the
        // regime ε is meant to quantify (a declared operating envelope).
        let plane = point_plane_distance_realization(
            &PlaneDistanceDomain::new(
                [iv(1.5, 1.5); 3],
                [iv(0.0, 0.0); 3],
                [iv(1.5, 1.5), iv(0.5, 0.5), iv(3.5, 3.5)],
            )
            .expect("valid"),
        )
        .expect("bounded");
        let distance = plane.quantity("signed_distance").expect("named");
        let squared = 1.5 * 1.5 + 0.5 * 0.5 + 3.5 * 3.5;
        let expected = (1.5 * 1.5 + 1.5 * 0.5 + 1.5 * 3.5) / tpt_certus_math::sqrt(squared);
        assert!(distance.interval.contains(expected));
        assert!(
            distance.epsilon > 0.0 && distance.epsilon < 1e-12,
            "point-domain ε should be rounding-scale, got {}",
            distance.epsilon
        );

        let ray_plane = ray_plane_t_realization(
            &RayPlaneDomain::new(
                [iv(-5.0, -5.0); 3],
                [iv(1.0, 1.0); 3],
                [iv(0.0, 0.0); 3],
                [iv(1.0, 1.0), iv(0.0, 0.0), iv(0.0, 0.0)],
            )
            .expect("valid"),
        )
        .expect("bounded");
        let t = ray_plane.quantity("t").expect("named");
        assert!(t.interval.contains(5.0));
        assert!(t.epsilon > 0.0 && t.epsilon < 1e-12);

        let slab = slab_realization(
            &InputDomain::new(
                [iv(-1.0, -1.0); 3],
                [iv(1.0, 1.0); 3],
                [iv(0.0, 0.0); 3],
                [iv(1.0, 1.0); 3],
                iv(1.0, 1.0),
            )
            .expect("valid"),
        )
        .expect("bounded");
        assert!(slab.max_epsilon() > 0.0 && slab.max_epsilon() < 1e-12);
    }

    #[test]
    fn ray_plane_rejects_a_direction_parallel_to_the_plane() {
        // dir · normal = 0 for every input in this domain: mirroring the
        // artifact's `None` branch is impossible, so no ε is emitted.
        let parallel = RayPlaneDomain::new(
            [iv(0.0, 0.0); 3],
            [iv(0.0, 0.0), iv(0.0, 0.0), iv(1.0, 1.0)],
            [iv(1.0, 1.0); 3],
            [iv(1.0, 1.0), iv(0.0, 0.0), iv(0.0, 0.0)],
        )
        .expect("finite domain");
        assert!(matches!(
            ray_plane_t_realization(&parallel),
            Err(DomainError::DivisionSpansZero(name)) if name == "t"
        ));
    }
}


