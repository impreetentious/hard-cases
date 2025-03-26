//! The verifier's own canonical model.
//!
//! Same mathematical object as the solver's, built from the same rules, and
//! deliberately not the same data structure. The solver stores its matrix by
//! column because a simplex asks for one column at a time; this stores a flat
//! list sorted by column and then by row, because the verifier's access
//! patterns are "walk everything once" and "look up one position", and a flat
//! sorted list answers both without a second index to keep consistent.
//!
//! The dump is the only output that has to match byte for byte. Everything
//! else about this type may differ from its counterpart, and the fact that two
//! unlike structures render identical bytes is the evidence that both read the
//! same file the same way.

use crate::vint::Int;
use crate::vrat::Frac;

/// Which format's rules produced an instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    /// MPS with positional column windows.
    MpsFixed,
    /// MPS with whitespace-delimited fields.
    MpsFree,
    /// The accepted LP-format subset.
    Lp,
    /// The JSON instance schema.
    Json,
}

impl Tag {
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
}

/// The direction the file was written in. The model always minimizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Minimize.
    Min,
    /// Maximize.
    Max,
}

impl Direction {
    /// The word the dump writes.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

/// One variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// The exact lower bound, absent when unbounded below.
    pub low: Option<Frac>,
    /// The exact upper bound, absent when unbounded above.
    pub high: Option<Frac>,
    /// Whether the model declares the column integer.
    pub whole: bool,
    /// The exact objective coefficient, minimizing.
    pub cost: Frac,
    /// The label the file used.
    pub name: String,
}

/// One retained row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constraint {
    /// The exact lower bound, absent when unbounded below.
    pub low: Option<Frac>,
    /// The exact upper bound, absent when unbounded above.
    pub high: Option<Frac>,
    /// The label the file used.
    pub name: String,
}

/// Which side of a two-sided object an axiom comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// The lower side, the row or bound unchanged.
    Low,
    /// The upper side, the row or bound negated.
    High,
}

impl Face {
    /// The word the dump writes.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Low => "lower",
            Self::High => "upper",
        }
    }
}

/// One canonical inequality: an axiom a proof is permitted to cite.
///
/// Variable bounds are axioms here exactly as row sides are. That is what
/// makes a contradiction a complete witness rather than one that leans on a
/// convention about which bounds a reader may assume were available.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Axiom {
    /// Whether the axiom comes from a row or a column.
    pub from_row: bool,
    /// The row or column index.
    pub at: u32,
    /// Which side.
    pub face: Face,
}

/// One stored coefficient.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The column index.
    pub column: u32,
    /// The row index.
    pub row: u32,
    /// The exact coefficient.
    pub value: Frac,
}

/// The canonical exact instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    /// Which rules produced it.
    pub tag: Tag,
    /// The digest of the tag byte followed by the file's exact bytes.
    pub digest: [u8; 32],
    /// The direction the file was written in.
    pub written: Direction,
    /// The objective constant, minimizing.
    pub constant: Frac,
    /// The columns, in index order.
    pub columns: Vec<Column>,
    /// The retained rows, in index order.
    pub rows: Vec<Constraint>,
    /// The coefficients, sorted by column and then row.
    pub cells: Vec<Cell>,
    /// Positions where duplicate entries cancelled.
    pub cancelled: Vec<(u32, u32)>,
    /// Names of free rows discarded after objective selection.
    pub dropped: Vec<String>,
}

impl Instance {
    /// The canonical inequalities, in identifier order.
    ///
    /// Rows first in index order, the lower side of a ranged row before its
    /// upper; then columns in index order, lower bound before upper. Only
    /// finite sides take an identifier, because an absent bound is not an
    /// axiom.
    #[must_use]
    pub fn axioms(&self) -> Vec<Axiom> {
        let mut out = Vec::new();
        for (index, row) in self.rows.iter().enumerate() {
            let at = index as u32;
            if row.low.is_some() {
                out.push(Axiom {
                    from_row: true,
                    at,
                    face: Face::Low,
                });
            }
            if row.high.is_some() {
                out.push(Axiom {
                    from_row: true,
                    at,
                    face: Face::High,
                });
            }
        }
        for (index, column) in self.columns.iter().enumerate() {
            let at = index as u32;
            if column.low.is_some() {
                out.push(Axiom {
                    from_row: false,
                    at,
                    face: Face::Low,
                });
            }
            if column.high.is_some() {
                out.push(Axiom {
                    from_row: false,
                    at,
                    face: Face::High,
                });
            }
        }
        out
    }

    /// The coefficient at a position, when one is stored there.
    #[must_use]
    pub fn cell(&self, column: u32, row: u32) -> Option<&Frac> {
        self.cells
            .binary_search_by(|entry| (entry.column, entry.row).cmp(&(column, row)))
            .ok()
            .map(|at| &self.cells[at].value)
    }

    /// The exact objective value at a point, including the constant.
    #[must_use]
    pub fn objective_at(&self, point: &[Frac]) -> Option<Frac> {
        if point.len() != self.columns.len() {
            return None;
        }
        let mut total = self.constant.clone();
        for (column, value) in self.columns.iter().zip(point) {
            if column.cost.is_zero() || value.is_zero() {
                continue;
            }
            total = total.plus(&column.cost.times(value));
        }
        Some(total)
    }

    /// The exact activity of one row at a point.
    #[must_use]
    pub fn activity_at(&self, row: u32, point: &[Frac]) -> Option<Frac> {
        if point.len() != self.columns.len() {
            return None;
        }
        let mut total = Frac::zero();
        for cell in &self.cells {
            if cell.row != row {
                continue;
            }
            let value = &point[cell.column as usize];
            if value.is_zero() {
                continue;
            }
            total = total.plus(&cell.value.times(value));
        }
        Some(total)
    }

    /// Renders the canonical dump.
    ///
    /// The output is compared byte for byte against the solver's. Every
    /// ordering here is total, so there is nowhere for two implementations to
    /// each be reasonable and still differ.
    #[must_use]
    pub fn dump(&self) -> String {
        let mut out = String::new();
        out.push_str("hc-canonical 1\n");
        out.push_str("format ");
        out.push_str(&self.tag.byte().to_string());
        out.push('\n');
        out.push_str("digest ");
        out.push_str(&crate::vhash::to_hex(&self.digest));
        out.push('\n');
        out.push_str("sense min original=");
        out.push_str(self.written.word());
        out.push('\n');
        out.push_str("objconst ");
        out.push_str(&self.constant.to_canonical());
        out.push('\n');

        let axioms = self.axioms();
        out.push_str("counts vars=");
        out.push_str(&self.columns.len().to_string());
        out.push_str(" rows=");
        out.push_str(&self.rows.len().to_string());
        out.push_str(" nonzeros=");
        out.push_str(&self.cells.len().to_string());
        out.push_str(" ineqs=");
        out.push_str(&axioms.len().to_string());
        out.push('\n');

        for (index, column) in self.columns.iter().enumerate() {
            out.push_str("var ");
            out.push_str(&index.to_string());
            out.push(' ');
            out.push(if column.whole { 'I' } else { 'C' });
            out.push(' ');
            out.push_str(&side(column.low.as_ref()));
            out.push(' ');
            out.push_str(&side(column.high.as_ref()));
            out.push(' ');
            out.push_str(&column.cost.to_canonical());
            out.push(' ');
            // The name is the rest of the line, so a name with a space in it
            // needs no quoting and no escape.
            out.push_str(&column.name);
            out.push('\n');
        }

        for (index, row) in self.rows.iter().enumerate() {
            out.push_str("row ");
            out.push_str(&index.to_string());
            out.push(' ');
            out.push_str(&side(row.low.as_ref()));
            out.push(' ');
            out.push_str(&side(row.high.as_ref()));
            out.push(' ');
            out.push_str(&row.name);
            out.push('\n');
        }

        for name in &self.dropped {
            out.push_str("discarded ");
            out.push_str(name);
            out.push('\n');
        }

        for cell in &self.cells {
            out.push_str("nz ");
            out.push_str(&cell.column.to_string());
            out.push(' ');
            out.push_str(&cell.row.to_string());
            out.push(' ');
            out.push_str(&cell.value.to_canonical());
            out.push('\n');
        }

        for (column, row) in &self.cancelled {
            out.push_str("zerosum ");
            out.push_str(&column.to_string());
            out.push(' ');
            out.push_str(&row.to_string());
            out.push('\n');
        }

        for (id, axiom) in axioms.iter().enumerate() {
            out.push_str("ineq ");
            out.push_str(&id.to_string());
            out.push(' ');
            out.push_str(if axiom.from_row { "row" } else { "var" });
            out.push(' ');
            out.push_str(&axiom.at.to_string());
            out.push(' ');
            out.push_str(axiom.face.word());
            out.push('\n');
        }

        out.push_str("end\n");
        out
    }
}

/// A bound, or the single character that means it is absent.
fn side(value: Option<&Frac>) -> String {
    match value {
        Some(bound) => bound.to_canonical(),
        None => String::from("*"),
    }
}

/// A convenience for building an exact value from a machine word.
#[must_use]
pub fn whole(value: i64) -> Frac {
    Frac::whole(Int::from_i64(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY_FILE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");
    const TINY_DUMP: &str = include_str!("../../../spec/v1/vectors/mps/tiny.canon");

    /// The smallest frozen instance, assembled by hand on this side.
    fn tiny() -> Instance {
        Instance {
            tag: Tag::MpsFixed,
            digest: crate::vhash::instance_digest(Tag::MpsFixed.byte(), TINY_FILE),
            written: Direction::Min,
            constant: Frac::zero(),
            columns: vec![Column {
                low: Some(whole(0)),
                high: Some(whole(10)),
                whole: true,
                cost: whole(-1),
                name: "X".to_owned(),
            }],
            rows: vec![Constraint {
                low: None,
                high: Some(whole(4)),
                name: "C1".to_owned(),
            }],
            cells: vec![Cell {
                column: 0,
                row: 0,
                value: whole(1),
            }],
            cancelled: Vec::new(),
            dropped: Vec::new(),
        }
    }

    #[test]
    fn the_hand_built_instance_renders_the_frozen_dump() {
        assert_eq!(tiny().dump(), TINY_DUMP);
    }

    #[test]
    fn the_dump_is_ascii_and_ends_where_it_should() {
        let dump = tiny().dump();
        assert!(dump.is_ascii());
        assert!(dump.ends_with("end\n"));
        assert!(!dump.contains('\r'));
        for line in dump.lines() {
            assert_eq!(line.trim_end(), line);
        }
    }

    #[test]
    fn axioms_are_rows_then_columns_and_only_the_finite_sides() {
        let instance = Instance {
            rows: vec![
                Constraint {
                    low: None,
                    high: Some(whole(1)),
                    name: "a".to_owned(),
                },
                Constraint {
                    low: Some(whole(0)),
                    high: Some(whole(1)),
                    name: "b".to_owned(),
                },
            ],
            columns: vec![
                Column {
                    low: Some(whole(0)),
                    high: None,
                    whole: false,
                    cost: Frac::zero(),
                    name: "x".to_owned(),
                },
                Column {
                    low: None,
                    high: None,
                    whole: false,
                    cost: Frac::zero(),
                    name: "y".to_owned(),
                },
            ],
            ..tiny()
        };
        let axioms = instance.axioms();
        assert_eq!(axioms.len(), 4);
        assert_eq!(
            axioms[0],
            Axiom {
                from_row: true,
                at: 0,
                face: Face::High
            }
        );
        assert_eq!(
            axioms[1],
            Axiom {
                from_row: true,
                at: 1,
                face: Face::Low
            }
        );
        assert_eq!(
            axioms[2],
            Axiom {
                from_row: true,
                at: 1,
                face: Face::High
            }
        );
        assert_eq!(
            axioms[3],
            Axiom {
                from_row: false,
                at: 0,
                face: Face::Low
            }
        );
    }

    #[test]
    fn positions_are_found_by_search_and_absences_reported() {
        let instance = tiny();
        assert_eq!(
            instance.cell(0, 0).map(|value| value.to_canonical()),
            Some("1".to_owned())
        );
        assert_eq!(instance.cell(0, 1), None);
        assert_eq!(instance.cell(1, 0), None);
    }

    #[test]
    fn the_objective_and_activity_are_exact_and_refuse_the_wrong_dimension() {
        let instance = tiny();
        let point = [whole(3)];
        assert_eq!(
            instance
                .objective_at(&point)
                .map(|value| value.to_canonical()),
            Some("-3".to_owned())
        );
        assert_eq!(
            instance
                .activity_at(0, &point)
                .map(|value| value.to_canonical()),
            Some("3".to_owned())
        );
        assert_eq!(instance.objective_at(&[]), None);
        assert_eq!(instance.activity_at(0, &[whole(1), whole(1)]), None);
    }
}
