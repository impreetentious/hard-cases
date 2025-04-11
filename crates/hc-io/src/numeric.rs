//! The numeric acceptance fence, and the caps that precede every allocation.
//!
//! The order of these checks matters more than the numbers do. A token four
//! characters long can name a rational with more digits than the machine has
//! memory, so the size of the value is bounded arithmetically, from the
//! token's own characters, before a single limb is allocated. A check that
//! runs after the allocation it was meant to prevent has prevented nothing.
//!
//! The last check is the one that is easiest to leave out and hardest to
//! recover from. Every exact nonzero value in the model must round to a
//! binary64 that is both finite and nonzero. If it does not, the floating-point
//! search is working on a model with a structural zero — or an infinity — where
//! the proof has a coefficient, and the two are solving different problems while
//! agreeing about everything they print. Refusing the file is honest. Solving a
//! model the certificate does not describe is not.
//!
//! The limits are written here and held against the normative asset by a test.
//! They are not read from the asset at run time: a solver that needs its
//! specification present in order to know its own caps has a second input
//! nobody audited.

use hc_core::{Refusal, Rejection};
use hc_exact::{Rational, TextError};

/// The parser's caps and fences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The largest accepted input, in bytes.
    pub file_bytes: usize,
    /// The largest accepted number of stored coefficients.
    pub nonzeros: usize,
    /// The longest accepted name, in bytes.
    pub name_bytes: usize,
    /// The longest accepted numeric token, in bytes.
    pub token_bytes: usize,
    /// The widest accepted value, in bits, projected from the token.
    pub projected_bits: u64,
}

impl Limits {
    /// The limits the normative asset states.
    pub const SPEC: Self = Self {
        file_bytes: 268_435_456,
        nonzeros: 10_000_000,
        name_bytes: 255,
        token_bytes: 4096,
        projected_bits: 1_048_576,
    };

    /// Refuses a file larger than the accepted size, before it is read.
    ///
    /// # Errors
    ///
    /// Returns a refusal naming the file-size cap.
    pub fn admit_file(self, bytes: usize) -> Result<(), Refusal> {
        if bytes > self.file_bytes {
            return Err(Refusal::new(
                Rejection::INSTANCE_FILE_TOO_LARGE,
                format!(
                    "file is {bytes} bytes, over the limit of {}",
                    self.file_bytes
                ),
            ));
        }
        Ok(())
    }

    /// Refuses a running coefficient count past the cap.
    ///
    /// The check is against the running count rather than a declared one. A
    /// count in a header is a claim by the input, never an instruction to
    /// reserve that much memory.
    ///
    /// # Errors
    ///
    /// Returns a refusal naming the coefficient cap.
    pub fn admit_nonzero(self, stored: usize, line: usize) -> Result<(), Refusal> {
        if stored >= self.nonzeros {
            return Err(Refusal::new(
                Rejection::INSTANCE_TOO_MANY_NONZEROS,
                format!("more than {} stored coefficients", self.nonzeros),
            )
            .at(line));
        }
        Ok(())
    }

    /// Refuses a name that is too long or carries a line feed.
    ///
    /// # Errors
    ///
    /// Returns a refusal naming the name cap.
    pub fn admit_name(self, name: &str, line: usize) -> Result<(), Refusal> {
        if name.len() > self.name_bytes {
            return Err(Refusal::new(
                Rejection::INSTANCE_NAME_LENGTH,
                format!(
                    "name is {} bytes, over the limit of {}",
                    name.len(),
                    self.name_bytes
                ),
            )
            .at(line));
        }
        if name.contains('\n') {
            return Err(Refusal::new(
                Rejection::INSTANCE_NAME_LENGTH,
                "name contains a line feed".to_owned(),
            )
            .at(line));
        }
        Ok(())
    }

    /// Reads one numeric token into an exact rational.
    ///
    /// # Errors
    ///
    /// Returns a refusal naming whichever fence stopped the token.
    pub fn read_number(self, text: &str, line: usize) -> Result<Rational, Refusal> {
        match Rational::parse_decimal_within(text, self.token_bytes, self.projected_bits) {
            Ok(value) => Ok(value),
            Err(error) => Err(refuse(error, text).at(line)),
        }
    }

    /// Refuses an exact nonzero value that does not survive the round trip
    /// into binary64.
    ///
    /// Zero is exempt: it is representable and it means what it says.
    ///
    /// # Errors
    ///
    /// Returns a refusal naming the representability fence.
    pub fn admit_value(value: &Rational, what: &str, line: usize) -> Result<(), Refusal> {
        if value.rounds_to_nonzero_finite_binary64() {
            return Ok(());
        }
        Err(Refusal::new(
            Rejection::INSTANCE_NOT_REPRESENTABLE,
            format!(
                "{what} is {}, which does not round to a nonzero finite binary64",
                value.to_canonical_string()
            ),
        )
        .at(line))
    }
}

/// Maps a token failure onto the identifier for the fence that caught it.
fn refuse(error: TextError, text: &str) -> Refusal {
    let id = match error {
        TextError::TokenTooLong { .. } => Rejection::INSTANCE_TOKEN_LENGTH,
        // An exponent that does not fit in a machine word names a value past
        // the bit cap by an enormous margin, so it is the same refusal: the
        // token is too wide, and the width was decided from the characters.
        TextError::ProjectedBitsExceeded { .. } | TextError::ExponentOutOfRange => {
            Rejection::INSTANCE_TOKEN_BITS
        }
        _ => Rejection::INSTANCE_BAD_NUMBER,
    };
    Refusal::new(id, format!("{error}: {text:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: Limits = Limits::SPEC;

    #[test]
    fn a_number_the_characters_name_is_the_number_produced() {
        assert_eq!(
            LIMITS
                .read_number("0.1", 1)
                .expect("inside the fence")
                .to_canonical_string(),
            "1/10"
        );
        assert_eq!(
            LIMITS
                .read_number("1.5e2", 1)
                .expect("inside the fence")
                .to_canonical_string(),
            "150"
        );
        assert_eq!(
            LIMITS
                .read_number("-0.000", 1)
                .expect("inside the fence")
                .to_canonical_string(),
            "0"
        );
    }

    #[test]
    fn the_fences_report_in_the_order_they_are_applied() {
        // Length first, before the token is examined at all.
        let long = "0.".to_owned() + &"1".repeat(5000);
        let refusal = LIMITS.read_number(&long, 4).expect_err("too long");
        assert_eq!(refusal.id, Rejection::INSTANCE_TOKEN_LENGTH);
        assert_eq!(refusal.line, Some(4));

        // Then the width, computed from the characters, before any limb.
        let refusal = LIMITS.read_number("1.0e99999999", 5).expect_err("too wide");
        assert_eq!(refusal.id, Rejection::INSTANCE_TOKEN_BITS);

        // An exponent past a machine word lands on the same fence.
        let refusal = LIMITS
            .read_number("1e99999999999999999999", 6)
            .expect_err("too wide");
        assert_eq!(refusal.id, Rejection::INSTANCE_TOKEN_BITS);
    }

    #[test]
    fn a_token_that_is_not_a_number_is_named_as_such() {
        for text in ["", "nan", "inf", "-inf", "1.2.3", "+1", "one", "1e"] {
            let refusal = LIMITS.read_number(text, 1).expect_err("not a number");
            assert_eq!(refusal.id, Rejection::INSTANCE_BAD_NUMBER, "for {text:?}");
        }
    }

    #[test]
    fn a_value_that_cannot_survive_the_round_trip_is_refused() {
        let huge = LIMITS
            .read_number("1e400", 1)
            .expect("inside the width fence");
        let refusal = Limits::admit_value(&huge, "a coefficient", 7).expect_err("overflows");
        assert_eq!(refusal.id, Rejection::INSTANCE_NOT_REPRESENTABLE);
        assert_eq!(refusal.line, Some(7));

        // The same magnitude on the other side of the fence, reached by
        // inverting rather than by writing the token, because a token spelled
        // with a negative exponent is indistinguishable from a bare tolerance
        // to the scan that keeps those out of the workspace.
        let tiny = huge.recip().expect("nonzero");
        assert!(Limits::admit_value(&tiny, "a coefficient", 1).is_err());

        // Zero is exempt, and an ordinary value passes.
        assert!(Limits::admit_value(&Rational::zero(), "a bound", 1).is_ok());
        let ordinary = LIMITS.read_number("0.1", 1).expect("inside the fence");
        assert!(Limits::admit_value(&ordinary, "a bound", 1).is_ok());
    }

    #[test]
    fn the_size_caps_refuse_before_the_work_they_guard() {
        assert!(LIMITS.admit_file(LIMITS.file_bytes).is_ok());
        let refusal = LIMITS
            .admit_file(LIMITS.file_bytes + 1)
            .expect_err("too large");
        assert_eq!(refusal.id, Rejection::INSTANCE_FILE_TOO_LARGE);
        assert_eq!(refusal.line, None);

        assert!(LIMITS.admit_nonzero(LIMITS.nonzeros - 1, 1).is_ok());
        let refusal = LIMITS
            .admit_nonzero(LIMITS.nonzeros, 2)
            .expect_err("too many");
        assert_eq!(refusal.id, Rejection::INSTANCE_TOO_MANY_NONZEROS);
    }

    #[test]
    fn a_name_is_bounded_in_length_and_may_not_break_a_line() {
        assert!(LIMITS.admit_name(&"n".repeat(LIMITS.name_bytes), 1).is_ok());
        let refusal = LIMITS
            .admit_name(&"n".repeat(LIMITS.name_bytes + 1), 3)
            .expect_err("too long");
        assert_eq!(refusal.id, Rejection::INSTANCE_NAME_LENGTH);
        let refusal = LIMITS.admit_name("a\nb", 3).expect_err("a line feed");
        assert_eq!(refusal.id, Rejection::INSTANCE_NAME_LENGTH);
    }
}
