//! Reading and writing numbers as text, on the verifier's side.
//!
//! Two grammars. Canonical rational text is what certificates and canonical
//! dumps carry, and it has exactly one spelling per value. A decimal token is
//! what an instance file may contain, and it has many spellings per value —
//! which is why the verifier reads the file itself instead of accepting the
//! solver's account of what the file said.
//!
//! The scanner below walks the token byte by byte through explicit states.
//! That is more code than splitting on separators and it is what the verifier
//! should be: every accepted shape is visible in one place, and the shapes
//! that are not accepted are not accepted by omission rather than by a
//! separator happening not to be found.
//!
//! Size is bounded before value. The projected width of the number a token
//! names is computed from the token's own characters, in machine arithmetic,
//! and compared against the cap before anything is allocated. A four-character
//! exponent can name a number larger than memory; a check that runs after the
//! allocation it was meant to prevent has prevented nothing.

use core::fmt;

use crate::vint::{Int, Nat};
use crate::vrat::Frac;

impl Frac {
    /// Renders the value as canonical rational text.
    #[must_use]
    pub fn to_canonical(&self) -> String {
        if self.is_whole() {
            return self.top().to_decimal();
        }
        let mut out = self.top().to_decimal();
        out.push('/');
        out.push_str(&self.bottom().to_decimal());
        out
    }

    /// Reads canonical rational text, refusing every other spelling of the
    /// same value.
    ///
    /// # Errors
    ///
    /// Returns [`NumberError`] naming the rule the text broke.
    pub fn from_canonical(text: &str) -> Result<Self, NumberError> {
        let mut slash = None;
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'/' {
                if slash.is_some() {
                    return Err(NumberError::NotCanonical);
                }
                slash = Some(index);
            }
        }

        let Some(at) = slash else {
            let whole = Int::from_decimal(text).map_err(|_| NumberError::NotCanonical)?;
            return Ok(Self::whole(whole));
        };

        let (left, right) = (&text[..at], &text[at + 1..]);
        let top = Int::from_decimal(left).map_err(|_| NumberError::NotCanonical)?;
        if right.starts_with('-') {
            return Err(NumberError::SignedDenominator);
        }
        if right.len() > 1 && right.starts_with('0') {
            return Err(NumberError::NotCanonical);
        }
        let bottom = Nat::from_decimal(right).map_err(|_| NumberError::NotCanonical)?;
        if bottom.is_zero() {
            return Err(NumberError::ZeroDenominator);
        }
        if bottom.is_one() {
            return Err(NumberError::DenominatorOfOne);
        }
        if top.is_zero() {
            return Err(NumberError::ZeroAsFraction);
        }
        if !top.size().gcd(&bottom).is_one() {
            return Err(NumberError::NotReduced);
        }
        Self::ratio(&top, &Int::positive(bottom)).ok_or(NumberError::ZeroDenominator)
    }

    /// Reads a decimal token from an instance file, applying the token length
    /// cap and then the projected width cap, in that order, before building
    /// anything.
    ///
    /// # Errors
    ///
    /// Returns [`NumberError`] naming the fence that stopped the token.
    pub fn from_decimal_token(
        text: &str,
        token_bytes: usize,
        width_bits: u64,
    ) -> Result<Self, NumberError> {
        if text.len() > token_bytes {
            return Err(NumberError::TokenTooLong);
        }
        let scanned = Scan::run(text)?;
        if scanned.projected_bits() > width_bits {
            return Err(NumberError::TooWide);
        }
        Ok(scanned.value())
    }

    /// Reads a decimal token with no caps, for vectors and internal callers.
    ///
    /// # Errors
    ///
    /// Returns [`NumberError`] when the token is not a decimal number.
    pub fn from_decimal_uncapped(text: &str) -> Result<Self, NumberError> {
        Ok(Scan::run(text)?.value())
    }
}

/// A decimal token walked into its parts.
struct Scan<'a> {
    minus: bool,
    whole: &'a str,
    fraction: &'a str,
    exponent: i64,
}

/// Where the scanner is in the token.
enum State {
    Whole,
    Fraction,
    Exponent,
}

impl<'a> Scan<'a> {
    /// Walks a token byte by byte, accepting only the shapes named in the
    /// format rules.
    fn run(text: &'a str) -> Result<Self, NumberError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(NumberError::Malformed);
        }

        let mut at = 0usize;
        let minus = bytes[0] == b'-';
        if minus || bytes[0] == b'+' {
            if bytes[0] == b'+' {
                return Err(NumberError::Malformed);
            }
            at = 1;
        }

        let whole_start = at;
        let mut whole_end = at;
        let mut fraction_start = at;
        let mut fraction_end = at;
        let mut exponent_start = None;
        let mut state = State::Whole;

        while at < bytes.len() {
            let byte = bytes[at];
            match state {
                State::Whole => {
                    if byte.is_ascii_digit() {
                        at += 1;
                        whole_end = at;
                    } else if byte == b'.' {
                        at += 1;
                        fraction_start = at;
                        fraction_end = at;
                        state = State::Fraction;
                    } else if byte == b'e' || byte == b'E' {
                        at += 1;
                        exponent_start = Some(at);
                        state = State::Exponent;
                    } else {
                        return Err(NumberError::Malformed);
                    }
                }
                State::Fraction => {
                    if byte.is_ascii_digit() {
                        at += 1;
                        fraction_end = at;
                    } else if byte == b'e' || byte == b'E' {
                        at += 1;
                        exponent_start = Some(at);
                        state = State::Exponent;
                    } else {
                        return Err(NumberError::Malformed);
                    }
                }
                State::Exponent => {
                    // The exponent's own sign and digits are checked below, in
                    // one place, so the scanner only has to find its extent.
                    at += 1;
                }
            }
        }

        let whole = &text[whole_start..whole_end];
        let fraction = &text[fraction_start..fraction_end];
        if whole.is_empty() && fraction.is_empty() {
            return Err(NumberError::Malformed);
        }

        let exponent = match exponent_start {
            None => 0,
            Some(start) => read_exponent(&text[start..])?,
        };

        Ok(Self {
            minus,
            whole,
            fraction,
            exponent,
        })
    }

    /// An overestimate of the value's width in bits, from the characters only.
    ///
    /// Every decimal digit is below four bits' worth, and each unit of
    /// exponent shifts by a factor of ten, which is also below four bits. The
    /// bound is loose and it is sound, which is the requirement.
    fn projected_bits(&self) -> u64 {
        let digits = (self.whole.len() + self.fraction.len()) as u64;
        let net = self.exponent - self.fraction.len() as i64;
        let shift = net.unsigned_abs();
        digits
            .saturating_mul(4)
            .saturating_add(shift.saturating_mul(4))
            .saturating_add(1)
    }

    /// Evaluates the token exactly.
    fn value(&self) -> Frac {
        let mut digits = String::with_capacity(self.whole.len() + self.fraction.len());
        digits.push_str(self.whole);
        digits.push_str(self.fraction);
        let mantissa = Nat::from_decimal(&digits).unwrap_or_else(|_| Nat::zero());
        let net = self.exponent - self.fraction.len() as i64;

        let (top, bottom) = if net >= 0 {
            (
                mantissa.times(&Nat::power_of_ten(net.unsigned_abs() as u32)),
                Nat::one(),
            )
        } else {
            (mantissa, Nat::power_of_ten(net.unsigned_abs() as u32))
        };
        Frac::ratio(&Int::signed(self.minus, top), &Int::positive(bottom))
            .expect("a power of ten is never zero")
    }
}

/// Reads the exponent that follows an `e` or `E`.
fn read_exponent(text: &str) -> Result<i64, NumberError> {
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(NumberError::Malformed);
    }
    // An exponent too large for a machine word is beyond every cap by an
    // enormous margin, and saying so here keeps the projection in machine
    // arithmetic where it belongs.
    let magnitude: i64 = digits.parse().map_err(|_| NumberError::ExponentTooLarge)?;
    Ok(if negative { -magnitude } else { magnitude })
}

/// Why a number's text was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberError {
    /// The token is not a decimal number.
    Malformed,
    /// The text is not the canonical spelling of a rational.
    NotCanonical,
    /// The denominator carried a sign.
    SignedDenominator,
    /// The denominator was zero.
    ZeroDenominator,
    /// The denominator was one, which canonical text omits.
    DenominatorOfOne,
    /// Zero was written as a fraction.
    ZeroAsFraction,
    /// The two halves shared a factor.
    NotReduced,
    /// The token was longer than the accepted length.
    TokenTooLong,
    /// The exponent did not fit in a machine word.
    ExponentTooLarge,
    /// The value the token names is wider than the cap.
    TooWide,
}

impl fmt::Display for NumberError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Malformed => "numeric token is malformed",
            Self::NotCanonical => "rational text is not in canonical form",
            Self::SignedDenominator => "rational denominator carries a sign",
            Self::ZeroDenominator => "rational denominator is zero",
            Self::DenominatorOfOne => "canonical rational text omits a denominator of one",
            Self::ZeroAsFraction => "canonical zero is written as a bare digit",
            Self::NotReduced => "rational text is not reduced to lowest terms",
            Self::TokenTooLong => "numeric token exceeds the accepted length",
            Self::ExponentTooLarge => "numeric exponent is out of range",
            Self::TooWide => "numeric token projects a value wider than the cap",
        };
        formatter.write_str(text)
    }
}

impl core::error::Error for NumberError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(text: &str) -> Frac {
        Frac::from_decimal_uncapped(text).expect("a decimal token")
    }

    #[test]
    fn a_tenth_is_read_from_its_characters() {
        assert_eq!(token("0.1").to_canonical(), "1/10");
        assert_eq!(token("12.375").to_canonical(), "99/8");
        assert_eq!(
            token("0.3333333333333333").to_canonical(),
            "3333333333333333/10000000000000000"
        );
    }

    #[test]
    fn exponents_shift_in_both_directions() {
        assert_eq!(token("1.5e2").to_canonical(), "150");
        assert_eq!(token("2.5E+3").to_canonical(), "2500");
        assert_eq!(token("15e1").to_canonical(), "150");
        assert_eq!(token("1000E-1").to_canonical(), "100");
    }

    #[test]
    fn insignificant_characters_never_reach_the_value() {
        assert_eq!(token("007.50").to_canonical(), "15/2");
        assert_eq!(token(".5").to_canonical(), "1/2");
        assert_eq!(token("5.").to_canonical(), "5");
        assert_eq!(token("-0.000").to_canonical(), "0");
        assert!(!token("-0.000").is_minus());
    }

    #[test]
    fn the_scanner_refuses_every_shape_outside_the_grammar() {
        for text in [
            "", "+1", "1.2.3", ".", "-", "1e", "1e+", "nan", "inf", "1e2x", "1 2", "--1", "1..2",
        ] {
            assert!(
                Frac::from_decimal_uncapped(text).is_err(),
                "accepted {text:?}"
            );
        }
        assert_eq!(
            Frac::from_decimal_uncapped("1e99999999999999999999"),
            Err(NumberError::ExponentTooLarge)
        );
    }

    #[test]
    fn the_caps_apply_in_the_order_the_fence_names() {
        let long = format!("0.{}", "1".repeat(5000));
        assert_eq!(
            Frac::from_decimal_token(&long, 4096, 1_048_576),
            Err(NumberError::TokenTooLong)
        );
        assert_eq!(
            Frac::from_decimal_token("1.0e99999999", 4096, 1_048_576),
            Err(NumberError::TooWide)
        );
        assert_eq!(
            Frac::from_decimal_token("1.5e2", 4096, 1_048_576)
                .expect("inside")
                .to_canonical(),
            "150"
        );
    }

    #[test]
    fn canonical_text_round_trips() {
        for text in ["0", "1", "-1", "3/2", "-3/2", "1/10", "150", "-1/80"] {
            assert_eq!(
                Frac::from_canonical(text)
                    .expect("canonical")
                    .to_canonical(),
                text
            );
        }
    }

    #[test]
    fn every_near_miss_of_canonical_text_is_named() {
        for (text, expected) in [
            ("3/1", NumberError::DenominatorOfOne),
            ("6/4", NumberError::NotReduced),
            ("0/5", NumberError::ZeroAsFraction),
            ("1/-2", NumberError::SignedDenominator),
            ("1/0", NumberError::ZeroDenominator),
            ("+3", NumberError::NotCanonical),
            ("1.5", NumberError::NotCanonical),
            ("1e2", NumberError::NotCanonical),
            ("03/2", NumberError::NotCanonical),
            ("1/02", NumberError::NotCanonical),
            ("-0", NumberError::NotCanonical),
            ("1/2/3", NumberError::NotCanonical),
            (" 3", NumberError::NotCanonical),
        ] {
            assert_eq!(Frac::from_canonical(text), Err(expected), "for {text:?}");
        }
    }

    #[test]
    fn the_two_grammars_agree_where_both_accept() {
        for text in ["0", "1", "-1", "150", "2500"] {
            assert_eq!(
                Frac::from_canonical(text).expect("canonical"),
                Frac::from_decimal_uncapped(text).expect("decimal")
            );
        }
    }
}
