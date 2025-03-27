//! Sections: which ones exist, in what order, and how often.
//!
//! The accepted list is closed, and an unknown header is refused rather than
//! skipped. Forward compatibility by ignoring what is not understood is a
//! reasonable policy for a configuration file and a disastrous one here: a
//! quadratic objective section quietly skipped leaves a model that reads
//! cleanly, solves cleanly, and is not the model in the file.
//!
//! Ordering is enforced because the format's meaning depends on it. A bound
//! record refers to a column, a column record refers to a row, and a reader
//! that accepted them in any order would have to buffer everything and resolve
//! names afterwards — which is a different reader with a different set of
//! things it can get wrong.

use hc_core::{Refusal, Rejection, Sense};

/// One accepted section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Section {
    /// The instance's name. Carries no model content.
    Name,
    /// The objective sense.
    ObjSense,
    /// Row declarations.
    Rows,
    /// Coefficients, by column.
    Columns,
    /// Right-hand sides.
    Rhs,
    /// Row ranges.
    Ranges,
    /// Column bounds.
    Bounds,
    /// The terminator.
    EndData,
}

impl Section {
    /// The section a header word names, when it names one.
    #[must_use]
    pub fn from_header(header: &str) -> Option<Self> {
        match header {
            "NAME" => Some(Self::Name),
            "OBJSENSE" => Some(Self::ObjSense),
            "ROWS" => Some(Self::Rows),
            "COLUMNS" => Some(Self::Columns),
            "RHS" => Some(Self::Rhs),
            "RANGES" => Some(Self::Ranges),
            "BOUNDS" => Some(Self::Bounds),
            "ENDATA" => Some(Self::EndData),
            _ => None,
        }
    }

    /// The header word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Name => "NAME",
            Self::ObjSense => "OBJSENSE",
            Self::Rows => "ROWS",
            Self::Columns => "COLUMNS",
            Self::Rhs => "RHS",
            Self::Ranges => "RANGES",
            Self::Bounds => "BOUNDS",
            Self::EndData => "ENDATA",
        }
    }

    /// Where the section must fall relative to the others.
    ///
    /// The two preamble sections share a rank because they may appear in
    /// either order. Everything else is strictly ordered, and the duplicate
    /// check below is what stops a shared rank from admitting a repeat.
    #[must_use]
    const fn rank(self) -> u8 {
        match self {
            Self::Name | Self::ObjSense => 0,
            Self::Rows => 1,
            Self::Columns => 2,
            Self::Rhs => 3,
            Self::Ranges => 4,
            Self::Bounds => 5,
            Self::EndData => 6,
        }
    }

    /// Whether a file without this section is incomplete.
    #[must_use]
    const fn required(self) -> bool {
        matches!(self, Self::Rows | Self::Columns | Self::EndData)
    }

    /// Every accepted section, in rank order.
    #[must_use]
    pub const fn all() -> [Self; 8] {
        [
            Self::Name,
            Self::ObjSense,
            Self::Rows,
            Self::Columns,
            Self::Rhs,
            Self::Ranges,
            Self::Bounds,
            Self::EndData,
        ]
    }
}

/// Tracks which sections have been seen and refuses an illegal sequence.
#[derive(Clone, Debug, Default)]
pub struct Sequence {
    seen: Vec<Section>,
    highest: Option<u8>,
}

impl Sequence {
    /// A fresh tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Admits a header word, returning the section it names.
    ///
    /// # Errors
    ///
    /// Returns a refusal for an unknown header, a repeated section, or one out
    /// of order.
    pub fn admit(&mut self, header: &str, line: usize) -> Result<Section, Refusal> {
        let Some(section) = Section::from_header(header) else {
            return Err(Refusal::new(
                Rejection::INSTANCE_UNKNOWN_SECTION,
                format!("unknown section header {header:?}"),
            )
            .at(line));
        };
        if self.seen.contains(&section) {
            return Err(Refusal::new(
                Rejection::INSTANCE_SECTION_ORDER,
                format!("section {} appears more than once", section.as_str()),
            )
            .at(line));
        }
        if let Some(highest) = self.highest {
            if section.rank() < highest {
                return Err(Refusal::new(
                    Rejection::INSTANCE_SECTION_ORDER,
                    format!("section {} appears out of order", section.as_str()),
                )
                .at(line));
            }
        }
        self.highest = Some(section.rank().max(self.highest.unwrap_or(0)));
        self.seen.push(section);
        Ok(section)
    }

    /// Whether a section has been seen.
    #[must_use]
    pub fn saw(&self, section: Section) -> bool {
        self.seen.contains(&section)
    }

    /// Refuses a file that ended without every required section.
    ///
    /// # Errors
    ///
    /// Returns a refusal naming the first missing section, or the terminator
    /// refusal when the file simply stopped.
    pub fn admit_end(&self) -> Result<(), Refusal> {
        if !self.saw(Section::EndData) {
            return Err(Refusal::new(
                Rejection::INSTANCE_UNEXPECTED_END,
                "input ended before the terminating section".to_owned(),
            ));
        }
        for section in Section::all() {
            if section.required() && !self.saw(section) {
                return Err(Refusal::new(
                    Rejection::INSTANCE_SECTION_MISSING,
                    format!("required section {} is missing", section.as_str()),
                ));
            }
        }
        Ok(())
    }
}

/// Reads an objective-sense word.
///
/// # Errors
///
/// Returns a refusal when the word is not one of the four accepted spellings.
pub fn read_sense(word: &str, line: usize) -> Result<Sense, Refusal> {
    match word {
        "MAX" | "MAXIMIZE" => Ok(Sense::Max),
        "MIN" | "MINIMIZE" => Ok(Sense::Min),
        _ => Err(Refusal::new(
            Rejection::INSTANCE_UNKNOWN_SECTION,
            format!("objective sense {word:?} is not one of MIN, MINIMIZE, MAX, or MAXIMIZE"),
        )
        .at(line)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_accepted_header_maps_to_its_section_and_back() {
        for section in Section::all() {
            assert_eq!(Section::from_header(section.as_str()), Some(section));
        }
        assert_eq!(Section::from_header("QUADOBJ"), None);
        assert_eq!(
            Section::from_header("rows"),
            None,
            "headers are not case folded"
        );
    }

    #[test]
    fn a_minimal_legal_sequence_is_admitted() {
        let mut sequence = Sequence::new();
        for header in ["ROWS", "COLUMNS", "ENDATA"] {
            sequence.admit(header, 1).expect("a legal sequence");
        }
        assert!(sequence.admit_end().is_ok());
    }

    #[test]
    fn the_preamble_may_appear_in_either_order() {
        for preamble in [["NAME", "OBJSENSE"], ["OBJSENSE", "NAME"]] {
            let mut sequence = Sequence::new();
            for header in preamble {
                sequence.admit(header, 1).expect("either order is legal");
            }
            for header in ["ROWS", "COLUMNS", "ENDATA"] {
                sequence.admit(header, 1).expect("a legal sequence");
            }
            assert!(sequence.admit_end().is_ok());
        }
    }

    #[test]
    fn the_optional_trailing_sections_may_be_skipped_but_not_reordered() {
        let mut sequence = Sequence::new();
        for header in ["ROWS", "COLUMNS", "BOUNDS", "ENDATA"] {
            sequence.admit(header, 1).expect("skipping is legal");
        }
        assert!(sequence.admit_end().is_ok());

        let mut backwards = Sequence::new();
        for header in ["ROWS", "COLUMNS", "RANGES"] {
            backwards.admit(header, 1).expect("a legal prefix");
        }
        let refusal = backwards.admit("RHS", 9).expect_err("out of order");
        assert_eq!(refusal.id, Rejection::INSTANCE_SECTION_ORDER);
        assert_eq!(refusal.line, Some(9));
    }

    #[test]
    fn a_repeated_section_is_refused() {
        let mut sequence = Sequence::new();
        sequence.admit("ROWS", 1).expect("legal");
        let refusal = sequence.admit("ROWS", 5).expect_err("a repeat");
        assert_eq!(refusal.id, Rejection::INSTANCE_SECTION_ORDER);
    }

    #[test]
    fn an_unknown_header_is_refused_rather_than_skipped() {
        let mut sequence = Sequence::new();
        for header in ["QUADOBJ", "SOS", "INDICATORS", "OBJSENSE MAX"] {
            let refusal = sequence.admit(header, 2).expect_err("unknown");
            assert_eq!(
                refusal.id,
                Rejection::INSTANCE_UNKNOWN_SECTION,
                "for {header:?}"
            );
        }
    }

    #[test]
    fn a_file_that_stops_early_is_named_as_stopping_early() {
        let mut sequence = Sequence::new();
        sequence.admit("ROWS", 1).expect("legal");
        sequence.admit("COLUMNS", 2).expect("legal");
        let refusal = sequence.admit_end().expect_err("no terminator");
        assert_eq!(refusal.id, Rejection::INSTANCE_UNEXPECTED_END);
    }

    #[test]
    fn a_terminated_file_missing_a_required_section_is_named_differently() {
        let mut sequence = Sequence::new();
        sequence.admit("ROWS", 1).expect("legal");
        sequence.admit("ENDATA", 2).expect("legal");
        let refusal = sequence.admit_end().expect_err("no columns");
        assert_eq!(refusal.id, Rejection::INSTANCE_SECTION_MISSING);
    }

    #[test]
    fn every_spelling_of_the_objective_sense_is_read() {
        assert_eq!(read_sense("MIN", 1).expect("legal"), Sense::Min);
        assert_eq!(read_sense("MINIMIZE", 1).expect("legal"), Sense::Min);
        assert_eq!(read_sense("MAX", 1).expect("legal"), Sense::Max);
        assert_eq!(read_sense("MAXIMIZE", 1).expect("legal"), Sense::Max);
        assert!(read_sense("MAXIMISE", 1).is_err());
        assert!(read_sense("max", 1).is_err());
    }
}
