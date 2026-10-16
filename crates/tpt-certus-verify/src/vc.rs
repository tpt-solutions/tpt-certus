//! Verification conditions for a [`Graph`].
//!
//! The graph is evaluated *symbolically*.  Each node becomes a set of
//! alternatives: a value (a polynomial, or `±∞`) together with the facts that
//! hold on that path.  Branching comes from `Min`/`Max` (which argument wins)
//! and from `Select` (which branch of the condition is taken).  A division
//! `a / b` introduces an auxiliary variable `q` with the fact `b·q − a = 0`.
//! That is sound only when `b` has a declared sign, so the generator rejects a
//! denominator with no sign assumption.
//!
//! A [`Claim`] holds when, on every alternative of its node, the negation of
//! the claim is refuted by [`crate::search::find_refutation`].  Each
//! refutation is itself checked by the trusted kernel, so this module only
//! decides *which* problems to ask, never whether an answer is believed.
//!
//! # Compositional use
//!
//! A claim on a branch-free node can be promoted to an assumption with
//! [`prove_lemma`].  Later claims then build on it, which is how a multi-step
//! argument is split into degree-2 pieces.  Each lemma is proven before it is
//! used, so the chain stays sound.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::ir::{Cmp, Cond, Graph, NodeId, Op};
use crate::poly::Poly;
use crate::rational::Rational;
use crate::refute::{Fact, Problem, Relation};
use crate::search::find_refutation;

/// Upper bound on symbolic alternatives per node.  Exceeding it is an error,
/// never a silent truncation.
pub const MAX_ALTERNATIVES: usize = 4096;

/// A property of one node, stated over every path that reaches it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// The node equals the constant.
    Eq(NodeId, Rational),
    /// The node is at least the constant.
    Ge(NodeId, Rational),
    /// The node is at most the constant.
    Le(NodeId, Rational),
}

/// Why a verification condition could not be discharged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VcError {
    /// The graph uses a construct this generator does not model.
    Unsupported(String),
    /// Symbolic branching exceeded [`MAX_ALTERNATIVES`].
    TooManyAlternatives,
    /// A claim's negation was not refuted on one alternative.
    NotProven {
        /// Index of the failing alternative for that node.
        alternative: usize,
    },
    /// Exact arithmetic overflowed while building a condition.
    Overflow,
    /// A lemma was requested for a node that is not unconditional.
    ConditionalLemma,
}

/// Summary of a successful proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proof {
    /// Alternatives on the claimed node that were each discharged.
    pub alternatives: usize,
}

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Poly(Poly),
    PosInf,
    NegInf,
}

#[derive(Clone, Debug)]
struct Alt {
    facts: Vec<Fact>,
    val: Val,
}

/// Evaluation context: the graph, the caller's assumptions, and the variable
/// layout (inputs first, then one auxiliary variable per division node).
struct Ctx<'a> {
    graph: &'a Graph,
    assumptions: &'a [Fact],
    aux_base: usize,
}

fn var_index(i: usize) -> Result<u16, VcError> {
    u16::try_from(i).map_err(|_| VcError::Unsupported(String::from("too many variables")))
}

fn poly_of(v: &Val) -> Result<&Poly, VcError> {
    match v {
        Val::Poly(p) => Ok(p),
        _ => Err(VcError::Unsupported(String::from(
            "arithmetic on an infinite value",
        ))),
    }
}

fn one(facts: Vec<Fact>, val: Val) -> Result<Vec<Alt>, VcError> {
    Ok(alloc::vec![Alt { facts, val }])
}

fn join(a: &[Fact], b: &[Fact]) -> Vec<Fact> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    out.extend_from_slice(a);
    out.extend_from_slice(b);
    out
}

fn cap(alts: Vec<Alt>) -> Result<Vec<Alt>, VcError> {
    if alts.len() > MAX_ALTERNATIVES {
        Err(VcError::TooManyAlternatives)
    } else {
        Ok(alts)
    }
}

/// Order key for extended values: `−∞ < finite < +∞`.
fn key(v: &Val) -> i8 {
    match v {
        Val::NegInf => -1,
        Val::Poly(_) => 0,
        Val::PosInf => 1,
    }
}

impl<'a> Ctx<'a> {
    fn eval(&self, id: NodeId) -> Result<Vec<Alt>, VcError> {
        let node = self
            .graph
            .node(id)
            .ok_or_else(|| VcError::Unsupported(format!("node {id} out of range")))?;
        match node.op {
            Op::Input(i) => one(Vec::new(), Val::Poly(Poly::var(var_index(i)?))),
            Op::Const(c) => one(Vec::new(), Val::Poly(Poly::constant(c))),
            Op::Infinity(p) => one(Vec::new(), if p { Val::PosInf } else { Val::NegInf }),
            Op::Add(a, b) => self.binary(a, b, |x, y| x.add(y)),
            Op::Sub(a, b) => self.binary(a, b, |x, y| x.sub(y)),
            Op::Mul(a, b) => self.binary(a, b, |x, y| x.mul(y)),
            Op::Neg(a) => self.unary(a, |x| x.neg()),
            Op::Div(a, b) => self.div(id, a, b),
            Op::Min(a, b) => self.minmax(a, b, true),
            Op::Max(a, b) => self.minmax(a, b, false),
            Op::Select {
                cond,
                then,
                otherwise,
            } => self.select(cond, then, otherwise),
        }
    }

    fn unary<F>(&self, a: NodeId, f: F) -> Result<Vec<Alt>, VcError>
    where
        F: Fn(&Poly) -> Option<Poly>,
    {
        let mut out = Vec::new();
        for ua in self.eval(a)? {
            let p = f(poly_of(&ua.val)?).ok_or(VcError::Overflow)?;
            out.push(Alt {
                facts: ua.facts,
                val: Val::Poly(p),
            });
        }
        cap(out)
    }

    fn binary<F>(&self, a: NodeId, b: NodeId, f: F) -> Result<Vec<Alt>, VcError>
    where
        F: Fn(&Poly, &Poly) -> Option<Poly>,
    {
        let mut out = Vec::new();
        for ua in self.eval(a)? {
            for ub in self.eval(b)? {
                let p = f(poly_of(&ua.val)?, poly_of(&ub.val)?).ok_or(VcError::Overflow)?;
                out.push(Alt {
                    facts: join(&ua.facts, &ub.facts),
                    val: Val::Poly(p),
                });
            }
        }
        cap(out)
    }

    /// `q = a / b` with `b·q − a = 0`, valid because `b` has a declared sign.
    fn div(&self, id: NodeId, a: NodeId, b: NodeId) -> Result<Vec<Alt>, VcError> {
        let q_var = var_index(self.aux_base + id)?;
        let q = Poly::var(q_var);
        let mut out = Vec::new();
        for ua in self.eval(a)? {
            for ub in self.eval(b)? {
                let num = poly_of(&ua.val)?;
                let den = poly_of(&ub.val)?;
                if !self.sign_declared(den) {
                    return Err(VcError::Unsupported(format!(
                        "division at node {id} has no declared sign for its denominator"
                    )));
                }
                let eq = den.mul(&q).and_then(|p| p.sub(num)).ok_or(VcError::Overflow)?;
                let mut facts = join(&ua.facts, &ub.facts);
                facts.push(Fact {
                    poly: eq,
                    rel: Relation::Eq,
                });
                out.push(Alt {
                    facts,
                    val: Val::Poly(q.clone()),
                });
            }
        }
        cap(out)
    }

    /// Whether the assumptions plus `facts` are already contradictory.  Pruning
    /// such a path never changes what is provable, since it has no models.
    fn refuted(&self, facts: &[Fact]) -> bool {
        let mut all = join(self.assumptions, facts);
        all.shrink_to_fit();
        find_refutation(&Problem { facts: all }).is_some()
    }

    /// Whether `den` is declared strictly positive or negative by an assumption.
    fn sign_declared(&self, den: &Poly) -> bool {
        let neg = match den.neg() {
            Some(n) => n,
            None => return false,
        };
        self.assumptions.iter().any(|f| {
            f.rel == Relation::Gt && (f.poly == *den || f.poly == neg)
        })
    }

    fn minmax(&self, a: NodeId, b: NodeId, is_min: bool) -> Result<Vec<Alt>, VcError> {
        let mut out = Vec::new();
        for ua in self.eval(a)? {
            for ub in self.eval(b)? {
                if key(&ua.val) != 0 || key(&ub.val) != 0 {
                    // Infinite operand: the winner is decided by ordering alone.
                    let pick_a = if is_min {
                        key(&ua.val) <= key(&ub.val)
                    } else {
                        key(&ua.val) >= key(&ub.val)
                    };
                    let chosen = if pick_a { &ua.val } else { &ub.val };
                    out.push(Alt {
                        facts: join(&ua.facts, &ub.facts),
                        val: chosen.clone(),
                    });
                    continue;
                }
                let (pa, pb) = (poly_of(&ua.val)?, poly_of(&ub.val)?);
                // ge ≥ 0 says `a` is the winner: for min, a ≤ b; for max, a ≥ b.
                let ge = if is_min {
                    pb.sub(pa).ok_or(VcError::Overflow)?
                } else {
                    pa.sub(pb).ok_or(VcError::Overflow)?
                };
                let neg_ge = ge.neg().ok_or(VcError::Overflow)?;
                let base = join(&ua.facts, &ub.facts);

                let mut with_a = base.clone();
                with_a.push(Fact {
                    poly: ge,
                    rel: Relation::Ge,
                });
                out.push(Alt {
                    facts: with_a,
                    val: ua.val.clone(),
                });

                let mut with_b = base;
                with_b.push(Fact {
                    poly: neg_ge,
                    rel: Relation::Ge,
                });
                out.push(Alt {
                    facts: with_b,
                    val: ub.val.clone(),
                });
            }
        }
        cap(out)
    }

    /// Condition alternatives: each is a set of facts plus the truth value it
    /// establishes.  Equality and disequality split into strict sub-cases.
    fn condition(&self, op: Cmp, va: &Val, vb: &Val) -> Result<Vec<(Vec<Fact>, bool)>, VcError> {
        match (va, vb) {
            (Val::Poly(a), Val::Poly(b)) => {
                let p = a.sub(b).ok_or(VcError::Overflow)?;
                let np = p.neg().ok_or(VcError::Overflow)?;
                let fact = |poly: Poly, rel: Relation| Fact { poly, rel };
                Ok(match op {
                    Cmp::Lt => alloc::vec![
                        (alloc::vec![fact(np, Relation::Gt)], true),
                        (alloc::vec![fact(p, Relation::Ge)], false),
                    ],
                    Cmp::Le => alloc::vec![
                        (alloc::vec![fact(np, Relation::Ge)], true),
                        (alloc::vec![fact(p, Relation::Gt)], false),
                    ],
                    Cmp::Eq => alloc::vec![
                        (alloc::vec![fact(p.clone(), Relation::Eq)], true),
                        (alloc::vec![fact(p.clone(), Relation::Gt)], false),
                        (alloc::vec![fact(np.clone(), Relation::Gt)], false),
                    ],
                    Cmp::Ne => alloc::vec![
                        (alloc::vec![fact(p.clone(), Relation::Gt)], true),
                        (alloc::vec![fact(np, Relation::Gt)], true),
                        (alloc::vec![fact(p.clone(), Relation::Eq)], false),
                    ],
                })
            }
            _ => {
                // Infinite operands: the comparison is decided by ordering.
                let (ka, kb) = (key(va), key(vb));
                let truth = match op {
                    Cmp::Lt => ka < kb,
                    Cmp::Le => ka <= kb,
                    Cmp::Eq => ka == kb,
                    Cmp::Ne => ka != kb,
                };
                Ok(alloc::vec![(Vec::new(), truth)])
            }
        }
    }

    fn select(
        &self,
        cond: Cond,
        then: NodeId,
        otherwise: NodeId,
    ) -> Result<Vec<Alt>, VcError> {
        let mut out = Vec::new();
        for ua in self.eval(cond.a)? {
            for ub in self.eval(cond.b)? {
                for (cf, truth) in self.condition(cond.op, &ua.val, &ub.val)? {
                    let branch = if truth { then } else { otherwise };
                    let base = join(&join(&ua.facts, &ub.facts), &cf);
                    for chosen in self.eval(branch)? {
                        out.push(Alt {
                            facts: join(&base, &chosen.facts),
                            val: chosen.val,
                        });
                    }
                }
            }
        }
        cap(out)
    }
}

fn check_all(ctx: &Ctx<'_>, node: NodeId, alternatives: &[Alt], claim_poly: impl Fn(&Poly) -> Option<Vec<Poly>>) -> Result<Proof, VcError> {
    let mut count = 0usize;
    for (ai, alt) in alternatives.iter().enumerate() {
        let p = match &alt.val {
            Val::Poly(p) => p,
            _ => {
                return Err(VcError::Unsupported(format!(
                    "claim on node {node} reaches an infinite value"
                )))
            }
        };
        let negations = claim_poly(p).ok_or(VcError::Overflow)?;
        for neg in negations {
            if neg.is_zero() {
                continue;
            }
            let mut facts = join(ctx.assumptions, &alt.facts);
            facts.push(Fact {
                poly: neg,
                rel: Relation::Gt,
            });
            let problem = Problem { facts };
            if find_refutation(&problem).is_none() {
                return Err(VcError::NotProven { alternative: ai });
            }
        }
        count += 1;
    }
    Ok(Proof {
        alternatives: count,
    })
}

/// Prove `claim` on every path that reaches its node, under `assumptions`.
///
/// Each returned [`Proof`] covers all alternatives; any undischarged one
/// yields an error naming it.
pub fn prove(graph: &Graph, assumptions: &[Fact], claim: Claim) -> Result<Proof, VcError> {
    let ctx = Ctx {
        graph,
        assumptions,
        aux_base: graph.input_count(),
    };
    let (node, negations): (NodeId, fn(&Poly, &Rational) -> Option<Vec<Poly>>) = match claim {
        Claim::Eq(n, _) => (n, |p, c| {
            let d = p.sub(&Poly::constant(*c))?;
            Some(alloc::vec![d.clone(), d.neg()?])
        }),
        Claim::Ge(n, _) => (n, |p, c| {
            Some(alloc::vec![Poly::constant(*c).sub(p)?])
        }),
        Claim::Le(n, _) => (n, |p, c| Some(alloc::vec![p.sub(&Poly::constant(*c))?])),
    };
    let c = match claim {
        Claim::Eq(_, c) | Claim::Ge(_, c) | Claim::Le(_, c) => c,
    };
    let alts = ctx.eval(node)?;
    check_all(&ctx, node, &alts, |p| negations(p, &c))
}

/// Whether the subgraph under `id` contains no `Min`/`Max`/`Select`.  Such a
/// subgraph has a single symbolic path, so its facts are definitional (the
/// division equations) and hold globally under the declared assumptions.
fn branch_free(graph: &Graph, id: NodeId) -> bool {
    let Some(node) = graph.node(id) else {
        return false;
    };
    match node.op {
        Op::Input(_) | Op::Const(_) => true,
        Op::Infinity(_) => false,
        Op::Neg(a) => branch_free(graph, a),
        Op::Add(a, b) | Op::Sub(a, b) | Op::Mul(a, b) | Op::Div(a, b) => {
            branch_free(graph, a) && branch_free(graph, b)
        }
        Op::Min(_, _) | Op::Max(_, _) | Op::Select { .. } => false,
    }
}

/// Prove `claim` and return it, with its definitional facts, as assumptions for
/// later proofs.
///
/// Only branch-free nodes qualify: their single path's facts are the division
/// equations, which hold under the declared sign assumptions.  Anything
/// conditional yields [`VcError::ConditionalLemma`], since a fact valid on one
/// branch is not valid globally.
pub fn prove_lemma(graph: &Graph, assumptions: &[Fact], claim: Claim) -> Result<Vec<Fact>, VcError> {
    prove(graph, assumptions, claim)?;
    let node = match claim {
        Claim::Eq(n, _) | Claim::Ge(n, _) | Claim::Le(n, _) => n,
    };
    if !branch_free(graph, node) {
        return Err(VcError::ConditionalLemma);
    }
    let ctx = Ctx {
        graph,
        assumptions,
        aux_base: graph.input_count(),
    };
    let alts = ctx.eval(node)?;
    let [only] = alts.as_slice() else {
        return Err(VcError::ConditionalLemma);
    };
    let p = match &only.val {
        Val::Poly(p) => p.clone(),
        _ => return Err(VcError::ConditionalLemma),
    };
    let (poly, rel) = match claim {
        Claim::Eq(_, c) => (p.sub(&Poly::constant(c)), Relation::Eq),
        Claim::Ge(_, c) => (p.sub(&Poly::constant(c)), Relation::Ge),
        Claim::Le(_, c) => (Poly::constant(c).sub(&p), Relation::Ge),
    };
    let poly = poly.ok_or(VcError::Overflow)?;
    let mut out = only.facts.clone();
    out.push(Fact { poly, rel });
    Ok(out)
}
