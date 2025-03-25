//! Decimal text and exact magnitudes, in both directions.
//!
//! This is the boundary the exactness rule lives on. A coefficient written
//! `0.1` in an instance file becomes the rational one tenth here, and never
//! the binary64 nearest to it; a parser that goes through a float produces
//! `3602879701896397/36028797018963968`, and a proof about that number is a
//! proof about a model nobody wrote. Conversion in this direction is therefore
//! pure integer arithmetic with no floating-point value anywhere on the path.
//!
//! Both directions work nineteen digits at a time, which is the largest power
//! of ten that fits in a limb. A digit-at-a-time loop would be the same shape
//! and nineteen times the passes.

use core::fmt;

use crate::uint::BigUint;

/// The largest power of ten that fits in one limb, and its exponent.
pub(crate) const DEC_CHUNK_POW: u64 = 10_000_000_000_000_000_000;
pub(crate) const DEC_CHUNK_DIGITS: usize = 19;

impl BigUint {
    /// Reads a decimal magnitude with no sign, no separators, and at least one
    /// digit. Leading zeros are accepted and are not significant.
    ///
    /// # Errors
    ///
    /// Returns [`ParseUintError`] when the text is empty or holds a character
    /// that is not an ASCII digit.
    pub fn parse_dec(text: &str) -> Result<Self, ParseUintError> {
        if text.is_empty() {
            return Err(ParseUintError::Empty);
        }
        let bytes = text.as_bytes();
        if let Some(&bad) = bytes.iter().find(|byte| !byte.is_ascii_digit()) {
            return Err(ParseUintError::NotADigit(bad));
        }
        let mut out = Self::zero();
        let mut position = 0usize;
        while position < bytes.len() {
            let take = core::cmp::min(DEC_CHUNK_DIGITS, bytes.len() - position);
            let chunk = &text[position..position + take];
            // The chunk is at most nineteen ASCII digits, so it is inside the
            // limb range by construction and the parse cannot fail.
            let value: u64 = chunk.parse().map_err(|_| ParseUintError::Empty)?;
            out = out.mul_small(pow10_u64(take as u32)).add_small(value);
            position += take;
        }
        Ok(out)
    }

    /// Renders the value in decimal with no leading zeros.
    #[must_use]
    pub fn to_dec_string(&self) -> String {
        if self.is_zero() {
            return "0".to_owned();
        }
        let mut chunks = Vec::new();
        let mut current = self.clone();
        while !current.is_zero() {
            let (quotient, remainder) = current.divrem_small(DEC_CHUNK_POW);
            chunks.push(remainder);
            current = quotient;
        }
        // The most significant chunk is written bare; every later one is
        // padded, because a chunk of value seven in the middle of a number is
        // eighteen zeros and a seven.
        let mut out = String::new();
        for (index, chunk) in chunks.iter().rev().enumerate() {
            if index == 0 {
                out.push_str(&chunk.to_string());
            } else {
                out.push_str(&format!("{chunk:0>DEC_CHUNK_DIGITS$}"));
            }
        }
        out
    }

    /// Ten raised to an exponent, exactly.
    #[must_use]
    pub fn pow10(exponent: u32) -> Self {
        let mut out = Self::one();
        let mut remaining = exponent;
        while remaining >= DEC_CHUNK_DIGITS as u32 {
            out = out.mul_small(DEC_CHUNK_POW);
            remaining -= DEC_CHUNK_DIGITS as u32;
        }
        if remaining > 0 {
            out = out.mul_small(pow10_u64(remaining));
        }
        out
    }
}

/// Ten raised to an exponent that fits in a limb.
fn pow10_u64(exponent: u32) -> u64 {
    debug_assert!(exponent as usize <= DEC_CHUNK_DIGITS);
    let mut out = 1u64;
    for _ in 0..exponent {
        out *= 10;
    }
    out
}

/// What can go wrong reading a decimal magnitude.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseUintError {
    /// The text held no digits at all.
    Empty,
    /// The text held a byte that is not an ASCII digit.
    NotADigit(u8),
}

impl fmt::Display for ParseUintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("decimal magnitude is empty"),
            Self::NotADigit(byte) => {
                write!(
                    formatter,
                    "decimal magnitude holds a non-digit byte {byte:#04x}"
                )
            }
        }
    }
}

impl core::error::Error for ParseUintError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> BigUint {
        BigUint::parse_dec(text).expect("vector operand parses")
    }

    #[test]
    fn decimal_round_trips_across_a_chunk_boundary() {
        for text in [
            "0",
            "1",
            "9999999999999999999",
            "10000000000000000000",
            "10000000000000000001",
            "340282366920938463463374607431768211455",
            "222232244629420445529739893461909967206666939096499764990979600",
        ] {
            assert_eq!(parse(text).to_dec_string(), text);
        }
    }

    #[test]
    fn an_interior_chunk_keeps_its_leading_zeros() {
        // Ten to the twentieth is a one, then a chunk that is eighteen zeros
        // and a zero. An implementation that writes each chunk bare loses them
        // and reports ten to the second.
        assert_eq!(BigUint::pow10(20).to_dec_string().len(), 21);
        let value = BigUint::pow10(20).add_small(7);
        assert_eq!(value.to_dec_string(), "100000000000000000007");
    }

    #[test]
    fn leading_zeros_are_read_but_never_written() {
        assert_eq!(parse("00700").to_dec_string(), "700");
        assert_eq!(parse("0000000000000000000000").to_dec_string(), "0");
    }

    #[test]
    fn a_non_digit_is_named_rather_than_ignored() {
        assert_eq!(BigUint::parse_dec(""), Err(ParseUintError::Empty));
        assert_eq!(
            BigUint::parse_dec("12a4"),
            Err(ParseUintError::NotADigit(b'a'))
        );
        assert_eq!(
            BigUint::parse_dec("-1"),
            Err(ParseUintError::NotADigit(b'-'))
        );
        assert_eq!(
            BigUint::parse_dec(" 1"),
            Err(ParseUintError::NotADigit(b' '))
        );
        assert_eq!(
            BigUint::parse_dec("1_0"),
            Err(ParseUintError::NotADigit(b'_'))
        );
    }

    #[test]
    fn powers_of_ten_are_exact_across_chunk_boundaries() {
        assert_eq!(BigUint::pow10(0).to_dec_string(), "1");
        assert_eq!(BigUint::pow10(19).to_dec_string(), "10000000000000000000");
        assert_eq!(BigUint::pow10(20).to_dec_string(), "100000000000000000000");
        let mut expected = String::from("1");
        expected.push_str(&"0".repeat(45));
        assert_eq!(BigUint::pow10(45).to_dec_string(), expected);
    }

    #[test]
    fn a_hundred_digit_value_survives_the_round_trip() {
        let text = "1234567890".repeat(10);
        assert_eq!(parse(&text).to_dec_string(), text);
    }
}
