//! Signed integers, as a sign beside a magnitude.
//!
//! Two's complement would make addition a single loop and would make every
//! question the proof rules actually ask — is this multiplier nonnegative, is
//! this right-hand side strictly positive, is this lifting coefficient
//! negative — a matter of inspecting a top bit at the right width. Those
//! questions are the ones a verifier gets wrong, so they are answered here by
//! reading a field that says what it means.
//!
//! Zero is never negative. Without that rule there are two zeros, structural
//! equality stops agreeing with numeric equality, and canonical text can print
//! a minus sign in front of nothing.

use core::cmp::Ordering;
use core::fmt;

use super::nat::{Nat, NatError};

/// A signed arbitrary-precision integer.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct Int {
    /// False whenever the magnitude is zero.
    minus: bool,
    size: Nat,
}

impl Int {
    /// Zero.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            minus: false,
            size: Nat::zero(),
        }
    }

    /// One.
    #[must_use]
    pub fn one() -> Self {
        Self::positive(Nat::one())
    }

    /// A value from a sign and a magnitude, with negative zero normalized away.
    #[must_use]
    pub fn signed(minus: bool, size: Nat) -> Self {
        Self {
            minus: minus && !size.is_zero(),
            size,
        }
    }

    /// A nonnegative value from a magnitude.
    #[must_use]
    pub fn positive(size: Nat) -> Self {
        Self::signed(false, size)
    }

    /// A value from a machine word.
    #[must_use]
    pub fn from_i64(value: i64) -> Self {
        Self::signed(value < 0, Nat::from_u64(value.unsigned_abs()))
    }

    /// The magnitude.
    #[must_use]
    pub fn size(&self) -> &Nat {
        &self.size
    }

    /// Whether the value is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.size.is_zero()
    }

    /// Whether the value is strictly below zero.
    #[must_use]
    pub fn is_minus(&self) -> bool {
        self.minus
    }

    /// Whether the value is strictly above zero.
    #[must_use]
    pub fn is_plus(&self) -> bool {
        !self.minus && !self.size.is_zero()
    }

    /// Minus one, zero, or one.
    #[must_use]
    pub fn sign(&self) -> i32 {
        if self.size.is_zero() {
            0
        } else if self.minus {
            -1
        } else {
            1
        }
    }

    /// The absolute value.
    #[must_use]
    pub fn magnitude(&self) -> Self {
        Self::positive(self.size.clone())
    }

    /// The additive inverse.
    #[must_use]
    pub fn negated(&self) -> Self {
        Self::signed(!self.minus, self.size.clone())
    }

    /// Sum.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        if self.minus == other.minus {
            return Self::signed(self.minus, self.size.plus(&other.size));
        }
        match self.size.compare(&other.size) {
            Ordering::Equal => Self::zero(),
            Ordering::Greater => Self::signed(
                self.minus,
                self.size
                    .minus(&other.size)
                    .expect("the larger magnitude leads"),
            ),
            Ordering::Less => Self::signed(
                other.minus,
                other
                    .size
                    .minus(&self.size)
                    .expect("the larger magnitude leads"),
            ),
        }
    }

    /// Difference.
    #[must_use]
    pub fn minus_value(&self, other: &Self) -> Self {
        self.plus(&other.negated())
    }

    /// Product.
    #[must_use]
    pub fn times(&self, other: &Self) -> Self {
        Self::signed(self.minus != other.minus, self.size.times(&other.size))
    }

    /// Quotient and remainder truncated toward zero, or `None` for a zero
    /// divisor.
    #[must_use]
    pub fn divide(&self, divisor: &Self) -> Option<(Self, Self)> {
        let (quotient, remainder) = self.size.divide(&divisor.size)?;
        Some((
            Self::signed(self.minus != divisor.minus, quotient),
            Self::signed(self.minus, remainder),
        ))
    }

    /// Quotient and remainder with the quotient taken toward negative
    /// infinity, so the remainder carries the divisor's sign.
    ///
    /// The cut rules are written against this convention and not against
    /// truncation. The difference shows up only on negative operands, which is
    /// the half of the input space a hand-checked example rarely covers.
    #[must_use]
    pub fn divide_floor(&self, divisor: &Self) -> Option<(Self, Self)> {
        let (quotient, remainder) = self.divide(divisor)?;
        if !remainder.is_zero() && remainder.is_minus() != divisor.is_minus() {
            return Some((quotient.minus_value(&Self::one()), remainder.plus(divisor)));
        }
        Some((quotient, remainder))
    }

    /// Comparison by value.
    #[must_use]
    pub fn compare(&self, other: &Self) -> Ordering {
        match (self.minus, other.minus) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.size.compare(&other.size),
            (true, true) => other.size.compare(&self.size),
        }
    }

    /// The greatest common divisor of the magnitudes.
    #[must_use]
    pub fn gcd(&self, other: &Self) -> Self {
        Self::positive(self.size.gcd(&other.size))
    }

    /// Reads canonical decimal text: digits with an optional leading minus, no
    /// leading plus, no leading zero on a nonzero magnitude, and no negative
    /// zero.
    ///
    /// # Errors
    ///
    /// Returns [`IntError`] naming which of those rules the text broke.
    pub fn from_decimal(text: &str) -> Result<Self, IntError> {
        let (minus, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        if digits.len() > 1 && digits.starts_with('0') {
            return Err(IntError::LeadingZero);
        }
        let size = Nat::from_decimal(digits).map_err(IntError::Magnitude)?;
        if minus && size.is_zero() {
            return Err(IntError::MinusZero);
        }
        Ok(Self::signed(minus, size))
    }

    /// Renders the value in canonical decimal text.
    #[must_use]
    pub fn to_decimal(&self) -> String {
        if self.minus {
            let mut out = String::from("-");
            out.push_str(&self.size.to_decimal());
            out
        } else {
            self.size.to_decimal()
        }
    }
}

impl fmt::Display for Int {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_decimal())
    }
}

impl fmt::Debug for Int {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Int({})", self.to_decimal())
    }
}

/// What can go wrong reading a canonical decimal integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntError {
    /// The magnitude was not decimal digits.
    Magnitude(NatError),
    /// A nonzero magnitude carried a leading zero.
    LeadingZero,
    /// The text spelled negative zero.
    MinusZero,
}

impl fmt::Display for IntError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Magnitude(inner) => write!(formatter, "{inner}"),
            Self::LeadingZero => formatter.write_str("integer text has a leading zero"),
            Self::MinusZero => formatter.write_str("integer text spells negative zero"),
        }
    }
}

impl core::error::Error for IntError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Int {
        Int::from_decimal(text).expect("canonical integer text")
    }

    #[test]
    fn zero_is_never_negative() {
        assert!(!Int::signed(true, Nat::zero()).is_minus());
        assert_eq!(Int::signed(true, Nat::zero()), Int::zero());
        assert_eq!(Int::zero().negated(), Int::zero());
        assert_eq!(Int::zero().to_decimal(), "0");
        assert_eq!(Int::zero().sign(), 0);
    }

    #[test]
    fn addition_takes_the_sign_of_the_larger_magnitude() {
        assert_eq!(read("-7").plus(&read("3")).to_decimal(), "-4");
        assert_eq!(read("7").plus(&read("-3")).to_decimal(), "4");
        assert_eq!(read("-3").plus(&read("7")).to_decimal(), "4");
        assert_eq!(read("-3").plus(&read("-7")).to_decimal(), "-10");
        assert!(read("42").plus(&read("-42")).is_zero());
    }

    #[test]
    fn products_and_differences_carry_the_expected_sign() {
        assert_eq!(read("-6").times(&read("7")).to_decimal(), "-42");
        assert_eq!(read("-6").times(&read("-7")).to_decimal(), "42");
        assert!(!read("-6").times(&Int::zero()).is_minus());
        assert_eq!(read("5").minus_value(&read("9")).to_decimal(), "-4");
    }

    #[test]
    fn truncating_and_floored_division_differ_only_on_negatives() {
        for (dividend, divisor, truncated, floored) in [
            ("7", "2", ("3", "1"), ("3", "1")),
            ("-7", "2", ("-3", "-1"), ("-4", "1")),
            ("7", "-2", ("-3", "1"), ("-4", "-1")),
            ("-7", "-2", ("3", "-1"), ("3", "-1")),
            ("-8", "2", ("-4", "0"), ("-4", "0")),
        ] {
            let (a, b) = (read(dividend), read(divisor));
            let (q, r) = a.divide(&b).expect("nonzero divisor");
            assert_eq!(
                (q.to_decimal().as_str(), r.to_decimal().as_str()),
                truncated,
                "trunc {dividend}/{divisor}"
            );
            assert_eq!(q.times(&b).plus(&r), a);
            let (fq, fr) = a.divide_floor(&b).expect("nonzero divisor");
            assert_eq!(
                (fq.to_decimal().as_str(), fr.to_decimal().as_str()),
                floored,
                "floor {dividend}/{divisor}"
            );
            assert_eq!(fq.times(&b).plus(&fr), a);
            assert!(fr.is_zero() || fr.is_minus() == b.is_minus());
        }
        assert_eq!(Int::one().divide(&Int::zero()), None);
        assert_eq!(Int::one().divide_floor(&Int::zero()), None);
    }

    #[test]
    fn ordering_puts_every_negative_below_every_nonnegative() {
        let mut values = [read("3"), read("-10"), read("0"), read("-1"), read("10")];
        values.sort_by(Int::compare);
        let rendered: Vec<String> = values.iter().map(Int::to_decimal).collect();
        assert_eq!(rendered, vec!["-10", "-1", "0", "3", "10"]);
    }

    #[test]
    fn canonical_text_round_trips_and_refuses_its_near_misses() {
        for text in [
            "0",
            "1",
            "-1",
            "340282366920938463463374607431768211455",
            "-99",
        ] {
            assert_eq!(read(text).to_decimal(), text);
        }
        assert_eq!(Int::from_decimal("-0"), Err(IntError::MinusZero));
        assert_eq!(Int::from_decimal("007"), Err(IntError::LeadingZero));
        assert_eq!(Int::from_decimal("-007"), Err(IntError::LeadingZero));
        assert!(Int::from_decimal("+7").is_err());
        assert!(Int::from_decimal("").is_err());
        assert!(Int::from_decimal("1 ").is_err());
    }

    #[test]
    fn the_divisor_of_two_values_ignores_their_signs() {
        assert_eq!(read("-24").gcd(&read("36")).to_decimal(), "12");
        assert_eq!(read("36").gcd(&read("-24")).to_decimal(), "12");
        assert!(!read("-24").gcd(&read("36")).is_minus());
    }

    #[test]
    fn the_extreme_machine_word_survives_conversion() {
        assert_eq!(Int::from_i64(i64::MIN).to_decimal(), "-9223372036854775808");
        assert_eq!(Int::from_i64(i64::MAX).to_decimal(), "9223372036854775807");
        assert!(Int::from_i64(0).is_zero());
    }
}
