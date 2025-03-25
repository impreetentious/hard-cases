//! Identifiers, senses, and the tags that say which rules produced a model.
//!
//! Every index here is a distinct type. A row index and a column index are
//! both small integers and are never the same thing, and a matrix routine that
//! confuses them produces a model that is wrong in a way no test of the
//! routine itself would show. The wrappers cost nothing at run time and remove
//! the whole family.

use core::fmt;

/// A variable's position in the canonical model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VarId(pub u32);

/// A retained row's position in the canonical model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowId(pub u32);

/// A canonical inequality's identifier.
///
/// Every axiom a proof may cite is one of these, variable bounds included.
/// There is no separate convention for a bound, which is what makes a
/// contradiction a complete witness with nothing left out by custom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IneqId(pub u32);

impl VarId {
    /// The index as a machine word.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl RowId {
    /// The index as a machine word.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl IneqId {
    /// The identifier as a machine word.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for VarId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl fmt::Display for RowId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl fmt::Display for IneqId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Which side of a two-sided constraint or bound an inequality comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    /// The lower side, contributing the row or bound unchanged.
    Lower,
    /// The upper side, contributing the negated row or bound.
    Upper,
}

impl Side {
    /// The word the canonical dump writes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lower => "lower",
            Self::Upper => "upper",
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The direction an objective is optimized in.
///
/// The canonical model always minimizes. The original sense is carried so that
/// reports and postsolved witnesses can be printed the way their author wrote
/// them, and because a sign error in the negation is otherwise invisible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sense {
    /// Minimize.
    Min,
    /// Maximize.
    Max,
}

impl Sense {
    /// The word the canonical dump writes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

impl fmt::Display for Sense {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Which format's rules produced a model.
///
/// The tag is prepended to the raw bytes before hashing, so one byte sequence
/// read under two sets of rules cannot produce the same instance digest. Fixed
/// and free MPS differ here precisely because a file can be legal under both
/// and mean different things under each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormatTag {
    /// MPS with positional column windows.
    MpsFixed,
    /// MPS with whitespace-delimited fields.
    MpsFree,
    /// The accepted subset of LP format.
    Lp,
    /// The JSON instance schema.
    Json,
}

impl FormatTag {
    /// The byte prepended to the raw bytes before hashing.
    #[must_use]
    pub const fn byte(self) -> u8 {
        match self {
            Self::MpsFixed => 1,
            Self::MpsFree => 2,
            Self::Lp => 3,
            Self::Json => 4,
        }
    }

    /// The tag a byte names, when it names one.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::MpsFixed),
            2 => Some(Self::MpsFree),
            3 => Some(Self::Lp),
            4 => Some(Self::Json),
            _ => None,
        }
    }

    /// The human name for reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MpsFixed => "MPS, fixed columns",
            Self::MpsFree => "MPS, free form",
            Self::Lp => "LP format subset",
            Self::Json => "JSON instance schema",
        }
    }
}

impl fmt::Display for FormatTag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tag_round_trips_through_its_byte() {
        for tag in [
            FormatTag::MpsFixed,
            FormatTag::MpsFree,
            FormatTag::Lp,
            FormatTag::Json,
        ] {
            assert_eq!(FormatTag::from_byte(tag.byte()), Some(tag));
        }
        assert_eq!(FormatTag::from_byte(0), None);
        assert_eq!(FormatTag::from_byte(5), None);
    }

    #[test]
    fn the_two_mps_forms_are_different_tags() {
        // A file legal under both forms must not hash to the same instance.
        assert_ne!(FormatTag::MpsFixed.byte(), FormatTag::MpsFree.byte());
    }

    #[test]
    fn identifiers_order_and_print_by_their_index() {
        assert!(VarId(1) < VarId(2));
        assert!(RowId(1) < RowId(2));
        assert!(IneqId(1) < IneqId(2));
        assert_eq!(VarId(7).index(), 7);
        assert_eq!(RowId(7).to_string(), "7");
        assert_eq!(IneqId(7).to_string(), "7");
    }

    #[test]
    fn the_words_the_dump_writes_are_fixed() {
        assert_eq!(Side::Lower.as_str(), "lower");
        assert_eq!(Side::Upper.as_str(), "upper");
        assert_eq!(Sense::Min.as_str(), "min");
        assert_eq!(Sense::Max.as_str(), "max");
    }
}
