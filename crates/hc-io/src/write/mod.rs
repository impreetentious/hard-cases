//! Writing a canonical model back out.
//!
//! A writer is held to a harder standard than a reader here. Reading is
//! allowed to refuse; writing is allowed to refuse too, but what it produces
//! must read back as the same model — not an equivalent one, the same one,
//! down to the canonical dump below the line that identifies the file.
//!
//! Two consequences are worth naming, because both look like over-engineering
//! until the round trip fails without them.
//!
//! A cancellation is written out as the pair that cancelled. The canonical
//! dump distinguishes a coefficient that is absent from a pair of coefficients
//! that summed to zero, because those are the same matrix and different files,
//! and a writer that emitted only the surviving coefficients would produce a
//! file that is not the model it was handed.
//!
//! And a bound that is defaulted is written anyway. Every format defaults an
//! omitted lower bound to zero, and every format has a fork about what a
//! negative upper bound means beside that default. Writing both sides means
//! the reader never has to reach the fork.

pub mod json;
pub mod lp;
pub mod mps;

use core::fmt;

/// Why a model cannot be written in a format.
///
/// A writer refuses rather than producing something close. The formats are not
/// equally expressive, and a file that names a different model than the one it
/// was given is worse than no file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unwritable {
    /// What could not be written.
    pub detail: String,
}

impl Unwritable {
    /// A refusal with a reason.
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Unwritable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl core::error::Error for Unwritable {}

/// The coefficients of one row, with cancellations restored as pairs.
///
/// Shared by all three writers because it is the same fact in three
/// spellings: the entries the matrix stores, plus a pair for every position
/// the dump records as a cancellation, in variable order.
pub(crate) fn row_entries(
    model: &hc_core::Model,
    row: hc_core::RowId,
) -> Vec<(hc_core::VarId, Option<&hc_exact::Rational>)> {
    let mut out = Vec::new();
    for (variable, at, value) in model.matrix().entries() {
        if at == row {
            out.push((variable, Some(value)));
        }
    }
    for (variable, at) in model.zero_sums() {
        if *at == row {
            // The value is absent, which is how a caller is told to write the
            // cancelling pair rather than a coefficient.
            out.push((*variable, None));
        }
    }
    out.sort_by_key(|(variable, _)| *variable);
    out
}

/// The longest decimal expansion a writer will produce.
///
/// The same number as the token cap the readers apply, because a token longer
/// than that is refused on the way back in and a writer that produced one
/// would be writing a file nothing can read.
const MAX_DIGITS: usize = 4096;

/// An exact value as the decimal text every accepted format is written in.
///
/// Not every rational has one. A value whose denominator carries a prime other
/// than two or five has no finite decimal expansion, and there is no rounding
/// available here: a written coefficient that is nearly the coefficient is the
/// one thing a file about a proof may not contain. Such a model is refused.
///
/// The expansion is found by scaling rather than by factoring, which is much
/// shorter to read. It is bounded rather than open-ended: a denominator of the
/// form two to the a times five to the b needs at most as many digits as it has
/// bits, so a value that has not become whole by then never will, and the loop
/// ends after a couple of turns on the values a file actually carries.
pub(crate) fn decimal(value: &hc_exact::Rational) -> Result<String, Unwritable> {
    if value.is_integer() {
        return Ok(value.to_canonical_string());
    }
    let reachable = usize::try_from(value.denominator().bit_len()).unwrap_or(MAX_DIGITS);
    let ten = hc_exact::Rational::from_i64(10);
    let mut scaled = value.clone();
    for digits in 1..=reachable.min(MAX_DIGITS) {
        scaled = scaled.mul(&ten);
        if !scaled.is_integer() {
            continue;
        }
        let whole = scaled.to_canonical_string();
        let negative = whole.starts_with('-');
        let bare = whole.trim_start_matches('-');
        let padded = if bare.len() <= digits {
            format!("{}{bare}", "0".repeat(digits - bare.len() + 1))
        } else {
            bare.to_owned()
        };
        let point = padded.len() - digits;
        return Ok(format!(
            "{}{}.{}",
            if negative { "-" } else { "" },
            &padded[..point],
            &padded[point..]
        ));
    }
    Err(Unwritable::new(format!(
        "the value {} has no finite decimal expansion",
        value.to_canonical_string()
    )))
}

#[cfg(test)]
mod tests {
    use hc_exact::Rational;

    use super::decimal;

    fn of(top: i64, bottom: i64) -> Rational {
        Rational::from_i64(top)
            .checked_div(&Rational::from_i64(bottom))
            .expect("a nonzero denominator")
    }

    #[test]
    fn a_terminating_value_is_written_exactly() {
        assert_eq!(decimal(&of(1, 10)).expect("terminates"), "0.1");
        assert_eq!(decimal(&of(5, 2)).expect("terminates"), "2.5");
        assert_eq!(decimal(&of(-5, 2)).expect("terminates"), "-2.5");
        assert_eq!(decimal(&of(1, 4)).expect("terminates"), "0.25");
        assert_eq!(decimal(&of(3, 8)).expect("terminates"), "0.375");
        assert_eq!(decimal(&of(-1, 1000)).expect("terminates"), "-0.001");
        assert_eq!(decimal(&Rational::from_i64(150)).expect("whole"), "150");
        assert_eq!(decimal(&Rational::zero()).expect("whole"), "0");
    }

    #[test]
    fn a_value_that_does_not_terminate_is_refused_rather_than_rounded() {
        // A written coefficient that is nearly the coefficient is the one
        // thing a file a proof is about may not contain.
        let refusal = decimal(&of(1, 3)).expect_err("one third does not terminate");
        assert!(
            refusal.to_string().contains("no finite decimal expansion"),
            "{refusal}"
        );
        assert!(decimal(&of(2, 7)).is_err());
    }

    #[test]
    fn what_is_written_reads_back_as_what_was_written() {
        for (top, bottom) in [(1, 10), (5, 2), (-5, 2), (1, 4), (3, 8), (7, 1), (-1, 1000)] {
            let value = of(top, bottom);
            let text = decimal(&value).expect("terminates");
            let again = Rational::parse_decimal_within(&text, 4096, 1_048_576).expect("a decimal");
            assert_eq!(again, value, "{text}");
        }
    }
}
