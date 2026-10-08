//! Sparse multivariate polynomials with exact rational coefficients.
//!
//! Variables are `u16` indices into the caller's variable table.  Every
//! operation is fail-closed: an overflow yields `None`.

use alloc::collections::BTreeMap;

use crate::rational::Rational;

/// A monomial: variable index → exponent.  The empty map is the constant `1`.
pub type Mono = BTreeMap<u16, u32>;

/// A polynomial: monomial → nonzero coefficient.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Poly {
    terms: BTreeMap<Mono, Rational>,
}

impl Poly {
    /// The zero polynomial.
    pub fn zero() -> Self {
        Poly::default()
    }

    /// The constant polynomial `c`.
    pub fn constant(c: Rational) -> Self {
        let mut p = Poly::zero();
        if !c.is_zero() {
            p.terms.insert(Mono::new(), c);
        }
        p
    }

    /// The polynomial `x_i`.
    pub fn var(i: u16) -> Self {
        let mut m = Mono::new();
        m.insert(i, 1);
        let mut p = Poly::zero();
        p.terms.insert(m, Rational::ONE);
        p
    }

    /// `Some(c)` if this polynomial is the constant `c` (including zero).
    pub fn as_constant(&self) -> Option<Rational> {
        match self.terms.len() {
            0 => Some(Rational::ZERO),
            1 => self.terms.get(&Mono::new()).copied(),
            _ => None,
        }
    }

    /// The monomial → coefficient map (nonzero coefficients only).
    pub fn terms(&self) -> &BTreeMap<Mono, Rational> {
        &self.terms
    }

    /// The coefficient of `m`, zero if absent.
    pub fn coefficient(&self, m: &Mono) -> Rational {
        self.terms.get(m).copied().unwrap_or(Rational::ZERO)
    }

    /// Whether this is the zero polynomial.
    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    /// Checked sum.
    pub fn add(&self, rhs: &Poly) -> Option<Poly> {
        let mut out = self.clone();
        for (m, c) in &rhs.terms {
            out.add_term(m.clone(), *c)?;
        }
        Some(out)
    }

    /// Checked negation.
    pub fn neg(&self) -> Option<Poly> {
        self.scale(Rational::from_int(-1))
    }

    /// Checked difference `self - rhs`.
    pub fn sub(&self, rhs: &Poly) -> Option<Poly> {
        self.add(&rhs.neg()?)
    }

    /// Checked scalar multiple.
    pub fn scale(&self, k: Rational) -> Option<Poly> {
        let mut out = Poly::zero();
        for (m, c) in &self.terms {
            out.add_term(m.clone(), c.checked_mul(k)?)?;
        }
        Some(out)
    }

    /// Checked product.
    pub fn mul(&self, rhs: &Poly) -> Option<Poly> {
        let mut out = Poly::zero();
        for (m1, c1) in &self.terms {
            for (m2, c2) in &rhs.terms {
                let mut m = m1.clone();
                for (v, e) in m2 {
                    let entry = m.entry(*v).or_insert(0);
                    *entry = entry.checked_add(*e)?;
                }
                out.add_term(m, c1.checked_mul(*c2)?)?;
            }
        }
        Some(out)
    }

    fn add_term(&mut self, m: Mono, c: Rational) -> Option<()> {
        let sum = match self.terms.get(&m) {
            Some(prev) => prev.checked_add(c)?,
            None => c,
        };
        if sum.is_zero() {
            self.terms.remove(&m);
        } else {
            self.terms.insert(m, sum);
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_removes_terms() {
        let x = Poly::var(0);
        let d = x.sub(&x).unwrap();
        assert!(d.is_zero());
        assert_eq!(d.as_constant(), Some(Rational::ZERO));
    }

    #[test]
    fn product_expands_and_combines_exponents() {
        // (d + 1) * (d - 1) = d^2 - 1
        let d = Poly::var(0);
        let one = Poly::constant(Rational::ONE);
        let p = d.add(&one).unwrap().mul(&d.sub(&one).unwrap()).unwrap();
        let expected = d
            .mul(&d)
            .unwrap()
            .sub(&one)
            .unwrap();
        assert_eq!(p, expected);
        assert!(p.as_constant().is_none());
    }
}
