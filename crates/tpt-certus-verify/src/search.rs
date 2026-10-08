//! Untrusted certificate search for degree-2 refutations.
//!
//! **Not part of the trust boundary.**  This module proposes a [`Refutation`];
//! [`find_refutation`] only returns it after [`crate::refute::check`] accepts
//! it, so a bug here can make the search fail but can never make an unsound
//! claim pass.
//!
//! # Method
//!
//! Candidate products are every fact alone and every pair of inequality facts
//! (plus the constant `1`).  Finding nonnegative multipliers that make the
//! candidates sum to a constant contradiction is a linear program over the
//! monomial coefficients, solved here by exact phase-1 simplex with Bland's
//! rule.  Two targets are tried:
//!
//! * the constant part equals `−1` (contradiction `< 0`), or
//! * the constant part equals `0` and the strict products carry total weight
//!   `1` (contradiction `0 > 0`).
//!
//! Equality facts get a free multiplier, split into a positive and a negative
//! part.  Exact arithmetic that overflows makes the search give up (`None`).

use alloc::vec;
use alloc::vec::Vec;

use crate::poly::{Mono, Poly};
use crate::rational::Rational;
use crate::refute::{check, Fact, Problem, Refutation, Relation, Term};

/// A candidate product the multipliers can scale.
struct Candidate {
    factors: Vec<usize>,
    poly: Poly,
    strict: bool,
    equality: bool,
}

/// The two contradiction shapes the search targets.
#[derive(Clone, Copy)]
enum Target {
    /// Constant part of the sum is `−1`.
    Negative,
    /// Constant part is `0` and strict products have total weight `1`.
    ZeroWithStrict,
}

/// Search for a refutation of `problem` among degree-≤2 products.  Returns a
/// certificate only if the trusted checker accepts it.
pub fn find_refutation(problem: &Problem) -> Option<Refutation> {
    let candidates = candidates(problem)?;
    for target in [Target::Negative, Target::ZeroWithStrict] {
        if let Some(cert) = solve(&candidates, target) {
            if check(problem, &cert).is_ok() {
                return Some(cert);
            }
        }
    }
    None
}

fn candidates(problem: &Problem) -> Option<Vec<Candidate>> {
    let facts: &[Fact] = &problem.facts;
    let mut out = vec![Candidate {
        factors: Vec::new(),
        poly: Poly::constant(Rational::ONE),
        strict: false,
        equality: false,
    }];
    for (i, f) in facts.iter().enumerate() {
        out.push(Candidate {
            factors: vec![i],
            poly: f.poly.clone(),
            strict: f.rel == Relation::Gt,
            equality: f.rel == Relation::Eq,
        });
    }
    for i in 0..facts.len() {
        for j in i..facts.len() {
            let (fi, fj) = (&facts[i], &facts[j]);
            if fi.rel == Relation::Eq || fj.rel == Relation::Eq {
                continue;
            }
            out.push(Candidate {
                factors: vec![i, j],
                poly: fi.poly.mul(&fj.poly)?,
                strict: fi.rel == Relation::Gt && fj.rel == Relation::Gt,
                equality: false,
            });
        }
    }
    Some(out)
}

/// One LP unknown: which candidate it scales, and with which sign.
struct Var {
    cand: usize,
    sign: i128,
}

fn solve(cands: &[Candidate], target: Target) -> Option<Refutation> {
    // Unknowns: one per inequality candidate, two (+/−) per equality candidate.
    let mut vars: Vec<Var> = Vec::new();
    for (ci, c) in cands.iter().enumerate() {
        if c.equality {
            vars.push(Var { cand: ci, sign: 1 });
            vars.push(Var { cand: ci, sign: -1 });
        } else {
            vars.push(Var { cand: ci, sign: 1 });
        }
    }

    // Rows: every non-constant monomial must cancel; the constant row fixes the
    // target; for ZeroWithStrict, strict weights sum to one.
    let mut monos: Vec<Mono> = Vec::new();
    for c in cands {
        for m in c.poly.terms().keys() {
            if !m.is_empty() && !monos.contains(m) {
                monos.push(m.clone());
            }
        }
    }

    let mut rows: Vec<(Vec<Rational>, Rational)> = Vec::new();
    for m in &monos {
        let row = vars
            .iter()
            .map(|v| scaled(cands[v.cand].poly.coefficient(m), v.sign))
            .collect::<Option<Vec<_>>>()?;
        rows.push((row, Rational::ZERO));
    }
    let const_row = vars
        .iter()
        .map(|v| scaled(cands[v.cand].poly.coefficient(&Mono::new()), v.sign))
        .collect::<Option<Vec<_>>>()?;
    let rhs = match target {
        Target::Negative => Rational::from_int(-1),
        Target::ZeroWithStrict => Rational::ZERO,
    };
    rows.push((const_row, rhs));
    if let Target::ZeroWithStrict = target {
        let strict_row = vars
            .iter()
            .map(|v| {
                let c = &cands[v.cand];
                Some(if c.strict && !c.equality { Rational::ONE } else { Rational::ZERO })
            })
            .collect::<Option<Vec<_>>>()?;
        rows.push((strict_row, Rational::ONE));
    }

    let x = feasible(rows, vars.len())?;

    // Assemble the certificate from the nonzero multipliers.
    let mut terms: Vec<Term> = Vec::new();
    let mut idx = 0;
    for c in cands {
        let coeff = if c.equality {
            let v = x[idx].checked_sub(x[idx + 1])?;
            idx += 2;
            v
        } else {
            let v = x[idx];
            idx += 1;
            v
        };
        if !coeff.is_zero() {
            terms.push(Term {
                coeff,
                factors: c.factors.clone(),
            });
        }
    }
    Some(Refutation { terms })
}

fn scaled(r: Rational, sign: i128) -> Option<Rational> {
    if sign < 0 {
        r.checked_neg()
    } else {
        Some(r)
    }
}

/// Exact phase-1 simplex: find `x ≥ 0` with `A x = b`, or `None` if none exists
/// or arithmetic overflows.  Returns the first `n` components of the solution.
fn feasible(rows: Vec<(Vec<Rational>, Rational)>, n: usize) -> Option<Vec<Rational>> {
    let m = rows.len();
    let width = n + m + 1;
    // Tableau with artificial columns n..n+m and rhs in the last column.
    let mut t: Vec<Vec<Rational>> = Vec::with_capacity(m);
    for (i, (a, b)) in rows.into_iter().enumerate() {
        let flip = b.is_negative();
        let mut row = vec![Rational::ZERO; width];
        for (j, v) in a.into_iter().enumerate() {
            row[j] = if flip { v.checked_neg()? } else { v };
        }
        row[n + i] = Rational::ONE;
        row[width - 1] = if flip { b.checked_neg()? } else { b };
        t.push(row);
    }
    let mut basis: Vec<usize> = (0..m).map(|i| n + i).collect();
    let cost = |j: usize| -> Rational {
        if j >= n {
            Rational::ONE
        } else {
            Rational::ZERO
        }
    };

    loop {
        // Reduced cost r_j = c_j − Σ_i c_{B(i)} t[i][j]; enter the smallest j < 0.
        let mut entering = None;
        for j in 0..n + m {
            let mut r = cost(j);
            for (i, row) in t.iter().enumerate() {
                r = r.checked_sub(cost(basis[i]).checked_mul(row[j])?)?;
            }
            if r.is_negative() {
                entering = Some(j);
                break;
            }
        }
        let Some(j) = entering else { break };

        // Ratio test with Bland's tie-break on the smallest basis index.
        let mut leave: Option<(usize, Rational)> = None;
        for (i, row) in t.iter().enumerate() {
            if row[j].is_positive() {
                let ratio = row[width - 1].checked_div(row[j])?;
                let better = match &leave {
                    None => true,
                    Some((li, lr)) => {
                        ratio < *lr || (ratio == *lr && basis[i] < basis[*li])
                    }
                };
                if better {
                    leave = Some((i, ratio));
                }
            }
        }
        let (i, _) = leave?;

        // Pivot on (i, j).
        let p = t[i][j];
        for v in t[i].iter_mut() {
            *v = v.checked_div(p)?;
        }
        let pivot_row = t[i].clone();
        for (k, row) in t.iter_mut().enumerate() {
            if k != i && !row[j].is_zero() {
                let f = row[j];
                for (v, pv) in row.iter_mut().zip(pivot_row.iter()) {
                    *v = v.checked_sub(f.checked_mul(*pv)?)?;
                }
            }
        }
        basis[i] = j;
    }

    // Feasible iff every artificial variable has left the basis at value zero.
    let mut x = vec![Rational::ZERO; n];
    for (i, &b) in basis.iter().enumerate() {
        let value = t[i][width - 1];
        if b >= n {
            if !value.is_zero() {
                return None;
            }
        } else {
            x[b] = value;
        }
    }
    Some(x)
}
