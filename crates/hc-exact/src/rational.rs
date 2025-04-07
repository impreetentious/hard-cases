//! Exact rationals — the numbers every claim is finally made of.
//!
//! A rational is a [`BigInt`] numerator over a strictly positive [`BigUint`]
//! denominator, reduced on construction so that one value has exactly one
//! representation. Equality is then structural, and the canonical text a
//! certificate carries is a total function of the value rather than of how it
//! was computed.
//!
//! Two conversions live here and they point in opposite directions. Reading a
//! decimal token produces the number the file's characters name, with no
//! floating-point value anywhere on the path. Producing a `f64` is the single
//! place a rational becomes a float, it exists for the search, and nothing it
//! returns may be cited by a proof.

use core::cmp::Ordering;
use core::fmt;

use crate::int::BigInt;
use crate::uint::BigUint;

/// An exact rational number.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct Rational {
    /// Carries the sign of the value.
    numerator: BigInt,
    /// Strictly positive, and coprime with the numerator's magnitude.
    denominator: BigUint,
}

impl Rational {
    /// Zero.
    #[must_use]
    pub fn zero() -> Self {
        Self {
            numerator: BigInt::zero(),
            denominator: BigUint::one(),
        }
    }

    /// One.
    #[must_use]
    pub fn one() -> Self {
        Self {
            numerator: BigInt::one(),
            denominator: BigUint::one(),
        }
    }

    /// A rational from an integer.
    #[must_use]
    pub fn from_int(value: BigInt) -> Self {
        Self {
            numerator: value,
            denominator: BigUint::one(),
        }
    }

    /// A rational from a machine word.
    #[must_use]
    pub fn from_i64(value: i64) -> Self {
        Self::from_int(BigInt::from_i64(value))
    }

    /// A rational from a numerator and denominator, reduced.
    ///
    /// # Errors
    ///
    /// Returns [`RationalError::ZeroDenominator`] when the denominator is zero.
    pub fn new(numerator: BigInt, denominator: BigInt) -> Result<Self, RationalError> {
        if denominator.is_zero() {
            return Err(RationalError::ZeroDenominator);
        }
        let negative = numerator.is_negative() != denominator.is_negative();
        let mut top = numerator.magnitude().clone();
        let mut bottom = denominator.magnitude().clone();
        let divisor = top.gcd(&bottom);
        if divisor.cmp_value(&BigUint::one()) != Ordering::Equal {
            top = top.div(&divisor);
            bottom = bottom.div(&divisor);
        }
        if top.is_zero() {
            // Zero has one canonical form and its denominator is one. Leaving
            // the original denominator here would make `0/5` and `0/1` compare
            // unequal while naming the same number.
            return Ok(Self::zero());
        }
        Ok(Self {
            numerator: BigInt::from_sign_magnitude(negative, top),
            denominator: bottom,
        })
    }

    /// The reduced numerator, carrying the sign.
    #[must_use]
    pub fn numerator(&self) -> &BigInt {
        &self.numerator
    }

    /// The reduced denominator, always strictly positive.
    #[must_use]
    pub fn denominator(&self) -> &BigUint {
        &self.denominator
    }

    /// Whether the value is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.numerator.is_zero()
    }

    /// Whether the value is strictly negative.
    #[must_use]
    pub fn is_negative(&self) -> bool {
        self.numerator.is_negative()
    }

    /// Whether the value is strictly positive.
    #[must_use]
    pub fn is_positive(&self) -> bool {
        self.numerator.is_positive()
    }

    /// Minus one, zero, or one.
    #[must_use]
    pub fn signum(&self) -> i32 {
        self.numerator.signum()
    }

    /// Whether the value is an integer.
    #[must_use]
    pub fn is_integer(&self) -> bool {
        self.denominator.cmp_value(&BigUint::one()) == Ordering::Equal
    }

    /// The value as an integer, when it is one.
    #[must_use]
    pub fn to_int(&self) -> Option<BigInt> {
        self.is_integer().then(|| self.numerator.clone())
    }

    /// The larger of the numerator and denominator bit lengths.
    ///
    /// This is what the certificate bit cap is measured against, so a value is
    /// only as large as its widest half.
    #[must_use]
    pub fn bit_len(&self) -> u64 {
        core::cmp::max(
            self.numerator.magnitude().bit_len(),
            self.denominator.bit_len(),
        )
    }

    /// The additive inverse.
    #[must_use]
    pub fn neg(&self) -> Self {
        Self {
            numerator: self.numerator.neg(),
            denominator: self.denominator.clone(),
        }
    }

    /// The absolute value.
    #[must_use]
    pub fn abs(&self) -> Self {
        Self {
            numerator: self.numerator.abs(),
            denominator: self.denominator.clone(),
        }
    }

    /// The multiplicative inverse, or `None` for zero.
    #[must_use]
    pub fn recip(&self) -> Option<Self> {
        if self.is_zero() {
            return None;
        }
        Some(Self {
            numerator: BigInt::from_sign_magnitude(
                self.numerator.is_negative(),
                self.denominator.clone(),
            ),
            denominator: self.numerator.magnitude().clone(),
        })
    }

    /// Sum.
    #[must_use]
    pub fn add(&self, rhs: &Self) -> Self {
        let numerator = self
            .numerator
            .mul(&BigInt::from_magnitude(rhs.denominator.clone()))
            .add(
                &rhs.numerator
                    .mul(&BigInt::from_magnitude(self.denominator.clone())),
            );
        let denominator = BigInt::from_magnitude(self.denominator.mul(&rhs.denominator));
        Self::new(numerator, denominator).expect("denominators are strictly positive")
    }

    /// Difference.
    #[must_use]
    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    /// Product.
    #[must_use]
    pub fn mul(&self, rhs: &Self) -> Self {
        let numerator = self.numerator.mul(&rhs.numerator);
        let denominator = BigInt::from_magnitude(self.denominator.mul(&rhs.denominator));
        Self::new(numerator, denominator).expect("denominators are strictly positive")
    }

    /// Quotient, or `None` when `rhs` is zero.
    #[must_use]
    pub fn checked_div(&self, rhs: &Self) -> Option<Self> {
        let inverse = rhs.recip()?;
        Some(self.mul(&inverse))
    }

    /// Ordering by value, decided by cross multiplication.
    ///
    /// Both denominators are strictly positive, so no sign flip is needed and
    /// the comparison is exact where a float comparison of the same two values
    /// is not.
    #[must_use]
    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        let left = self
            .numerator
            .mul(&BigInt::from_magnitude(rhs.denominator.clone()));
        let right = rhs
            .numerator
            .mul(&BigInt::from_magnitude(self.denominator.clone()));
        left.cmp_value(&right)
    }

    /// The greatest integer no greater than the value.
    #[must_use]
    pub fn floor(&self) -> BigInt {
        self.numerator
            .div_floor(&BigInt::from_magnitude(self.denominator.clone()))
    }

    /// The least integer no less than the value.
    #[must_use]
    pub fn ceil(&self) -> BigInt {
        self.neg().floor().neg()
    }

    /// The fractional part, in the half-open interval from zero to one.
    ///
    /// This is the quantity every Gomory derivation is built from, and it is
    /// defined against the floor rather than against truncation, so it is
    /// nonnegative for negative values too.
    #[must_use]
    pub fn frac(&self) -> Self {
        self.sub(&Self::from_int(self.floor()))
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_value(other)
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_integer() {
            formatter.write_str(&self.numerator().to_dec_string())
        } else {
            write!(
                formatter,
                "{}/{}",
                self.numerator().to_dec_string(),
                self.denominator().to_dec_string()
            )
        }
    }
}

impl fmt::Debug for Rational {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Rational({self})")
    }
}

/// What can go wrong constructing a rational.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RationalError {
    /// The denominator was zero.
    ZeroDenominator,
}

impl fmt::Display for RationalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDenominator => formatter.write_str("rational denominator is zero"),
        }
    }
}

impl core::error::Error for RationalError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn rational(numerator: i64, denominator: i64) -> Rational {
        Rational::new(BigInt::from_i64(numerator), BigInt::from_i64(denominator))
            .expect("nonzero denominator")
    }

    #[test]
    fn construction_reduces_by_the_common_divisor() {
        assert_eq!(rational(6, 4), rational(3, 2));
        assert_eq!(rational(12, 4), Rational::from_i64(3));
        assert!(rational(12, 4).is_integer());
    }

    #[test]
    fn the_sign_lives_on_the_numerator() {
        let negative = rational(6, -4);
        assert!(negative.is_negative());
        assert!(negative.numerator().is_negative());
        assert_eq!(negative, rational(-6, 4));
        assert_eq!(negative.denominator().to_dec_string(), "2");
    }

    #[test]
    fn zero_normalizes_its_denominator_to_one() {
        let zero = rational(0, 5);
        assert!(zero.is_zero());
        assert!(zero.is_integer());
        assert_eq!(zero.denominator().to_dec_string(), "1");
        assert_eq!(zero, Rational::zero());
        assert!(!rational(0, -5).is_negative());
    }

    #[test]
    fn a_zero_denominator_is_reported_rather_than_trapped() {
        assert_eq!(
            Rational::new(BigInt::one(), BigInt::zero()),
            Err(RationalError::ZeroDenominator)
        );
        assert_eq!(Rational::zero().recip(), None);
        assert_eq!(Rational::one().checked_div(&Rational::zero()), None);
    }

    #[test]
    fn addition_reduces_further_than_either_operand() {
        assert_eq!(rational(1, 3).add(&rational(1, 6)), rational(1, 2));
        assert!(rational(1, 3).add(&rational(-1, 3)).is_zero());
        assert_eq!(rational(1, 2).sub(&rational(1, 3)), rational(1, 6));
    }

    #[test]
    fn multiplication_cancels_exactly_once() {
        assert_eq!(rational(2, 3).mul(&rational(3, 2)), Rational::one());
        assert_eq!(rational(-2, 3).mul(&rational(3, 4)), rational(-1, 2));
        assert_eq!(
            rational(1, 2)
                .checked_div(&rational(1, 3))
                .expect("nonzero"),
            rational(3, 2)
        );
    }

    #[test]
    fn comparison_by_cross_multiplication_separates_close_approximations() {
        // Three decimal places apart: 355 times 7 is 2485 and 22 times 113 is
        // 2486, so the better approximation of the two is the smaller number.
        assert_eq!(
            rational(355, 113).cmp_value(&rational(22, 7)),
            Ordering::Less
        );
        assert_eq!(
            rational(22, 7).cmp_value(&rational(355, 113)),
            Ordering::Greater
        );
        // One plus two to the minus fifty-third, against one. Both operands
        // round to the same binary64, so a comparison that went through a
        // float would call these equal.
        assert_eq!(
            rational(9_007_199_254_740_993, 9_007_199_254_740_992).cmp_value(&Rational::one()),
            Ordering::Greater
        );
        assert_eq!(rational(1, 3).cmp_value(&rational(2, 6)), Ordering::Equal);
        assert!(rational(-1, 3) < rational(1, 1000000));
    }

    #[test]
    fn floor_and_ceiling_agree_with_the_integer_case() {
        for value in [
            rational(7, 2),
            rational(-7, 2),
            rational(4, 1),
            rational(-4, 1),
        ] {
            let floor = Rational::from_int(value.floor());
            let ceil = Rational::from_int(value.ceil());
            assert!(floor.cmp_value(&value) != Ordering::Greater);
            assert!(ceil.cmp_value(&value) != Ordering::Less);
            assert!(value.sub(&floor).cmp_value(&Rational::one()) == Ordering::Less);
        }
        assert_eq!(rational(7, 2).floor().to_dec_string(), "3");
        assert_eq!(rational(-7, 2).floor().to_dec_string(), "-4");
        assert_eq!(rational(7, 2).ceil().to_dec_string(), "4");
        assert_eq!(rational(-7, 2).ceil().to_dec_string(), "-3");
        assert_eq!(rational(4, 1).floor().to_dec_string(), "4");
        assert_eq!(rational(4, 1).ceil().to_dec_string(), "4");
    }

    #[test]
    fn the_fractional_part_is_never_negative() {
        assert_eq!(rational(7, 2).frac(), rational(1, 2));
        assert_eq!(rational(-7, 2).frac(), rational(1, 2));
        assert!(rational(4, 1).frac().is_zero());
        for value in [
            rational(7, 2),
            rational(-7, 2),
            rational(-1, 3),
            rational(0, 1),
        ] {
            let fraction = value.frac();
            assert!(!fraction.is_negative());
            assert!(fraction.cmp_value(&Rational::one()) == Ordering::Less);
            assert_eq!(Rational::from_int(value.floor()).add(&fraction), value);
        }
    }

    #[test]
    fn the_bit_length_is_the_wider_half() {
        let wide = Rational::new(
            BigInt::from_magnitude(BigUint::one().shl_bits(200)),
            BigInt::from_i64(3),
        )
        .expect("nonzero denominator");
        assert_eq!(wide.bit_len(), 201);
        assert_eq!(Rational::zero().bit_len(), 1);
    }

    #[test]
    fn arithmetic_stays_exact_over_operands_a_float_would_collapse() {
        let tiny = Rational::new(BigInt::one(), BigInt::from_i64(3)).expect("nonzero denominator");
        let huge = Rational::from_int(BigInt::from_magnitude(BigUint::pow10(60)));
        let sum = huge.add(&tiny);
        assert_ne!(sum, huge);
        assert_eq!(sum.sub(&huge), tiny);
    }
}
