//! Signed arbitrary-precision integers, as a sign and a magnitude.
//!
//! Sign-magnitude rather than two's complement, because every consumer here
//! asks about the sign directly — a multiplier must be nonnegative, a
//! contradiction's right-hand side must be strictly positive, a lifting
//! coefficient must not be negative — and a representation that answers those
//! questions by inspecting a field is one fewer place to be wrong.
//!
//! The invariant is that zero is never negative. Without it there are two
//! encodings of zero, structural equality stops meaning numeric equality, and
//! a canonical rational text could print a minus sign in front of nothing.

use core::cmp::Ordering;
use core::fmt;

use crate::dec::ParseUintError;
use crate::uint::BigUint;

/// An arbitrary-precision signed integer.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct BigInt {
    /// Never true when the magnitude is zero.
    negative: bool,
    magnitude: BigUint,
}

impl BigInt {
    /// The additive identity.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            negative: false,
            magnitude: BigUint::zero(),
        }
    }

    /// The multiplicative identity.
    #[must_use]
    pub fn one() -> Self {
        Self::from_i64(1)
    }

    /// Negative one.
    #[must_use]
    pub fn minus_one() -> Self {
        Self::from_i64(-1)
    }

    /// Builds a value from a sign and a magnitude, normalizing negative zero.
    #[must_use]
    pub fn from_sign_magnitude(negative: bool, magnitude: BigUint) -> Self {
        let negative = negative && !magnitude.is_zero();
        Self {
            negative,
            magnitude,
        }
    }

    /// Builds a nonnegative value from a magnitude.
    #[must_use]
    pub fn from_magnitude(magnitude: BigUint) -> Self {
        Self::from_sign_magnitude(false, magnitude)
    }

    /// Builds a value from a machine word.
    #[must_use]
    pub fn from_i64(value: i64) -> Self {
        Self::from_sign_magnitude(
            value < 0,
            BigUint::from_u128(u128::from(value.unsigned_abs())),
        )
    }

    /// Builds a value from an unsigned machine word.
    #[must_use]
    pub fn from_u64(value: u64) -> Self {
        Self::from_magnitude(BigUint::from_u64(value))
    }

    /// The magnitude, without its sign.
    #[must_use]
    pub fn magnitude(&self) -> &BigUint {
        &self.magnitude
    }

    /// Whether the value is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.magnitude.is_zero()
    }

    /// Whether the value is strictly negative.
    #[must_use]
    pub fn is_negative(&self) -> bool {
        self.negative
    }

    /// Whether the value is strictly positive.
    #[must_use]
    pub fn is_positive(&self) -> bool {
        !self.negative && !self.magnitude.is_zero()
    }

    /// Minus one, zero, or one.
    #[must_use]
    pub fn signum(&self) -> i32 {
        if self.magnitude.is_zero() {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    /// The absolute value.
    #[must_use]
    pub fn abs(&self) -> Self {
        Self::from_magnitude(self.magnitude.clone())
    }

    /// The additive inverse.
    #[must_use]
    pub fn neg(&self) -> Self {
        Self::from_sign_magnitude(!self.negative, self.magnitude.clone())
    }

    /// Sum.
    #[must_use]
    pub fn add(&self, rhs: &Self) -> Self {
        if self.negative == rhs.negative {
            return Self::from_sign_magnitude(self.negative, self.magnitude.add(&rhs.magnitude));
        }
        match self.magnitude.cmp_value(&rhs.magnitude) {
            Ordering::Equal => Self::zero(),
            Ordering::Greater => {
                Self::from_sign_magnitude(self.negative, self.magnitude.sub(&rhs.magnitude))
            }
            Ordering::Less => {
                Self::from_sign_magnitude(rhs.negative, rhs.magnitude.sub(&self.magnitude))
            }
        }
    }

    /// Difference.
    #[must_use]
    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.neg())
    }

    /// Product.
    #[must_use]
    pub fn mul(&self, rhs: &Self) -> Self {
        Self::from_sign_magnitude(
            self.negative != rhs.negative,
            self.magnitude.mul(&rhs.magnitude),
        )
    }

    /// Quotient and remainder, truncating the quotient toward zero, or `None`
    /// when the divisor is zero.
    ///
    /// Truncating division is chosen because the remainder then carries the
    /// sign of the dividend, which is what the identity
    /// `dividend = quotient * divisor + remainder` needs in order to hold with
    /// the magnitudes this crate computes. Floored division is available
    /// separately, spelled as what it is, so no call site has to remember
    /// which convention it is getting.
    #[must_use]
    pub fn checked_divrem(&self, divisor: &Self) -> Option<(Self, Self)> {
        let (quotient, remainder) = self.magnitude.checked_divrem(&divisor.magnitude)?;
        Some((
            Self::from_sign_magnitude(self.negative != divisor.negative, quotient),
            Self::from_sign_magnitude(self.negative, remainder),
        ))
    }

    /// Quotient and remainder with the quotient floored toward negative
    /// infinity, so the remainder carries the sign of the divisor.
    ///
    /// This is the division the integer rounding rules want. The floor of a
    /// negative quotient is not its truncation, and a cut formula written
    /// against the wrong convention is correct on every positive coefficient
    /// and wrong on every negative one — which is exactly the shape of defect
    /// that survives a test suite built from textbook examples.
    #[must_use]
    pub fn checked_divrem_floor(&self, divisor: &Self) -> Option<(Self, Self)> {
        let (quotient, remainder) = self.checked_divrem(divisor)?;
        if !remainder.is_zero() && remainder.is_negative() != divisor.is_negative() {
            return Some((quotient.sub(&Self::one()), remainder.add(divisor)));
        }
        Some((quotient, remainder))
    }

    /// The floor of the quotient.
    ///
    /// # Panics
    ///
    /// Panics when `divisor` is zero.
    #[must_use]
    pub fn div_floor(&self, divisor: &Self) -> Self {
        self.checked_divrem_floor(divisor)
            .expect("BigInt::div_floor by zero")
            .0
    }

    /// Ordering by value.
    #[must_use]
    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.magnitude.cmp_value(&rhs.magnitude),
            (true, true) => rhs.magnitude.cmp_value(&self.magnitude),
        }
    }

    /// The greatest common divisor of the magnitudes, always nonnegative.
    #[must_use]
    pub fn gcd(&self, other: &Self) -> Self {
        Self::from_magnitude(self.magnitude.gcd(&other.magnitude))
    }

    /// Reads a decimal integer with an optional leading minus.
    ///
    /// A leading plus is not accepted, and neither is the text `-0`: both are
    /// spellings the canonical form forbids, and accepting them here would let
    /// a certificate carry a number with two names.
    ///
    /// # Errors
    ///
    /// Returns [`ParseIntError`] when the text is not a canonical decimal
    /// integer.
    pub fn parse_dec(text: &str) -> Result<Self, ParseIntError> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        if digits.len() > 1 && digits.starts_with('0') {
            return Err(ParseIntError::LeadingZero);
        }
        let magnitude = BigUint::parse_dec(digits).map_err(ParseIntError::Magnitude)?;
        if negative && magnitude.is_zero() {
            return Err(ParseIntError::NegativeZero);
        }
        Ok(Self::from_sign_magnitude(negative, magnitude))
    }

    /// Renders the value in canonical decimal text.
    #[must_use]
    pub fn to_dec_string(&self) -> String {
        if self.negative {
            let mut out = String::from("-");
            out.push_str(&self.magnitude.to_dec_string());
            out
        } else {
            self.magnitude.to_dec_string()
        }
    }
}

impl PartialOrd for BigInt {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BigInt {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_value(other)
    }
}

impl fmt::Display for BigInt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_dec_string())
    }
}

impl fmt::Debug for BigInt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "BigInt({})", self.to_dec_string())
    }
}

/// What can go wrong reading a canonical decimal integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseIntError {
    /// The magnitude was not a decimal magnitude.
    Magnitude(ParseUintError),
    /// The text carried a leading zero on a nonzero magnitude.
    LeadingZero,
    /// The text spelled negative zero, which does not exist here.
    NegativeZero,
}

impl fmt::Display for ParseIntError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Magnitude(inner) => write!(formatter, "{inner}"),
            Self::LeadingZero => formatter.write_str("decimal integer has a leading zero"),
            Self::NegativeZero => formatter.write_str("decimal integer spells negative zero"),
        }
    }
}

impl core::error::Error for ParseIntError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> BigInt {
        BigInt::parse_dec(text).expect("vector operand parses")
    }

    #[test]
    fn zero_is_never_negative() {
        let zero = BigInt::from_sign_magnitude(true, BigUint::zero());
        assert!(!zero.is_negative());
        assert_eq!(zero, BigInt::zero());
        assert_eq!(zero.to_dec_string(), "0");
        assert_eq!(BigInt::zero().neg(), BigInt::zero());
        assert_eq!(BigInt::from_i64(0).signum(), 0);
    }

    #[test]
    fn addition_of_opposite_signs_cancels_exactly() {
        let a = parse("340282366920938463463374607431768211455");
        assert!(a.add(&a.neg()).is_zero());
        assert_eq!(a.add(&a.neg()).to_dec_string(), "0");
    }

    #[test]
    fn addition_takes_the_sign_of_the_larger_magnitude() {
        assert_eq!(parse("-7").add(&parse("3")).to_dec_string(), "-4");
        assert_eq!(parse("7").add(&parse("-3")).to_dec_string(), "4");
        assert_eq!(parse("-3").add(&parse("7")).to_dec_string(), "4");
        assert_eq!(parse("-3").add(&parse("-7")).to_dec_string(), "-10");
    }

    #[test]
    fn subtraction_is_addition_of_the_inverse() {
        let a = parse("-123456789012345678901234567890");
        let b = parse("98765432109876543210");
        assert_eq!(a.sub(&b), a.add(&b.neg()));
        assert_eq!(a.sub(&a), BigInt::zero());
        assert_eq!(b.sub(&a).add(&a), b);
    }

    #[test]
    fn products_carry_the_expected_sign() {
        let a = parse("-6");
        let b = parse("7");
        assert_eq!(a.mul(&b).to_dec_string(), "-42");
        assert_eq!(a.mul(&a).to_dec_string(), "36");
        assert_eq!(a.mul(&BigInt::zero()).to_dec_string(), "0");
        assert!(!a.mul(&BigInt::zero()).is_negative());
    }

    #[test]
    fn truncating_division_reconstructs_the_dividend() {
        for (dividend, divisor, quotient, remainder) in [
            ("7", "2", "3", "1"),
            ("-7", "2", "-3", "-1"),
            ("7", "-2", "-3", "1"),
            ("-7", "-2", "3", "-1"),
        ] {
            let a = parse(dividend);
            let b = parse(divisor);
            let (q, r) = a.checked_divrem(&b).expect("nonzero divisor");
            assert_eq!(q.to_dec_string(), quotient, "{dividend} / {divisor}");
            assert_eq!(r.to_dec_string(), remainder, "{dividend} % {divisor}");
            assert_eq!(q.mul(&b).add(&r), a);
        }
        assert_eq!(BigInt::one().checked_divrem(&BigInt::zero()), None);
    }

    #[test]
    fn floored_division_gives_the_remainder_the_sign_of_the_divisor() {
        for (dividend, divisor, quotient, remainder) in [
            ("7", "2", "3", "1"),
            ("-7", "2", "-4", "1"),
            ("7", "-2", "-4", "-1"),
            ("-7", "-2", "3", "-1"),
            ("-8", "2", "-4", "0"),
        ] {
            let a = parse(dividend);
            let b = parse(divisor);
            let (q, r) = a.checked_divrem_floor(&b).expect("nonzero divisor");
            assert_eq!(q.to_dec_string(), quotient, "floor {dividend} / {divisor}");
            assert_eq!(r.to_dec_string(), remainder, "floor {dividend} % {divisor}");
            assert_eq!(q.mul(&b).add(&r), a);
            assert_eq!(a.div_floor(&b), q);
            // The floored quotient never exceeds the truncated one, and drops
            // below it exactly when the truncation rounded toward zero from a
            // negative value.
            assert!(
                q.cmp_value(&a.checked_divrem(&b).expect("nonzero divisor").0) != Ordering::Greater
            );
        }
    }

    #[test]
    fn ordering_places_every_negative_below_every_nonnegative() {
        let mut values = [
            parse("3"),
            parse("-10"),
            parse("0"),
            parse("-1"),
            parse("10"),
        ];
        values.sort();
        let rendered: Vec<String> = values.iter().map(BigInt::to_dec_string).collect();
        assert_eq!(rendered, vec!["-10", "-1", "0", "3", "10"]);
    }

    #[test]
    fn canonical_text_round_trips_and_rejects_its_own_near_misses() {
        for text in [
            "0",
            "1",
            "-1",
            "340282366920938463463374607431768211455",
            "-99",
        ] {
            assert_eq!(parse(text).to_dec_string(), text);
        }
        assert_eq!(BigInt::parse_dec("-0"), Err(ParseIntError::NegativeZero));
        assert_eq!(BigInt::parse_dec("007"), Err(ParseIntError::LeadingZero));
        assert_eq!(BigInt::parse_dec("-007"), Err(ParseIntError::LeadingZero));
        assert_eq!(
            BigInt::parse_dec("+7").unwrap_err(),
            ParseIntError::Magnitude(ParseUintError::NotADigit(b'+'))
        );
        assert!(BigInt::parse_dec("0").is_ok());
        assert!(BigInt::parse_dec("").is_err());
    }

    #[test]
    fn the_divisor_of_two_magnitudes_ignores_their_signs() {
        let a = parse("-24");
        let b = parse("36");
        assert_eq!(a.gcd(&b).to_dec_string(), "12");
        assert_eq!(b.gcd(&a).to_dec_string(), "12");
        assert!(!a.gcd(&b).is_negative());
    }

    #[test]
    fn machine_word_conversion_handles_the_extreme() {
        assert_eq!(
            BigInt::from_i64(i64::MIN).to_dec_string(),
            "-9223372036854775808"
        );
        assert_eq!(
            BigInt::from_i64(i64::MAX).to_dec_string(),
            "9223372036854775807"
        );
        assert_eq!(
            BigInt::from_u64(u64::MAX).to_dec_string(),
            "18446744073709551615"
        );
    }
}
