//! Canonical rational text, and the decimal tokens instance files are written
//! in.
//!
//! Two grammars meet here and they are deliberately different. Canonical text
//! is what a certificate and a canonical dump carry: an integer, or `p/q` with
//! `q` strictly greater than one and coprime with `p`. It has exactly one
//! spelling per value, no exponent, no decimal point, and no leading plus. A
//! decimal token is what an instance file is allowed to contain: a sign,
//! digits with at most one point, and an optional exponent.
//!
//! The token becomes the rational its characters name. There is no binary
//! floating-point value anywhere on that path. A parser that reads `0.1` as
//! the nearest `f64` and converts afterwards produces
//! `3602879701896397/36028797018963968`, and every theorem proved about that
//! number is a theorem about a model nobody wrote.
//!
//! The bit projection is the other half of the contract. A four-character
//! exponent can name a rational with more digits than the machine has memory,
//! so the size of the value is bounded from the token's own characters, in
//! machine integers, before a single limb is allocated. A cap checked after
//! the allocation it was meant to prevent is decoration.

use core::fmt;

use crate::int::BigInt;
use crate::rational::Rational;
use crate::uint::BigUint;

impl Rational {
    /// Renders the value in canonical rational text.
    #[must_use]
    pub fn to_canonical_string(&self) -> String {
        if self.is_integer() {
            return self.numerator().to_dec_string();
        }
        let mut out = self.numerator().to_dec_string();
        out.push('/');
        out.push_str(&self.denominator().to_dec_string());
        out
    }

    /// Reads canonical rational text, rejecting every non-canonical spelling
    /// of the same value.
    ///
    /// # Errors
    ///
    /// Returns [`TextError`] when the text is not exactly the canonical
    /// spelling of a rational.
    pub fn parse_canonical(text: &str) -> Result<Self, TextError> {
        let Some((left, right)) = text.split_once('/') else {
            let value = BigInt::parse_dec(text).map_err(|_| TextError::NotCanonical)?;
            return Ok(Self::from_int(value));
        };
        if right.contains('/') {
            return Err(TextError::NotCanonical);
        }
        let numerator = BigInt::parse_dec(left).map_err(|_| TextError::NotCanonical)?;
        if right.starts_with('-') {
            return Err(TextError::NegativeDenominator);
        }
        if right.len() > 1 && right.starts_with('0') {
            return Err(TextError::NotCanonical);
        }
        let denominator = BigUint::parse_dec(right).map_err(|_| TextError::NotCanonical)?;
        if denominator.is_zero() {
            return Err(TextError::ZeroDenominator);
        }
        if denominator.cmp_value(&BigUint::one()) == core::cmp::Ordering::Equal {
            // `3/1` names the same number as `3`, so it is not canonical text.
            return Err(TextError::UnreducedDenominatorOfOne);
        }
        if numerator.is_zero() {
            return Err(TextError::UnreducedZero);
        }
        if numerator
            .magnitude()
            .gcd(&denominator)
            .cmp_value(&BigUint::one())
            != core::cmp::Ordering::Equal
        {
            return Err(TextError::Unreduced);
        }
        Self::new(numerator, BigInt::from_magnitude(denominator))
            .map_err(|_| TextError::ZeroDenominator)
    }

    /// Reads a decimal token from an instance file, subject to a token length
    /// cap and a projected bit cap.
    ///
    /// The checks run in the order the numeric fence names: token length,
    /// then the bit projection computed from the characters alone, then
    /// construction.
    ///
    /// # Errors
    ///
    /// Returns [`TextError`] naming the fence that stopped the token.
    pub fn parse_decimal_within(
        text: &str,
        max_token_bytes: usize,
        max_bits: u64,
    ) -> Result<Self, TextError> {
        if text.len() > max_token_bytes {
            return Err(TextError::TokenTooLong {
                bytes: text.len(),
                limit: max_token_bytes,
            });
        }
        let parts = DecimalToken::split(text)?;
        let projected = parts.projected_bits();
        if projected > max_bits {
            return Err(TextError::ProjectedBitsExceeded {
                projected,
                limit: max_bits,
            });
        }
        Ok(parts.build())
    }

    /// Reads a decimal token with no caps applied.
    ///
    /// Only vectors and internal callers use this. Anything reading bytes that
    /// came from outside the process goes through
    /// [`Rational::parse_decimal_within`], so that the fence cannot be
    /// forgotten at a call site.
    ///
    /// # Errors
    ///
    /// Returns [`TextError`] when the token is not a decimal number.
    pub fn parse_decimal(text: &str) -> Result<Self, TextError> {
        Ok(DecimalToken::split(text)?.build())
    }
}

/// A decimal token, split into its parts but not yet evaluated.
struct DecimalToken<'a> {
    negative: bool,
    integer_digits: &'a str,
    fraction_digits: &'a str,
    exponent: i64,
}

impl<'a> DecimalToken<'a> {
    /// Splits a token into sign, digits, and exponent without building any
    /// arbitrary-precision value.
    fn split(text: &'a str) -> Result<Self, TextError> {
        if text.is_empty() {
            return Err(TextError::Malformed);
        }
        let (negative, rest) = match text.as_bytes()[0] {
            b'-' => (true, &text[1..]),
            b'+' => return Err(TextError::Malformed),
            _ => (false, text),
        };

        let (mantissa, exponent_text) = match rest.find(['e', 'E']) {
            Some(at) => (&rest[..at], Some(&rest[at + 1..])),
            None => (rest, None),
        };

        let (integer_digits, fraction_digits) = match mantissa.split_once('.') {
            Some((left, right)) => (left, right),
            None => (mantissa, ""),
        };
        if fraction_digits.contains('.') {
            return Err(TextError::Malformed);
        }
        if integer_digits.is_empty() && fraction_digits.is_empty() {
            return Err(TextError::Malformed);
        }
        for digits in [integer_digits, fraction_digits] {
            if digits.bytes().any(|byte| !byte.is_ascii_digit()) {
                return Err(TextError::Malformed);
            }
        }

        let exponent = match exponent_text {
            None => 0,
            Some(raw) => {
                let (sign, magnitude) = match raw.as_bytes().first() {
                    Some(b'-') => (-1i64, &raw[1..]),
                    Some(b'+') => (1i64, &raw[1..]),
                    _ => (1i64, raw),
                };
                if magnitude.is_empty() || magnitude.bytes().any(|byte| !byte.is_ascii_digit()) {
                    return Err(TextError::Malformed);
                }
                // A token whose exponent does not fit in a machine integer is
                // beyond every cap by an enormous margin, and saying so here
                // keeps the projection below in machine arithmetic.
                let value: i64 = magnitude
                    .parse()
                    .map_err(|_| TextError::ExponentOutOfRange)?;
                sign * value
            }
        };

        Ok(Self {
            negative,
            integer_digits,
            fraction_digits,
            exponent,
        })
    }

    /// A sound upper bound on the bit length of the value's numerator and
    /// denominator, computed from the token's characters in machine integers.
    ///
    /// The bound is not tight and does not need to be. It needs to be an
    /// overestimate that can never be computed by allocating the number it is
    /// meant to keep out of memory.
    fn projected_bits(&self) -> u64 {
        // Ten to the k needs fewer than 4k bits, and a run of d digits is
        // below ten to the d, so 4 bits per digit bounds the mantissa.
        let digits = (self.integer_digits.len() + self.fraction_digits.len()) as u64;
        let net = self.exponent - self.fraction_digits.len() as i64;
        let shift = net.unsigned_abs();
        digits
            .saturating_mul(4)
            .saturating_add(shift.saturating_mul(4))
            .saturating_add(1)
    }

    /// Evaluates the token exactly.
    fn build(&self) -> Rational {
        let mut digits =
            String::with_capacity(self.integer_digits.len() + self.fraction_digits.len());
        digits.push_str(self.integer_digits);
        digits.push_str(self.fraction_digits);
        let mantissa = BigUint::parse_dec(&digits).unwrap_or_else(|_| BigUint::zero());
        let net = self.exponent - self.fraction_digits.len() as i64;

        let (numerator, denominator) = if net >= 0 {
            (mantissa.mul(&BigUint::pow10(net as u32)), BigUint::one())
        } else {
            (mantissa, BigUint::pow10(net.unsigned_abs() as u32))
        };
        Rational::new(
            BigInt::from_sign_magnitude(self.negative, numerator),
            BigInt::from_magnitude(denominator),
        )
        .expect("a power of ten is never zero")
    }
}

/// What can go wrong reading rational or decimal text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextError {
    /// The token is not a decimal number at all.
    Malformed,
    /// The text is not the canonical spelling of a rational.
    NotCanonical,
    /// The denominator carried a minus sign, which has no canonical form.
    NegativeDenominator,
    /// The denominator was zero.
    ZeroDenominator,
    /// The denominator was one, which canonical text never writes.
    UnreducedDenominatorOfOne,
    /// Zero was written as a fraction rather than as the bare digit.
    UnreducedZero,
    /// Numerator and denominator shared a factor.
    Unreduced,
    /// The token was longer than the accepted length.
    TokenTooLong {
        /// The token's length in bytes.
        bytes: usize,
        /// The accepted length.
        limit: usize,
    },
    /// The exponent did not fit in a machine integer.
    ExponentOutOfRange,
    /// The value the token names is larger than the accepted bit cap.
    ProjectedBitsExceeded {
        /// The bound computed from the token.
        projected: u64,
        /// The accepted bit length.
        limit: u64,
    },
}

impl fmt::Display for TextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => formatter.write_str("numeric token is malformed"),
            Self::NotCanonical => formatter.write_str("rational text is not in canonical form"),
            Self::NegativeDenominator => formatter.write_str("rational denominator carries a sign"),
            Self::ZeroDenominator => formatter.write_str("rational denominator is zero"),
            Self::UnreducedDenominatorOfOne => {
                formatter.write_str("canonical rational text never writes a denominator of one")
            }
            Self::UnreducedZero => formatter.write_str("canonical zero is written as a bare digit"),
            Self::Unreduced => formatter.write_str("rational text is not reduced to lowest terms"),
            Self::TokenTooLong { bytes, limit } => {
                write!(
                    formatter,
                    "numeric token is {bytes} bytes, over the limit of {limit}"
                )
            }
            Self::ExponentOutOfRange => formatter.write_str("numeric exponent is out of range"),
            Self::ProjectedBitsExceeded { projected, limit } => write!(
                formatter,
                "numeric token projects up to {projected} bits, over the cap of {limit}"
            ),
        }
    }
}

impl core::error::Error for TextError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(text: &str) -> Rational {
        Rational::parse_decimal(text).expect("vector token parses")
    }

    #[test]
    fn a_tenth_is_a_tenth_and_not_the_nearest_float() {
        let value = decimal("0.1");
        assert_eq!(value.to_canonical_string(), "1/10");
        assert_eq!(value.denominator().to_dec_string(), "10");
    }

    #[test]
    fn exponents_clear_the_fraction() {
        assert_eq!(decimal("1.5e2").to_canonical_string(), "150");
        assert_eq!(decimal("2.5E+3").to_canonical_string(), "2500");
        assert_eq!(decimal("15e1").to_canonical_string(), "150");
    }

    // Tokens carrying a negative exponent are exercised by the vector harness
    // rather than here. Their spelling collides with the workspace ban on bare
    // scientific-notation literals — the ban that keeps a tolerance from being
    // written at a call site — and the asset is where that data belongs
    // anyway: both arithmetic implementations read the same file, and neither
    // reads a copy transcribed into the other's tests.

    #[test]
    fn signed_zero_in_the_input_maps_to_canonical_zero() {
        assert_eq!(decimal("-0.000").to_canonical_string(), "0");
        assert!(decimal("-0.000").is_zero());
        assert!(!decimal("-0.000").is_negative());
        assert_eq!(decimal("0").to_canonical_string(), "0");
    }

    #[test]
    fn sixteen_threes_are_not_one_third() {
        let value = decimal("0.3333333333333333");
        assert_eq!(
            value.to_canonical_string(),
            "3333333333333333/10000000000000000"
        );
        assert_ne!(
            value,
            decimal("1").checked_div(&decimal("3")).expect("nonzero")
        );
    }

    #[test]
    fn insignificant_zeros_never_reach_the_canonical_text() {
        assert_eq!(decimal("007.50").to_canonical_string(), "15/2");
        assert_eq!(decimal("12.375").to_canonical_string(), "99/8");
        assert_eq!(decimal(".5").to_canonical_string(), "1/2");
        assert_eq!(decimal("5.").to_canonical_string(), "5");
    }

    #[test]
    fn a_malformed_token_is_named_rather_than_guessed_at() {
        for text in [
            "", "+1", "1.2.3", ".", "-", "1e", "1e+", "nan", "inf", "1e2x", "1 2",
        ] {
            assert!(Rational::parse_decimal(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn the_bit_projection_rejects_before_the_value_is_built() {
        // Twelve characters naming a rational of roughly three hundred million
        // bits. The rejection has to come from the characters. The projection
        // is symmetric in the sign of the exponent, so this direction tests
        // the same arithmetic.
        let token = "1.0e99999999";
        let error = Rational::parse_decimal_within(token, 4096, 1_048_576).unwrap_err();
        assert!(matches!(error, TextError::ProjectedBitsExceeded { .. }));
    }

    #[test]
    fn the_token_length_fence_precedes_the_projection() {
        let long = format!("0.{}", "1".repeat(5000));
        let error = Rational::parse_decimal_within(&long, 4096, 1_048_576).unwrap_err();
        assert!(matches!(error, TextError::TokenTooLong { .. }));
    }

    #[test]
    fn a_token_inside_both_fences_is_built() {
        let value = Rational::parse_decimal_within("1.5e2", 4096, 1_048_576).expect("inside");
        assert_eq!(value.to_canonical_string(), "150");
    }

    #[test]
    fn an_exponent_beyond_a_machine_integer_is_reported() {
        assert_eq!(
            Rational::parse_decimal("1e99999999999999999999"),
            Err(TextError::ExponentOutOfRange)
        );
    }

    #[test]
    fn canonical_text_round_trips() {
        for text in ["0", "1", "-1", "3/2", "-3/2", "1/10", "150", "-1/80"] {
            let value = Rational::parse_canonical(text).expect("canonical");
            assert_eq!(value.to_canonical_string(), text);
        }
    }

    #[test]
    fn near_misses_of_canonical_text_are_rejected() {
        for (text, expected) in [
            ("3/1", TextError::UnreducedDenominatorOfOne),
            ("6/4", TextError::Unreduced),
            ("0/5", TextError::UnreducedZero),
            ("1/-2", TextError::NegativeDenominator),
            ("1/0", TextError::ZeroDenominator),
            ("+3", TextError::NotCanonical),
            ("1.5", TextError::NotCanonical),
            ("1e2", TextError::NotCanonical),
            ("03/2", TextError::NotCanonical),
            ("1/02", TextError::NotCanonical),
            ("-0", TextError::NotCanonical),
            ("1/2/3", TextError::NotCanonical),
            (" 3", TextError::NotCanonical),
        ] {
            assert_eq!(
                Rational::parse_canonical(text),
                Err(expected),
                "for {text:?}"
            );
        }
    }

    #[test]
    fn canonical_text_and_decimal_text_agree_where_both_are_legal() {
        for text in ["0", "1", "-1", "150", "2500"] {
            assert_eq!(
                Rational::parse_canonical(text).expect("canonical"),
                Rational::parse_decimal(text).expect("decimal")
            );
        }
    }
}
