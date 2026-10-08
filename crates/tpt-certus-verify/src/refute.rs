//! Refutation certificates and their trusted checker.
//!
//! A [`Problem`] is a set of facts `p ≥ 0`, `p > 0` or `p = 0` over
//! polynomials.  A [`Refutation`] is a Positivstellensatz-style certificate:
//! a list of terms `c · Π f_i`, where each `f_i` is a fact, `c ≥ 0` for
//! inequality products, and `c` is unrestricted for a single equality fact.
//!
//! [`check`] accepts iff the terms sum to a constant `k` with either `k < 0`,
//! or `k = 0` and some strict product carries a positive coefficient.  Both
//! cases contradict the facts, so the problem is unsatisfiable.
//!
//! This module is the **trusted kernel** of the verifier.  It performs only
//! exact arithmetic and never searches.  Certificates are produced by
//! untrusted code elsewhere and are only believed after `check` accepts them.

use alloc::vec::Vec;

use crate::poly::Poly;
use crate::rational::Rational;

/// The relation a fact asserts about its polynomial.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    /// `p ≥ 0`.
    Ge,
    /// `p > 0`.
    Gt,
    /// `p = 0`.
    Eq,
}

/// One hypothesis (or the negated goal) over polynomials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    /// The polynomial the relation applies to.
    pub poly: Poly,
    /// The relation to zero.
    pub rel: Relation,
}

/// A conjunction of facts whose joint unsatisfiability is to be shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Problem {
    /// The facts, indexed by position in certificate terms.
    pub facts: Vec<Fact>,
}

/// One term `coeff · Π facts[factors]` of a refutation.
///
/// An empty `factors` list denotes the constant product `1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Term {
    /// The multiplier.  Must be `≥ 0` for inequality products; unrestricted
    /// for a term that is a single equality fact.
    pub coeff: Rational,
    /// Indices into [`Problem::facts`], multiplied together.
    pub factors: Vec<usize>,
}

/// A refutation certificate: terms whose sum is a constant contradiction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Refutation {
    /// The terms that are summed.
    pub terms: Vec<Term>,
}

/// Why a certificate was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckError {
    /// A term refers to a fact index that does not exist.
    UnknownFact(usize),
    /// An equality fact appears inside a product with other factors, or a
    /// lone equality fact has a non-trivial product structure.
    MisusedEquality(usize),
    /// An inequality term has a negative coefficient.
    NegativeCoefficient(usize),
    /// Exact arithmetic overflowed; the certificate is not accepted.
    Overflow,
    /// The terms do not sum to a constant, so no contradiction follows.
    NotConstant,
    /// The sum is a constant but does not contradict the facts.
    NoContradiction,
}

/// Verify `cert` against `problem`.  `Ok(())` means the facts are
/// unsatisfiable; any `Err` means the certificate proves nothing.
pub fn check(problem: &Problem, cert: &Refutation) -> Result<(), CheckError> {
    let mut sum = Poly::zero();
    let mut has_strict_positive = false;

    for (ti, term) in cert.terms.iter().enumerate() {
        let mut product = Poly::constant(Rational::ONE);
        let mut all_strict = !term.factors.is_empty();
        let mut lone_equality = false;

        for &fi in &term.factors {
            let fact = problem.facts.get(fi).ok_or(CheckError::UnknownFact(fi))?;
            match fact.rel {
                Relation::Eq => {
                    if term.factors.len() != 1 {
                        return Err(CheckError::MisusedEquality(ti));
                    }
                    lone_equality = true;
                    all_strict = false;
                }
                Relation::Gt => {}
                Relation::Ge => all_strict = false,
            }
            product = product.mul(&fact.poly).ok_or(CheckError::Overflow)?;
        }

        if !lone_equality && term.coeff.is_negative() {
            return Err(CheckError::NegativeCoefficient(ti));
        }
        if !lone_equality && all_strict && term.coeff.is_positive() {
            has_strict_positive = true;
        }

        let contribution = product
            .scale(term.coeff)
            .ok_or(CheckError::Overflow)?;
        sum = sum.add(&contribution).ok_or(CheckError::Overflow)?;
    }

    let constant = sum.as_constant().ok_or(CheckError::NotConstant)?;
    if constant.is_negative() || (constant.is_zero() && has_strict_positive) {
        Ok(())
    } else {
        Err(CheckError::NoContradiction)
    }
}
