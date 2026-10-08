//! Exact rational arithmetic over `i128`, fail-closed on overflow.
//!
//! Every operation returns `None` instead of wrapping or rounding.  A `None`
//! means the verifier cannot decide the obligation, never that it passed.

use alloc::string::String;
use core::cmp::Ordering;
use core::fmt;

/// A rational number `num / den` in lowest terms with `den > 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rational {
    num: i128,
    den: i128,
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    a = a.unsigned_abs() as i128;
    b = b.unsigned_abs() as i128;
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

impl Rational {
    /// Zero.
    pub const ZERO: Rational = Rational { num: 0, den: 1 };
    /// One.
    pub const ONE: Rational = Rational { num: 1, den: 1 };

    /// Build `num / den` in lowest terms.  `None` if `den == 0`.
    pub fn new(num: i128, den: i128) -> Option<Self> {
        if den == 0 {
            return None;
        }
        let (mut num, mut den) = if den < 0 {
            (num.checked_neg()?, den.checked_neg()?)
        } else {
            (num, den)
        };
        let g = gcd(num, den);
        if g > 1 {
            num /= g;
            den /= g;
        }
        if num == 0 {
            den = 1;
        }
        Some(Rational { num, den })
    }

    /// The integer `n`.
    pub fn from_int(n: i128) -> Self {
        Rational { num: n, den: 1 }
    }

    /// Numerator in lowest terms.
    pub fn num(&self) -> i128 {
        self.num
    }

    /// Denominator in lowest terms (always positive).
    pub fn den(&self) -> i128 {
        self.den
    }

    /// Whether this is zero.
    pub fn is_zero(&self) -> bool {
        self.num == 0
    }

    /// Whether this is strictly negative.
    pub fn is_negative(&self) -> bool {
        self.num < 0
    }

    /// Whether this is strictly positive.
    pub fn is_positive(&self) -> bool {
        self.num > 0
    }

    /// Checked negation.
    pub fn checked_neg(self) -> Option<Self> {
        Some(Rational {
            num: self.num.checked_neg()?,
            den: self.den,
        })
    }

    /// Checked addition.
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        let num = self
            .num
            .checked_mul(rhs.den)?
            .checked_add(rhs.num.checked_mul(self.den)?)?;
        let den = self.den.checked_mul(rhs.den)?;
        Rational::new(num, den)
    }

    /// Checked subtraction.
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.checked_add(rhs.checked_neg()?)
    }

    /// Checked multiplication.
    pub fn checked_mul(self, rhs: Self) -> Option<Self> {
        let num = self.num.checked_mul(rhs.num)?;
        let den = self.den.checked_mul(rhs.den)?;
        Rational::new(num, den)
    }

    /// Checked division.  `None` if `rhs` is zero or on overflow.
    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        if rhs.is_zero() {
            return None;
        }
        self.checked_mul(Rational::new(rhs.den, rhs.num)?)
    }

    /// Render as `num` or `num/den` (for audit output).
    pub fn to_audit_string(&self) -> String {
        if self.den == 1 {
            alloc::format!("{}", self.num)
        } else {
            alloc::format!("{}/{}", self.num, self.den)
        }
    }
}

/// Compare `an/ad` with `bn/bd` (denominators positive) without any
/// multiplication, so it cannot overflow.  Euclid-style on the integer parts.
fn cmp_frac(an: i128, ad: i128, bn: i128, bd: i128) -> Ordering {
    let (aq, bq) = (an.div_euclid(ad), bn.div_euclid(bd));
    if aq != bq {
        return aq.cmp(&bq);
    }
    let (ar, br) = (an.rem_euclid(ad), bn.rem_euclid(bd));
    match (ar == 0, br == 0) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        // ar/ad < br/bd  ⇔  ad/ar > bd/br, so compare reciprocals reversed.
        (false, false) => cmp_frac(ad, ar, bd, br).reverse(),
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> Ordering {
        cmp_frac(self.num, self.den, other.num, other.den)
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_audit_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_to_lowest_terms_with_positive_denominator() {
        let r = Rational::new(4, -6).unwrap();
        assert_eq!((r.num(), r.den()), (-2, 3));
        assert_eq!(Rational::new(0, -7).unwrap(), Rational::ZERO);
        assert!(Rational::new(1, 0).is_none());
    }

    #[test]
    fn arithmetic_is_exact() {
        let a = Rational::new(1, 3).unwrap();
        let b = Rational::new(1, 6).unwrap();
        assert_eq!(a.checked_add(b).unwrap(), Rational::new(1, 2).unwrap());
        assert_eq!(a.checked_sub(b).unwrap(), Rational::new(1, 6).unwrap());
        assert_eq!(a.checked_mul(b).unwrap(), Rational::new(1, 18).unwrap());
    }

    #[test]
    fn ordering_is_exact_without_overflow() {
        let a = Rational::new(1, 3).unwrap();
        let b = Rational::new(1, 2).unwrap();
        assert!(a < b);
        assert!(Rational::new(-7, 2).unwrap() < Rational::new(-3, 1).unwrap());
        // Near i128::MAX: cross-multiplication would overflow, the comparison must not.
        let x = Rational::new(i128::MAX, i128::MAX - 1).unwrap();
        let y = Rational::new(i128::MAX - 1, i128::MAX - 2).unwrap();
        assert!(x < y);
    }

    #[test]
    fn overflow_fails_closed() {
        let big = Rational::from_int(i128::MAX);
        assert!(big.checked_add(Rational::ONE).is_none());
        assert!(big.checked_mul(Rational::from_int(2)).is_none());
        assert!(Rational::from_int(i128::MIN).checked_neg().is_none());
    }
}
