//! The canonical dump.
//!
//! A total, byte-comparable rendering of the canonical model. This is how two
//! independently written parsers demonstrate that they read the same file the
//! same way — and it is compared as bytes, because a comparison performed
//! through either implementation's model type would be comparing one of them
//! against itself.
//!
//! Every ordering decision here is total. There is never a choice to make, so
//! there is never a place where two implementations could each be reasonable
//! and disagree. Names come last on their line and run to the line feed, so a
//! name containing a space needs no quoting and no escape. A cancelled pair of
//! duplicate coefficients gets a line of its own, because a cancellation and an
//! absence are the same matrix and different files, and the two parsers have to
//! agree about which one they were handed.

use core::fmt::Write as _;

use crate::hash::hex;
use crate::ids::Sense;
use crate::model::{IneqSource, Model};

/// The magic and version the first line carries.
const MAGIC: &str = "hc-canonical 1";

/// The single character an absent bound is written as.
const ABSENT: &str = "*";

impl Model {
    /// Renders the canonical dump.
    ///
    /// ASCII, line-feed endings, a trailing line feed after the final line, and
    /// no other trailing whitespace anywhere.
    #[must_use]
    pub fn canonical_dump(&self) -> String {
        let mut out = String::new();

        out.push_str(MAGIC);
        out.push('\n');
        let _ = writeln!(out, "format {}", self.format().byte());
        let _ = writeln!(out, "digest {}", hex(self.digest()));
        // The canonical sense is always minimize. The original is recorded
        // because the negation is otherwise invisible, and a sign error in it
        // produces a model that solves cleanly and answers a different
        // question.
        let _ = writeln!(
            out,
            "sense {} original={}",
            Sense::Min,
            self.original_sense()
        );
        let _ = writeln!(
            out,
            "objconst {}",
            self.objective_constant().to_canonical_string()
        );
        let _ = writeln!(
            out,
            "counts vars={} rows={} nonzeros={} ineqs={}",
            self.variables().len(),
            self.rows().len(),
            self.matrix().nonzeros(),
            self.inequalities().len()
        );

        for (index, variable) in self.variables().iter().enumerate() {
            let _ = writeln!(
                out,
                "var {} {} {} {} {} {}",
                index,
                if variable.integral { 'I' } else { 'C' },
                bound(variable.lower.as_ref()),
                bound(variable.upper.as_ref()),
                variable.objective.to_canonical_string(),
                variable.name
            );
        }

        for (index, row) in self.rows().iter().enumerate() {
            let _ = writeln!(
                out,
                "row {} {} {} {}",
                index,
                bound(row.lower.as_ref()),
                bound(row.upper.as_ref()),
                row.name
            );
        }

        for name in self.discarded() {
            let _ = writeln!(out, "discarded {name}");
        }

        for (variable, row, value) in self.matrix().entries() {
            let _ = writeln!(
                out,
                "nz {} {} {}",
                variable,
                row,
                value.to_canonical_string()
            );
        }

        for (variable, row) in self.zero_sums() {
            let _ = writeln!(out, "zerosum {variable} {row}");
        }

        for (id, inequality) in self.inequalities().iter().enumerate() {
            let (kind, index) = match inequality.source {
                IneqSource::Row(row) => ("row", row.index()),
                IneqSource::Var(variable) => ("var", variable.index()),
            };
            // The inequality's own coefficients are not restated. They are
            // determined by the model above, and repeating them here would
            // invite a dump that compares equal while the models differ.
            let _ = writeln!(out, "ineq {} {} {} {}", id, kind, index, inequality.side);
        }

        out.push_str("end\n");
        out
    }
}

/// A bound, or the single character that means it is absent.
fn bound(value: Option<&hc_exact::Rational>) -> String {
    value.map_or_else(
        || ABSENT.to_owned(),
        hc_exact::Rational::to_canonical_string,
    )
}

#[cfg(test)]
mod tests {
    use hc_exact::Rational;

    use crate::hash::instance_digest;
    use crate::ids::{FormatTag, RowId, Sense, VarId};
    use crate::model::{Model, ModelParts, Row, Variable};

    /// The frozen instance file and the dump both parsers must produce from it.
    const TINY_FILE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");
    const TINY_DUMP: &str = include_str!("../../../spec/v1/vectors/mps/tiny.canon");

    fn number(value: i64) -> Rational {
        Rational::from_i64(value)
    }

    /// The model the smallest frozen instance names, assembled by hand.
    ///
    /// Assembled by hand deliberately: no parser exists yet, and the dump has
    /// to be right before a parser is written against it, or the first parser
    /// becomes the specification of the dump rather than the other way round.
    fn tiny_model() -> Model {
        Model::new(ModelParts {
            format: FormatTag::MpsFixed,
            digest: instance_digest(FormatTag::MpsFixed.byte(), TINY_FILE),
            original_sense: Sense::Min,
            objective_constant: Rational::zero(),
            variables: vec![Variable {
                lower: Some(number(0)),
                upper: Some(number(10)),
                integral: true,
                objective: number(-1),
                name: "X".to_owned(),
            }],
            rows: vec![Row {
                lower: None,
                upper: Some(number(4)),
                name: "C1".to_owned(),
            }],
            entries: vec![(VarId(0), RowId(0), number(1))],
            zero_sums: Vec::new(),
            discarded: Vec::new(),
        })
    }

    #[test]
    fn the_hand_built_model_renders_the_frozen_dump_byte_for_byte() {
        assert_eq!(tiny_model().canonical_dump(), TINY_DUMP);
    }

    #[test]
    fn the_dump_is_ascii_and_ends_in_exactly_one_line_feed() {
        let dump = tiny_model().canonical_dump();
        assert!(dump.is_ascii(), "the dump is not ASCII");
        assert!(dump.ends_with("end\n"));
        assert!(!dump.ends_with("end\n\n"));
        assert!(!dump.contains('\r'), "the dump carries a carriage return");
        for line in dump.lines() {
            assert_eq!(
                line.trim_end(),
                line,
                "a line carries trailing whitespace: {line:?}"
            );
        }
    }

    #[test]
    fn absent_bounds_and_the_maximizing_sense_are_both_visible() {
        let mut parts = ModelParts {
            format: FormatTag::MpsFree,
            digest: [0; 32],
            original_sense: Sense::Max,
            objective_constant: number(-5),
            variables: vec![Variable {
                lower: None,
                upper: None,
                integral: false,
                objective: number(1),
                name: "free var".to_owned(),
            }],
            rows: vec![Row {
                lower: Some(number(1)),
                upper: None,
                name: "r".to_owned(),
            }],
            entries: vec![(VarId(0), RowId(0), number(1))],
            zero_sums: Vec::new(),
            discarded: vec!["N2".to_owned(), "N3".to_owned()],
        };
        parts.digest[0] = 0xab;
        let dump = Model::new(parts).canonical_dump();

        assert!(dump.contains("format 2\n"));
        assert!(dump.starts_with("hc-canonical 1\nformat 2\ndigest ab00"));
        assert!(dump.contains("sense min original=max\n"));
        assert!(dump.contains("objconst -5\n"));
        // A name containing a space needs no quoting, because the name is the
        // rest of the line.
        assert!(dump.contains("var 0 C * * 1 free var\n"));
        assert!(dump.contains("row 0 1 * r\n"));
        assert!(dump.contains("discarded N2\ndiscarded N3\n"));
        // Only the finite sides take identifiers: the row's lower bound, and
        // nothing from an unbounded variable.
        assert!(dump.contains("counts vars=1 rows=1 nonzeros=1 ineqs=1\n"));
        assert!(dump.contains("ineq 0 row 0 lower\n"));
    }

    #[test]
    fn a_cancelled_pair_is_written_where_its_coefficient_would_have_been() {
        let parts = ModelParts {
            format: FormatTag::MpsFixed,
            digest: [0; 32],
            original_sense: Sense::Min,
            objective_constant: Rational::zero(),
            variables: vec![
                Variable {
                    lower: Some(number(0)),
                    upper: None,
                    integral: false,
                    objective: Rational::zero(),
                    name: "a".to_owned(),
                },
                Variable {
                    lower: Some(number(0)),
                    upper: None,
                    integral: false,
                    objective: Rational::zero(),
                    name: "b".to_owned(),
                },
            ],
            rows: vec![Row {
                lower: Some(number(0)),
                upper: None,
                name: "r".to_owned(),
            }],
            entries: vec![(VarId(1), RowId(0), number(2))],
            zero_sums: vec![(VarId(0), RowId(0))],
            discarded: Vec::new(),
        };
        let dump = Model::new(parts).canonical_dump();
        assert!(dump.contains("nz 1 0 2\n"));
        assert!(dump.contains("zerosum 0 0\n"));
        // The cancelled entry is not a stored coefficient and does not count.
        assert!(dump.contains("nonzeros=1"));
    }

    #[test]
    fn the_counts_line_makes_a_truncated_dump_unable_to_compare_equal() {
        let dump = tiny_model().canonical_dump();
        let counts = dump
            .lines()
            .find(|line| line.starts_with("counts "))
            .expect("a counts line");
        assert_eq!(counts, "counts vars=1 rows=1 nonzeros=1 ineqs=3");
    }
}
