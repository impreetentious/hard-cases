//! Rejection identifiers.
//!
//! When something is refused, the refusal names the rule that refused it. A
//! parser that returns a boolean is a parser nobody can argue with, and the
//! ability to argue with it is the point: an identifier is what turns a
//! disagreement between two implementations into a report with an address.
//!
//! These identifiers are a public interface. They are printed, displayed in
//! the interface, cited by case files, and paired with each entry of the
//! tamper corpus. They are append-only: an identifier is never reused for a
//! different check, and a retired one is marked retired rather than removed.
//!
//! The list here is written by hand and checked against the normative asset by
//! a test. It is deliberately not generated from the asset: a specification
//! that emits code makes the code agree with it by construction, which is not
//! agreement, it is a copy. Two hand-written lists that must match is a real
//! constraint; one generated list is a formality.

use core::fmt;

/// One rejection identifier.
///
/// Constructed only through the named constants below, so an identifier that
/// appears in output is always one that exists in the registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rejection(&'static str);

impl Rejection {
    /// The identifier as it is printed.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// The group the identifier belongs to, which is its text up to the final
    /// separator.
    #[must_use]
    pub fn group(self) -> &'static str {
        match self.0.rfind('-') {
            Some(at) => &self.0[..at],
            None => self.0,
        }
    }
}

impl fmt::Display for Rejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

macro_rules! rejections {
    ($( $(#[$note:meta])* $name:ident => $id:literal ),* $(,)?) => {
        impl Rejection {
            $( $(#[$note])* pub const $name: Self = Self($id); )*
        }

        /// Every identifier this crate names, in registry order.
        ///
        /// A test holds this against the normative asset in both directions,
        /// so an identifier here that the asset does not declare, and an
        /// identifier in a covered group that is missing here, both fail the
        /// build.
        pub const REGISTERED: &[Rejection] = &[ $(Rejection::$name),* ];
    };
}

/// The groups this crate is responsible for naming completely.
///
/// The certificate groups belong to the crates that read certificates and are
/// added when those exist. Listing the covered groups explicitly is what lets
/// the completeness check be exact rather than approximate.
pub const COVERED_GROUPS: &[&str] = &["HC-INSTANCE", "HCC-PARSE"];

rejections! {
    /// The file is longer than the accepted size.
    INSTANCE_FILE_TOO_LARGE => "HC-INSTANCE-01",
    /// The file declares more coefficients than the accepted count.
    INSTANCE_TOO_MANY_NONZEROS => "HC-INSTANCE-02",
    /// A name is too long or contains a line feed.
    INSTANCE_NAME_LENGTH => "HC-INSTANCE-03",
    /// A numeric token is longer than the accepted length.
    INSTANCE_TOKEN_LENGTH => "HC-INSTANCE-04",
    /// A numeric token projects a value wider than the bit cap.
    INSTANCE_TOKEN_BITS => "HC-INSTANCE-05",
    /// An exact nonzero value does not round to a nonzero finite binary64.
    INSTANCE_NOT_REPRESENTABLE => "HC-INSTANCE-06",
    /// A numeric token is malformed or spells a not-a-number or infinity.
    INSTANCE_BAD_NUMBER => "HC-INSTANCE-07",
    /// The file carries a section header outside the accepted list.
    INSTANCE_UNKNOWN_SECTION => "HC-INSTANCE-08",
    /// A section appears out of order or more than once.
    INSTANCE_SECTION_ORDER => "HC-INSTANCE-09",
    /// A required section is missing.
    INSTANCE_SECTION_MISSING => "HC-INSTANCE-10",
    /// No objective row was declared.
    INSTANCE_NO_OBJECTIVE => "HC-INSTANCE-11",
    /// A record references a row the file never declared.
    INSTANCE_UNKNOWN_ROW => "HC-INSTANCE-12",
    /// A record references a column the file never declared.
    INSTANCE_UNKNOWN_COLUMN => "HC-INSTANCE-13",
    /// Two rows share a name.
    INSTANCE_DUPLICATE_ROW => "HC-INSTANCE-14",
    /// Two columns share a name.
    INSTANCE_DUPLICATE_COLUMN => "HC-INSTANCE-15",
    /// A section carries more than one named vector.
    INSTANCE_MULTIPLE_VECTORS => "HC-INSTANCE-16",
    /// A bound type outside the accepted list.
    INSTANCE_UNKNOWN_BOUND => "HC-INSTANCE-17",
    /// An integer bound record carries a value that is not an exact integer.
    INSTANCE_FRACTIONAL_INTEGER_BOUND => "HC-INSTANCE-18",
    /// A negative upper bound on a column still at the default lower bound.
    INSTANCE_AMBIGUOUS_NEGATIVE_UPPER => "HC-INSTANCE-19",
    /// A field's content runs past its fixed-form column window.
    INSTANCE_FIELD_OVERRUN => "HC-INSTANCE-20",
    /// The integrality markers are unbalanced or nested.
    INSTANCE_MARKER_BALANCE => "HC-INSTANCE-21",
    /// A column's records are not contiguous.
    INSTANCE_COLUMN_SPLIT => "HC-INSTANCE-22",
    /// A range record names the objective row or a free row.
    INSTANCE_RANGE_ON_FREE_ROW => "HC-INSTANCE-23",
    /// A second bound of the same type on one column.
    INSTANCE_DUPLICATE_BOUND => "HC-INSTANCE-24",
    /// A free-form record carries more fields than its type accepts.
    INSTANCE_TOO_MANY_FIELDS => "HC-INSTANCE-25",
    /// Input ended before the terminating section.
    INSTANCE_UNEXPECTED_END => "HC-INSTANCE-26",
    /// The input does not match the accepted LP-format grammar.
    INSTANCE_LP_GRAMMAR => "HC-INSTANCE-27",
    /// The input does not match the accepted JSON instance schema.
    INSTANCE_JSON_SCHEMA => "HC-INSTANCE-28",
    /// The input is not ASCII text with line-feed line endings.
    INSTANCE_NOT_TEXT => "HC-INSTANCE-29",

    /// The stream does not begin with the certificate magic.
    HCC_MAGIC => "HCC-PARSE-01",
    /// The header names a version this reader does not implement.
    HCC_VERSION => "HCC-PARSE-02",
    /// The header's digest is not the digest of the instance supplied.
    HCC_NOT_THIS_INSTANCE => "HCC-PARSE-03",
    /// A declared size exceeds a cap, before anything is allocated.
    HCC_DECLARED_TOO_LARGE => "HCC-PARSE-04",
    /// A variable-length integer is encoded in more bytes than it needs.
    HCC_NON_MINIMAL_LENGTH => "HCC-PARSE-05",
    /// A variable-length integer does not fit in sixty-four bits.
    HCC_LENGTH_OVERFLOW => "HCC-PARSE-06",
    /// The stream ends inside a record.
    HCC_TRUNCATED => "HCC-PARSE-07",
    /// Bytes follow the terminating record.
    HCC_TRAILING_BYTES => "HCC-PARSE-08",
    /// The checksum does not match the bytes it covers.
    HCC_CHECKSUM => "HCC-PARSE-09",
    /// A record type outside the closed list.
    HCC_UNKNOWN_RECORD => "HCC-PARSE-10",
    /// A record's payload is not the length it declared.
    HCC_RECORD_LENGTH => "HCC-PARSE-11",
    /// Record identifiers are not strictly increasing.
    HCC_IDENTIFIER_ORDER => "HCC-PARSE-12",
    /// A rational is not written in canonical form.
    HCC_RATIONAL_FORM => "HCC-PARSE-13",
    /// A rational is wider than the bit cap.
    HCC_RATIONAL_BITS => "HCC-PARSE-14",
    /// A rational's declared token length exceeds the maximum.
    HCC_RATIONAL_TOKEN => "HCC-PARSE-15",
    /// A set-valued array is unsorted or repeats an element.
    HCC_SET_ORDER => "HCC-PARSE-16",
    /// The terminating record's counts disagree with the stream.
    HCC_END_COUNTS => "HCC-PARSE-17",
    /// The declared tree depth exceeds the cap.
    HCC_DEPTH => "HCC-PARSE-18",
    /// The running record count exceeds the cap.
    HCC_RECORD_COUNT => "HCC-PARSE-19",
    /// The running byte count exceeds the cap.
    HCC_BYTE_COUNT => "HCC-PARSE-20",
}

/// A refusal: the rule, what it was looking at, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The rule that refused.
    pub id: Rejection,
    /// What was being read when it did.
    pub detail: String,
    /// The one-based line of the input, when the refusal has one.
    pub line: Option<usize>,
}

impl Refusal {
    /// A refusal with no position.
    #[must_use]
    pub fn new(id: Rejection, detail: impl Into<String>) -> Self {
        Self {
            id,
            detail: detail.into(),
            line: None,
        }
    }

    /// The same refusal, positioned at a line.
    #[must_use]
    pub fn at(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(formatter, "{} at line {}: {}", self.id, line, self.detail),
            None => write!(formatter, "{}: {}", self.id, self.detail),
        }
    }
}

impl core::error::Error for Refusal {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_prints_as_itself_and_knows_its_group() {
        assert_eq!(Rejection::INSTANCE_NO_OBJECTIVE.as_str(), "HC-INSTANCE-11");
        assert_eq!(
            Rejection::INSTANCE_NO_OBJECTIVE.to_string(),
            "HC-INSTANCE-11"
        );
        assert_eq!(Rejection::INSTANCE_NO_OBJECTIVE.group(), "HC-INSTANCE");
    }

    #[test]
    fn the_registry_has_no_repeats_and_is_in_order() {
        let mut seen: Vec<&str> = REGISTERED.iter().map(|item| item.as_str()).collect();
        let count = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), count, "the registry repeats an identifier");
        let ordered: Vec<&str> = REGISTERED.iter().map(|item| item.as_str()).collect();
        assert_eq!(ordered, seen, "the registry is not in identifier order");
    }

    #[test]
    fn a_refusal_carries_its_rule_and_position() {
        let refusal = Refusal::new(Rejection::INSTANCE_UNKNOWN_SECTION, "QUADOBJ").at(12);
        assert_eq!(refusal.to_string(), "HC-INSTANCE-08 at line 12: QUADOBJ");
        let plain = Refusal::new(Rejection::INSTANCE_UNEXPECTED_END, "no ENDATA");
        assert_eq!(plain.to_string(), "HC-INSTANCE-26: no ENDATA");
        assert_eq!(plain.line, None);
    }
}
