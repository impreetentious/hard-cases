//! The verifier's rejection identifiers.
//!
//! A second hand-written copy of the same public list. Not shared with the
//! solver's copy and not generated from the specification: the point of two
//! lists is that they can disagree, and a disagreement between them is a real
//! finding rather than a formality. A generated list agrees with the
//! specification by construction, which is not agreement.
//!
//! When this side says no, it says which rule said no. That is what turns a
//! disagreement between the two implementations into a report with an address,
//! and it is why the identifiers are an interface rather than an internal
//! detail.

use core::fmt;

/// One rejection identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Code(&'static str);

impl Code {
    /// The identifier as printed.
    #[must_use]
    pub const fn text(self) -> &'static str {
        self.0
    }

    /// The group the identifier belongs to.
    #[must_use]
    pub fn family(self) -> &'static str {
        match self.0.rfind('-') {
            Some(at) => &self.0[..at],
            None => self.0,
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

macro_rules! codes {
    ($( $(#[$note:meta])* $name:ident = $text:literal ),* $(,)?) => {
        impl Code {
            $( $(#[$note])* pub const $name: Self = Self($text); )*
        }

        /// Every identifier this crate names, in order.
        pub const KNOWN: &[Code] = &[ $(Code::$name),* ];
    };
}

/// The groups this crate names completely.
pub const FAMILIES: &[&str] = &["HC-INSTANCE"];

codes! {
    /// The file is longer than the accepted size.
    FILE_TOO_LARGE = "HC-INSTANCE-01",
    /// More coefficients than the accepted count.
    TOO_MANY_NONZEROS = "HC-INSTANCE-02",
    /// A name is too long or holds a line feed.
    NAME_LENGTH = "HC-INSTANCE-03",
    /// A numeric token is longer than the accepted length.
    TOKEN_LENGTH = "HC-INSTANCE-04",
    /// A numeric token names a value wider than the cap.
    TOKEN_WIDTH = "HC-INSTANCE-05",
    /// An exact nonzero value does not survive the round trip into binary64.
    NOT_REPRESENTABLE = "HC-INSTANCE-06",
    /// A numeric token is malformed or spells a not-a-number or infinity.
    BAD_NUMBER = "HC-INSTANCE-07",
    /// A section header outside the accepted list.
    UNKNOWN_SECTION = "HC-INSTANCE-08",
    /// A section out of order or repeated.
    SECTION_ORDER = "HC-INSTANCE-09",
    /// A required section is missing.
    SECTION_MISSING = "HC-INSTANCE-10",
    /// No free row was declared to serve as the objective.
    NO_OBJECTIVE = "HC-INSTANCE-11",
    /// A record names a row that was never declared.
    UNKNOWN_ROW = "HC-INSTANCE-12",
    /// A record names a column that was never declared.
    UNKNOWN_COLUMN = "HC-INSTANCE-13",
    /// Two rows share a name.
    DUPLICATE_ROW = "HC-INSTANCE-14",
    /// Two columns share a name.
    DUPLICATE_COLUMN = "HC-INSTANCE-15",
    /// A section carries more than one named vector.
    MULTIPLE_VECTORS = "HC-INSTANCE-16",
    /// A bound type outside the accepted list.
    UNKNOWN_BOUND = "HC-INSTANCE-17",
    /// An integer bound record carries a value that is not an exact integer.
    FRACTIONAL_INTEGER_BOUND = "HC-INSTANCE-18",
    /// A negative upper bound on a column still at the default lower bound.
    AMBIGUOUS_NEGATIVE_UPPER = "HC-INSTANCE-19",
    /// Content runs past a fixed-form column window.
    FIELD_OVERRUN = "HC-INSTANCE-20",
    /// Integrality markers are unbalanced or nested.
    MARKER_BALANCE = "HC-INSTANCE-21",
    /// A column's records are not contiguous.
    COLUMN_SPLIT = "HC-INSTANCE-22",
    /// A range record names the objective row or a free row.
    RANGE_ON_FREE_ROW = "HC-INSTANCE-23",
    /// A second bound of the same type on one column.
    DUPLICATE_BOUND = "HC-INSTANCE-24",
    /// A free-form record carries more fields than its type accepts.
    TOO_MANY_FIELDS = "HC-INSTANCE-25",
    /// Input ended before the terminating section.
    UNEXPECTED_END = "HC-INSTANCE-26",
    /// The input does not match the accepted LP-format grammar.
    LP_GRAMMAR = "HC-INSTANCE-27",
    /// The input does not match the accepted JSON instance schema.
    JSON_SCHEMA = "HC-INSTANCE-28",
    /// The input is not ASCII text with line-feed line endings.
    NOT_TEXT = "HC-INSTANCE-29",
}

/// A refusal: the rule, what it was reading, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejected {
    /// The rule that refused.
    pub code: Code,
    /// What was being read.
    pub detail: String,
    /// The one-based line, when there is one.
    pub line: Option<usize>,
}

impl Rejected {
    /// A refusal with no position.
    #[must_use]
    pub fn new(code: Code, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            line: None,
        }
    }

    /// The same refusal, positioned.
    #[must_use]
    pub fn on_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
}

impl fmt::Display for Rejected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(formatter, "{} at line {}: {}", self.code, line, self.detail),
            None => write!(formatter, "{}: {}", self.code, self.detail),
        }
    }
}

impl core::error::Error for Rejected {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_prints_as_itself_and_knows_its_family() {
        assert_eq!(Code::NO_OBJECTIVE.text(), "HC-INSTANCE-11");
        assert_eq!(Code::NO_OBJECTIVE.to_string(), "HC-INSTANCE-11");
        assert_eq!(Code::NO_OBJECTIVE.family(), "HC-INSTANCE");
        assert!(FAMILIES.contains(&Code::NO_OBJECTIVE.family()));
    }

    #[test]
    fn the_list_has_no_repeats_and_is_ordered() {
        let ordered: Vec<&str> = KNOWN.iter().map(|code| code.text()).collect();
        let mut sorted = ordered.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            ordered.len(),
            "the list repeats an identifier"
        );
        assert_eq!(ordered, sorted, "the list is not in identifier order");
    }

    #[test]
    fn a_refusal_carries_its_rule_and_position() {
        let refusal = Rejected::new(Code::UNKNOWN_SECTION, "QUADOBJ").on_line(12);
        assert_eq!(refusal.to_string(), "HC-INSTANCE-08 at line 12: QUADOBJ");
        let plain = Rejected::new(Code::UNEXPECTED_END, "no ENDATA");
        assert_eq!(plain.to_string(), "HC-INSTANCE-26: no ENDATA");
    }
}
