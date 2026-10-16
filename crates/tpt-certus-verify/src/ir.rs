//! A straight-line expression graph with comparisons and `Select`.
//!
//! One [`Graph`] is the single source of truth for a verified function.  It can
//! be evaluated two ways:
//!
//! * [`Graph::eval_f64`] — IEEE-754 `f64`, operation for operation, which is
//!   what the executing artifact computes.
//! * [`Graph::eval_exact`] — exact rationals, fail-closed: any overflow,
//!   division by zero, or infinity yields `None` rather than an approximation.
//!
//! Branching is expressed with [`Op::Select`] over a [`Cond`], so a function
//! written with `if` becomes a graph with no control flow.  Both evaluators
//! compute every node; a `Select` only chooses which value flows out.  The
//! exact evaluator therefore also rejects a division by zero in an untaken
//! branch.  That is deliberate: it keeps the exact semantics total and simple.

use alloc::string::String;
use alloc::vec::Vec;

use crate::rational::Rational;

/// Index of a node in a [`Graph`].
pub type NodeId = usize;

/// A comparison between two nodes.  `Gt`/`Ge` are written by swapping operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    /// `a < b`.
    Lt,
    /// `a ≤ b`.
    Le,
    /// `a = b`.
    Eq,
    /// `a ≠ b`.
    Ne,
}

/// A branch condition `a <op> b`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cond {
    /// The comparison.
    pub op: Cmp,
    /// Left operand.
    pub a: NodeId,
    /// Right operand.
    pub b: NodeId,
}

/// The operation a node performs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    /// The `i`-th declared input.
    Input(usize),
    /// An exact rational literal.
    Const(Rational),
    /// `+∞` (`true`) or `−∞` (`false`).  Only the `f64` evaluator accepts it.
    Infinity(bool),
    /// `a + b`.
    Add(NodeId, NodeId),
    /// `a − b`.
    Sub(NodeId, NodeId),
    /// `a · b`.
    Mul(NodeId, NodeId),
    /// `a / b`.
    Div(NodeId, NodeId),
    /// `min(a, b)`.
    Min(NodeId, NodeId),
    /// `max(a, b)`.
    Max(NodeId, NodeId),
    /// `−a`.
    Neg(NodeId),
    /// `then` if `cond` holds, otherwise `otherwise`.
    Select {
        /// The guard.
        cond: Cond,
        /// Value when the guard holds.
        then: NodeId,
        /// Value when it does not.
        otherwise: NodeId,
    },
}

/// One labelled node.  The label is for audit output only.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Human/audit-facing name.
    pub label: String,
    /// The operation.
    pub op: Op,
}

/// A straight-line graph.  Operands always refer to earlier nodes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    nodes: Vec<Node>,
    inputs: usize,
}

impl Graph {
    /// An empty graph.
    pub fn new() -> Self {
        Graph::default()
    }

    /// Number of declared inputs.
    pub fn input_count(&self) -> usize {
        self.inputs
    }

    /// Total node count.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The node at `id`, if any.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    fn push(&mut self, label: &str, op: Op) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            label: String::from(label),
            op,
        });
        id
    }

    /// Declare the next input.
    pub fn input(&mut self, label: &str) -> NodeId {
        let i = self.inputs;
        self.inputs += 1;
        self.push(label, Op::Input(i))
    }

    /// An exact rational constant.
    pub fn constant(&mut self, label: &str, value: Rational) -> NodeId {
        self.push(label, Op::Const(value))
    }

    /// `+∞` or `−∞`.
    pub fn infinity(&mut self, label: &str, positive: bool) -> NodeId {
        self.push(label, Op::Infinity(positive))
    }

    /// `a + b`.
    pub fn add(&mut self, label: &str, a: NodeId, b: NodeId) -> NodeId {
        self.push(label, Op::Add(a, b))
    }

    /// `a − b`.
    pub fn sub(&mut self, label: &str, a: NodeId, b: NodeId) -> NodeId {
        self.push(label, Op::Sub(a, b))
    }

    /// `a · b`.
    pub fn mul(&mut self, label: &str, a: NodeId, b: NodeId) -> NodeId {
        self.push(label, Op::Mul(a, b))
    }

    /// `a / b`.
    pub fn div(&mut self, label: &str, a: NodeId, b: NodeId) -> NodeId {
        self.push(label, Op::Div(a, b))
    }

    /// `min(a, b)`.
    pub fn min(&mut self, label: &str, a: NodeId, b: NodeId) -> NodeId {
        self.push(label, Op::Min(a, b))
    }

    /// `max(a, b)`.
    pub fn max(&mut self, label: &str, a: NodeId, b: NodeId) -> NodeId {
        self.push(label, Op::Max(a, b))
    }

    /// `−a`.
    pub fn neg(&mut self, label: &str, a: NodeId) -> NodeId {
        self.push(label, Op::Neg(a))
    }

    /// `then` if `cond`, else `otherwise`.
    pub fn select(&mut self, label: &str, cond: Cond, then: NodeId, otherwise: NodeId) -> NodeId {
        self.push(label, Op::Select { cond, then, otherwise })
    }

    /// Evaluate every node in IEEE-754 `f64`.  Returns one value per node, or
    /// `None` if `inputs` has the wrong length or an operand is out of range.
    pub fn eval_f64(&self, inputs: &[f64]) -> Option<Vec<f64>> {
        if inputs.len() != self.inputs {
            return None;
        }
        let mut v: Vec<f64> = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let x = match node.op {
                Op::Input(i) => inputs[i],
                Op::Const(c) => rational_to_f64(c),
                Op::Infinity(p) => {
                    if p {
                        f64::INFINITY
                    } else {
                        f64::NEG_INFINITY
                    }
                }
                Op::Add(a, b) => v[a] + v[b],
                Op::Sub(a, b) => v[a] - v[b],
                Op::Mul(a, b) => v[a] * v[b],
                Op::Div(a, b) => v[a] / v[b],
                Op::Min(a, b) => v[a].min(v[b]),
                Op::Max(a, b) => v[a].max(v[b]),
                Op::Neg(a) => -v[a],
                Op::Select {
                    cond,
                    then,
                    otherwise,
                } => {
                    if cmp_f64(cond.op, v[cond.a], v[cond.b]) {
                        v[then]
                    } else {
                        v[otherwise]
                    }
                }
            };
            v.push(x);
        }
        Some(v)
    }

    /// Evaluate every node exactly.  `None` on overflow, division by zero, or
    /// an infinite constant anywhere in the graph (fail-closed).
    pub fn eval_exact(&self, inputs: &[Rational]) -> Option<Vec<Rational>> {
        if inputs.len() != self.inputs {
            return None;
        }
        let mut v: Vec<Rational> = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let x = match node.op {
                Op::Input(i) => inputs[i],
                Op::Const(c) => c,
                Op::Infinity(_) => return None,
                Op::Add(a, b) => v[a].checked_add(v[b])?,
                Op::Sub(a, b) => v[a].checked_sub(v[b])?,
                Op::Mul(a, b) => v[a].checked_mul(v[b])?,
                Op::Div(a, b) => v[a].checked_div(v[b])?,
                Op::Min(a, b) => v[a].min(v[b]),
                Op::Max(a, b) => v[a].max(v[b]),
                Op::Neg(a) => v[a].checked_neg()?,
                Op::Select {
                    cond,
                    then,
                    otherwise,
                } => {
                    if cmp_exact(cond.op, v[cond.a], v[cond.b]) {
                        v[then]
                    } else {
                        v[otherwise]
                    }
                }
            };
            v.push(x);
        }
        Some(v)
    }
}

fn cmp_f64(op: Cmp, a: f64, b: f64) -> bool {
    match op {
        Cmp::Lt => a < b,
        Cmp::Le => a <= b,
        Cmp::Eq => a == b,
        Cmp::Ne => a != b,
    }
}

fn cmp_exact(op: Cmp, a: Rational, b: Rational) -> bool {
    match op {
        Cmp::Lt => a < b,
        Cmp::Le => a <= b,
        Cmp::Eq => a == b,
        Cmp::Ne => a != b,
    }
}

/// Nearest `f64` to a rational.  Used only for the `f64` evaluator's literals.
fn rational_to_f64(r: Rational) -> f64 {
    r.num() as f64 / r.den() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_f64_agree_on_a_simple_expression() {
        // (x − 1) / 2 + max(x, 0)
        let mut g = Graph::new();
        let x = g.input("x");
        let one = g.constant("1", Rational::ONE);
        let two = g.constant("2", Rational::from_int(2));
        let zero = g.constant("0", Rational::ZERO);
        let a = g.sub("x-1", x, one);
        let b = g.div("/2", a, two);
        let m = g.max("max(x,0)", x, zero);
        let out = g.add("out", b, m);

        let exact = g.eval_exact(&[Rational::from_int(3)]).unwrap();
        let approx = g.eval_f64(&[3.0]).unwrap();
        assert_eq!(exact[out], Rational::from_int(4));
        assert_eq!(approx[out], 4.0);
    }

    #[test]
    fn exact_evaluation_fails_closed_on_division_by_zero() {
        let mut g = Graph::new();
        let x = g.input("x");
        let zero = g.constant("0", Rational::ZERO);
        g.div("x/0", x, zero);
        assert!(g.eval_exact(&[Rational::ONE]).is_none());
    }

    #[test]
    fn exact_evaluation_fails_closed_on_infinity() {
        let mut g = Graph::new();
        g.infinity("+inf", true);
        assert!(g.eval_exact(&[]).is_none());
        assert!(g.eval_f64(&[]).unwrap()[0].is_infinite());
    }

    #[test]
    fn select_chooses_a_branch_without_hiding_untaken_failures() {
        // if x < 0 { 0 } else { 1 / x }: at x = 0 the else branch divides by zero,
        // which the exact evaluator reports rather than silently skipping.
        let mut g = Graph::new();
        let x = g.input("x");
        let zero = g.constant("0", Rational::ZERO);
        let one = g.constant("1", Rational::ONE);
        let inv = g.div("1/x", one, x);
        g.select(
            "guard",
            Cond { op: Cmp::Lt, a: x, b: zero },
            zero,
            inv,
        );
        assert_eq!(g.eval_exact(&[Rational::from_int(-2)]).unwrap()[4], Rational::ZERO);
        assert!(g.eval_exact(&[Rational::ZERO]).is_none());
    }
}
