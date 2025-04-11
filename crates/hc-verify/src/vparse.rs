//! The verifier's instance parser.
//!
//! A second reader for the same format, written separately, sharing nothing
//! with the solver's. The value of the pair is not that one of them is right —
//! it is that when they disagree about a file, the disagreement is visible
//! before anything is proved about that file, and either implementation may be
//! the one that is wrong.
//!
//! The construction is deliberately unlike its counterpart. There is no shared
//! record type and no line classifier handing fields to section handlers: this
//! walks the input once, in a small state machine, and each section pulls its
//! own fields out of the line it is looking at. Name tables are sorted vectors
//! searched by bisection rather than trees. None of that is better; it is
//! different, and different is the property being bought.
//!
//! Strictness is not negotiable here. The verifier has no lenient mode at all.
//! A file the solver read leniently must be independently accepted by this
//! parser, in strict form, before anything about it can carry a claim.

use crate::vmodel::{Cell, Column, Constraint, Direction, Instance, Tag};
use crate::vrat::Frac;
use crate::vreject::{Code, Rejected};

/// The longest accepted input, in bytes.
pub(crate) const MAX_FILE: usize = 268_435_456;
/// The largest accepted number of stored coefficients.
const MAX_CELLS: usize = 10_000_000;
/// The longest accepted name, in bytes.
const MAX_NAME: usize = 255;
/// The longest accepted numeric token, in bytes.
const MAX_TOKEN: usize = 4096;
/// The widest accepted value, in bits, projected from its token.
const MAX_BITS: u64 = 1_048_576;

/// The fixed-form windows, one-based and inclusive.
const FIELDS: [(usize, usize); 6] = [(2, 3), (5, 12), (15, 22), (25, 36), (40, 47), (50, 61)];
/// The columns a fixed-form record must leave blank.
const GAPS: [(usize, usize); 6] = [(1, 1), (4, 4), (13, 14), (23, 24), (37, 39), (48, 49)];

/// Which variant's rules apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Fields are read from their column windows.
    Fixed,
    /// Fields are whitespace-delimited.
    Free,
}

impl Layout {
    const fn tag(self) -> Tag {
        match self {
            Self::Fixed => Tag::MpsFixed,
            Self::Free => Tag::MpsFree,
        }
    }
}

/// The sections, in the order they must appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Name,
    Sense,
    Rows,
    Columns,
    Rhs,
    Ranges,
    Bounds,
    End,
}

impl Part {
    fn named(word: &str) -> Option<Self> {
        match word {
            "NAME" => Some(Self::Name),
            "OBJSENSE" => Some(Self::Sense),
            "ROWS" => Some(Self::Rows),
            "COLUMNS" => Some(Self::Columns),
            "RHS" => Some(Self::Rhs),
            "RANGES" => Some(Self::Ranges),
            "BOUNDS" => Some(Self::Bounds),
            "ENDATA" => Some(Self::End),
            _ => None,
        }
    }

    const fn place(self) -> u8 {
        match self {
            // The two preamble sections share a place because either may come
            // first; the repeat check is what keeps that from admitting one
            // of them twice.
            Self::Name | Self::Sense => 0,
            Self::Rows => 1,
            Self::Columns => 2,
            Self::Rhs => 3,
            Self::Ranges => 4,
            Self::Bounds => 5,
            Self::End => 6,
        }
    }

    const fn word(self) -> &'static str {
        match self {
            Self::Name => "NAME",
            Self::Sense => "OBJSENSE",
            Self::Rows => "ROWS",
            Self::Columns => "COLUMNS",
            Self::Rhs => "RHS",
            Self::Ranges => "RANGES",
            Self::Bounds => "BOUNDS",
            Self::End => "ENDATA",
        }
    }
}

/// How a row was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sense {
    Free,
    Less,
    More,
    Equal,
}

/// A declared row and what has been said about it.
struct RowSlot {
    name: String,
    sense: Sense,
    kept: Option<u32>,
    rhs: Option<Frac>,
    range: Option<Frac>,
}

/// A declared column and what has been said about it.
struct ColumnSlot {
    name: String,
    whole: bool,
    cost: Frac,
    low: Option<Frac>,
    high: Option<Frac>,
    low_default: bool,
    codes: Vec<String>,
}

/// A name table: entries paired with their index, kept sorted for bisection.
///
/// A sorted vector rather than a tree. The tables here are built once and read
/// many times, the comparison is the same either way, and a vector is one
/// fewer structure whose invariants a reader of this crate has to take on
/// trust.
#[derive(Default)]
pub(crate) struct Names {
    pub(crate) sorted: Vec<(String, usize)>,
}

impl Names {
    pub(crate) fn find(&self, name: &str) -> Option<usize> {
        self.sorted
            .binary_search_by(|(known, _)| known.as_str().cmp(name))
            .ok()
            .map(|at| self.sorted[at].1)
    }

    /// Inserts a name, reporting whether it was already present.
    pub(crate) fn insert(&mut self, name: &str, index: usize) -> bool {
        match self
            .sorted
            .binary_search_by(|(known, _)| known.as_str().cmp(name))
        {
            Ok(_) => false,
            Err(at) => {
                self.sorted.insert(at, (name.to_owned(), index));
                true
            }
        }
    }
}

/// Reads an instance file.
///
/// # Errors
///
/// Returns a refusal naming the rule that refused the file.
pub fn read(raw: &[u8], layout: Layout) -> Result<Instance, Rejected> {
    if raw.len() > MAX_FILE {
        return Err(Rejected::new(
            Code::FILE_TOO_LARGE,
            format!("file is {} bytes, over the limit of {MAX_FILE}", raw.len()),
        ));
    }
    let text = as_text(raw)?;
    let digest = crate::vhash::instance_digest(layout.tag().byte(), raw);
    Reader::new(layout, digest).walk(text)
}

/// Refuses input that is not ASCII text with line-feed endings.
pub(crate) fn as_text(raw: &[u8]) -> Result<&str, Rejected> {
    let mut at = 0usize;
    while at < raw.len() {
        let byte = raw[at];
        let fine = byte == b'\n' || byte == b'\t' || (0x20..=0x7e).contains(&byte);
        if !fine {
            return Err(Rejected::new(
                Code::NOT_TEXT,
                format!("byte {byte:#04x} at offset {at} is not accepted input"),
            ));
        }
        at += 1;
    }
    core::str::from_utf8(raw)
        .map_err(|error| Rejected::new(Code::NOT_TEXT, format!("input is not text: {error}")))
}

/// The reader's state.
struct Reader {
    layout: Layout,
    digest: [u8; 32],

    seen: Vec<Part>,
    furthest: u8,

    rows: Vec<RowSlot>,
    row_names: Names,
    objective: Option<usize>,
    kept_rows: u32,
    dropped: Vec<String>,

    columns: Vec<ColumnSlot>,
    column_names: Names,
    open_column: Option<usize>,

    /// Accumulated coefficients: column, row, running sum, and how many
    /// records contributed. Sorted at the end rather than kept sorted, because
    /// the records arrive column by column already.
    cells: Vec<(u32, u32, Frac, u32)>,
    cell_index: Names,

    constant: Frac,
    written: Direction,

    rhs_vector: Option<String>,
    ranges_vector: Option<String>,
    bounds_vector: Option<String>,

    in_markers: bool,
}

impl Reader {
    fn new(layout: Layout, digest: [u8; 32]) -> Self {
        Self {
            layout,
            digest,
            seen: Vec::new(),
            furthest: 0,
            rows: Vec::new(),
            row_names: Names::default(),
            objective: None,
            kept_rows: 0,
            dropped: Vec::new(),
            columns: Vec::new(),
            column_names: Names::default(),
            open_column: None,
            cells: Vec::new(),
            cell_index: Names::default(),
            constant: Frac::zero(),
            written: Direction::Min,
            rhs_vector: None,
            ranges_vector: None,
            bounds_vector: None,
            in_markers: false,
        }
    }

    fn walk(mut self, text: &str) -> Result<Instance, Rejected> {
        let mut part: Option<Part> = None;
        let mut line_number = 0usize;

        for line in text.split('\n') {
            line_number += 1;
            if part == Some(Part::End) {
                // Content after the terminator is ignored for parsing and is
                // still inside the digest, so it cannot smuggle in a second
                // model.
                continue;
            }
            if line.is_empty() || line.bytes().all(|byte| byte == b' ' || byte == b'\t') {
                continue;
            }
            if line.as_bytes()[0] == b'*' {
                continue;
            }

            let leading = line.as_bytes()[0];
            if leading != b' ' && leading != b'\t' {
                part = Some(self.header(line, line_number)?);
                continue;
            }

            let Some(open) = part else {
                return Err(Rejected::new(
                    Code::SECTION_MISSING,
                    "a record appears before any section header".to_owned(),
                )
                .on_line(line_number));
            };
            self.body(open, line, line_number)?;
        }

        self.close()
    }

    fn header(&mut self, line: &str, at: usize) -> Result<Part, Rejected> {
        let trimmed = line.trim_end();
        let cut = trimmed.find([' ', '\t']);
        let (word, tail) = match cut {
            Some(index) => (&trimmed[..index], trimmed[index..].trim()),
            None => (trimmed, ""),
        };

        let Some(part) = Part::named(word) else {
            return Err(Rejected::new(
                Code::UNKNOWN_SECTION,
                format!("unknown section header {word:?}"),
            )
            .on_line(at));
        };
        if self.seen.contains(&part) {
            return Err(Rejected::new(
                Code::SECTION_ORDER,
                format!("section {} appears more than once", part.word()),
            )
            .on_line(at));
        }
        if part.place() < self.furthest {
            return Err(Rejected::new(
                Code::SECTION_ORDER,
                format!("section {} appears out of order", part.word()),
            )
            .on_line(at));
        }
        self.furthest = self.furthest.max(part.place());
        self.seen.push(part);

        if part == Part::Sense && !tail.is_empty() {
            if self.layout == Layout::Fixed {
                return Err(Rejected::new(
                    Code::UNKNOWN_SECTION,
                    "the fixed form writes the objective sense on its own record".to_owned(),
                )
                .on_line(at));
            }
            self.written = read_direction(tail, at)?;
        }
        Ok(part)
    }

    /// Pulls a record's six roles out of a line.
    ///
    /// `leads` says whether the first role is an indicator code, which is what
    /// the free form needs in order to know where the names start.
    fn roles<'a>(&self, line: &'a str, leads: bool, at: usize) -> Result<[&'a str; 6], Rejected> {
        match self.layout {
            Layout::Fixed => {
                for &(from, to) in &GAPS {
                    let gap = slice(line, from, to);
                    if !gap.is_empty() {
                        return Err(Rejected::new(
                            Code::FIELD_OVERRUN,
                            format!("content in columns {from} to {to}: {gap:?}"),
                        )
                        .on_line(at));
                    }
                }
                let tail = slice(line, 62, usize::MAX);
                if !tail.is_empty() {
                    return Err(Rejected::new(
                        Code::FIELD_OVERRUN,
                        format!("content past column 61: {tail:?}"),
                    )
                    .on_line(at));
                }
                let mut out = [""; 6];
                for (slot, &(from, to)) in out.iter_mut().zip(&FIELDS) {
                    *slot = slice(line, from, to);
                }
                if !leads && !out[0].is_empty() {
                    return Err(Rejected::new(
                        Code::FIELD_OVERRUN,
                        format!(
                            "this section takes no indicator, and one is present: {:?}",
                            out[0]
                        ),
                    )
                    .on_line(at));
                }
                Ok(out)
            }
            Layout::Free => {
                let words: Vec<&str> = line.split_whitespace().collect();
                let accepted = if leads { 6 } else { 5 };
                if words.len() > accepted {
                    return Err(Rejected::new(
                        Code::TOO_MANY_FIELDS,
                        format!(
                            "record carries {} fields and its type accepts {accepted}",
                            words.len()
                        ),
                    )
                    .on_line(at));
                }
                let mut out = [""; 6];
                let shift = usize::from(leads);
                if leads && !words.is_empty() {
                    out[0] = words[0];
                }
                for (index, word) in words.iter().skip(shift).enumerate() {
                    out[index + 1] = word;
                }
                Ok(out)
            }
        }
    }

    fn body(&mut self, part: Part, line: &str, at: usize) -> Result<(), Rejected> {
        match part {
            Part::Name | Part::End => Ok(()),
            Part::Sense => {
                let roles = self.roles(line, true, at)?;
                let word = if roles[0].is_empty() {
                    roles[1]
                } else {
                    roles[0]
                };
                self.written = read_direction(word, at)?;
                Ok(())
            }
            Part::Rows => self.row_line(line, at),
            Part::Columns => self.column_line(line, at),
            Part::Rhs => self.rhs_line(line, at),
            Part::Ranges => self.range_line(line, at),
            Part::Bounds => self.bound_line(line, at),
        }
    }

    fn row_line(&mut self, line: &str, at: usize) -> Result<(), Rejected> {
        let roles = self.roles(line, true, at)?;
        let sense = match roles[0] {
            "N" => Sense::Free,
            "L" => Sense::Less,
            "G" => Sense::More,
            "E" => Sense::Equal,
            other => {
                return Err(Rejected::new(
                    Code::UNKNOWN_SECTION,
                    format!("row sense {other:?} is not one of N, L, G, or E"),
                )
                .on_line(at));
            }
        };
        let name = roles[1];
        check_name(name, at)?;

        let position = self.rows.len();
        if !self.row_names.insert(name, position) {
            return Err(Rejected::new(
                Code::DUPLICATE_ROW,
                format!("row {name:?} is declared twice"),
            )
            .on_line(at));
        }

        let kept = if sense == Sense::Free {
            if self.objective.is_none() {
                self.objective = Some(position);
            } else {
                self.dropped.push(name.to_owned());
            }
            None
        } else {
            let index = self.kept_rows;
            self.kept_rows += 1;
            Some(index)
        };

        self.rows.push(RowSlot {
            name: name.to_owned(),
            sense,
            kept,
            rhs: None,
            range: None,
        });
        Ok(())
    }

    fn column_line(&mut self, line: &str, at: usize) -> Result<(), Rejected> {
        let roles = self.roles(line, false, at)?;

        if let Some(opening) = self.marker(line, &roles) {
            if opening == self.in_markers {
                return Err(Rejected::new(
                    Code::MARKER_BALANCE,
                    if opening {
                        "an integrality marker opens inside another".to_owned()
                    } else {
                        "an integrality marker closes without an opening one".to_owned()
                    },
                )
                .on_line(at));
            }
            self.in_markers = opening;
            return Ok(());
        }

        let name = roles[1];
        check_name(name, at)?;
        let column = match self.column_names.find(name) {
            Some(existing) => {
                if self.open_column != Some(existing) {
                    return Err(Rejected::new(
                        Code::COLUMN_SPLIT,
                        format!("column {name:?} resumes after another column started"),
                    )
                    .on_line(at));
                }
                existing
            }
            None => {
                let index = self.columns.len();
                self.column_names.insert(name, index);
                self.columns.push(ColumnSlot {
                    name: name.to_owned(),
                    whole: self.in_markers,
                    cost: Frac::zero(),
                    low: Some(Frac::zero()),
                    high: None,
                    low_default: true,
                    codes: Vec::new(),
                });
                index
            }
        };
        self.open_column = Some(column);

        self.coefficient(column, roles[2], roles[3], at)?;
        if !roles[4].is_empty() || !roles[5].is_empty() {
            self.coefficient(column, roles[4], roles[5], at)?;
        }
        Ok(())
    }

    /// Whether a record opens or closes the integrality markers.
    ///
    /// The fixed form puts the literal in the third window and the code in the
    /// fifth. The free form writes exactly three fields: a label, the literal,
    /// and the code. Anything else in those positions is an ordinary entry.
    fn marker(&self, line: &str, roles: &[&str; 6]) -> Option<bool> {
        let (literal, code) = match self.layout {
            Layout::Fixed => (bare(roles[2]), bare(roles[4])),
            Layout::Free => {
                if line.split_whitespace().count() != 3 {
                    return None;
                }
                (bare(roles[2]), bare(roles[3]))
            }
        };
        if literal != "MARKER" {
            return None;
        }
        match code {
            "INTORG" => Some(true),
            "INTEND" => Some(false),
            _ => None,
        }
    }

    fn coefficient(
        &mut self,
        column: usize,
        row_name: &str,
        token: &str,
        at: usize,
    ) -> Result<(), Rejected> {
        if row_name.is_empty() && token.is_empty() {
            return Ok(());
        }
        let row = self.row(row_name, at)?;
        let value = number(token, at)?;
        representable(&value, "a coefficient", at)?;

        if Some(row) == self.objective {
            let existing = core::mem::replace(&mut self.columns[column].cost, Frac::zero());
            self.columns[column].cost = existing.plus(&value);
            return Ok(());
        }
        let Some(kept) = self.rows[row].kept else {
            return Ok(());
        };

        let key = format!("{column}:{kept}");
        match self.cell_index.find(&key) {
            Some(slot) => {
                let running = core::mem::replace(&mut self.cells[slot].2, Frac::zero());
                self.cells[slot].2 = running.plus(&value);
                self.cells[slot].3 += 1;
            }
            None => {
                if self.cells.len() >= MAX_CELLS {
                    return Err(Rejected::new(
                        Code::TOO_MANY_NONZEROS,
                        format!("more than {MAX_CELLS} stored coefficients"),
                    )
                    .on_line(at));
                }
                self.cell_index.insert(&key, self.cells.len());
                self.cells.push((column as u32, kept, value, 1));
            }
        }
        Ok(())
    }

    fn rhs_line(&mut self, line: &str, at: usize) -> Result<(), Rejected> {
        let roles = self.roles(line, false, at)?;
        self.vector_name(roles[1], at, 0)?;
        self.rhs(roles[2], roles[3], at)?;
        if !roles[4].is_empty() || !roles[5].is_empty() {
            self.rhs(roles[4], roles[5], at)?;
        }
        Ok(())
    }

    fn rhs(&mut self, row_name: &str, token: &str, at: usize) -> Result<(), Rejected> {
        if row_name.is_empty() && token.is_empty() {
            return Ok(());
        }
        let row = self.row(row_name, at)?;
        let value = number(token, at)?;
        representable(&value, "a right-hand side", at)?;
        if Some(row) == self.objective {
            // An entry on the objective row supplies the negation of the
            // objective constant, and a later entry replaces an earlier one.
            self.constant = value.negated();
            return Ok(());
        }
        self.rows[row].rhs = Some(value);
        Ok(())
    }

    fn range_line(&mut self, line: &str, at: usize) -> Result<(), Rejected> {
        let roles = self.roles(line, false, at)?;
        self.vector_name(roles[1], at, 1)?;
        self.range(roles[2], roles[3], at)?;
        if !roles[4].is_empty() || !roles[5].is_empty() {
            self.range(roles[4], roles[5], at)?;
        }
        Ok(())
    }

    fn range(&mut self, row_name: &str, token: &str, at: usize) -> Result<(), Rejected> {
        if row_name.is_empty() && token.is_empty() {
            return Ok(());
        }
        let row = self.row(row_name, at)?;
        if self.rows[row].sense == Sense::Free {
            return Err(Rejected::new(
                Code::RANGE_ON_FREE_ROW,
                format!("row {row_name:?} has no sense to range"),
            )
            .on_line(at));
        }
        let value = number(token, at)?;
        representable(&value, "a range", at)?;
        self.rows[row].range = Some(value);
        Ok(())
    }

    fn bound_line(&mut self, line: &str, at: usize) -> Result<(), Rejected> {
        let roles = self.roles(line, true, at)?;
        self.vector_name(roles[1], at, 2)?;

        let code = roles[0];
        if !matches!(
            code,
            "LO" | "UP" | "FX" | "FR" | "MI" | "PL" | "BV" | "LI" | "UI"
        ) {
            return Err(Rejected::new(
                Code::UNKNOWN_BOUND,
                format!("bound type {code:?} is not accepted"),
            )
            .on_line(at));
        }

        let column_name = roles[2];
        let Some(column) = self.column_names.find(column_name) else {
            return Err(Rejected::new(
                Code::UNKNOWN_COLUMN,
                format!("bound names column {column_name:?}, which was never declared"),
            )
            .on_line(at));
        };
        if self.columns[column].codes.iter().any(|seen| seen == code) {
            return Err(Rejected::new(
                Code::DUPLICATE_BOUND,
                format!("column {column_name:?} already has a {code} bound"),
            )
            .on_line(at));
        }
        self.columns[column].codes.push(code.to_owned());

        let value = if matches!(code, "LO" | "UP" | "FX" | "LI" | "UI") {
            let value = number(roles[3], at)?;
            representable(&value, "a bound", at)?;
            Some(value)
        } else {
            None
        };

        let slot = &mut self.columns[column];
        match code {
            "LO" => {
                slot.low = value;
                slot.low_default = false;
            }
            "UP" => {
                let value = value.expect("UP carries a value");
                if value.is_minus() && slot.low_default {
                    // Readers disagree about what this means and about whether
                    // it means it only for integer columns. There is no
                    // lenient mode here to fall back on.
                    return Err(Rejected::new(
                        Code::AMBIGUOUS_NEGATIVE_UPPER,
                        format!(
                            "column {:?} takes a negative upper bound while still at the default lower bound",
                            slot.name
                        ),
                    )
                    .on_line(at));
                }
                slot.high = Some(value);
            }
            "FX" => {
                slot.low = value.clone();
                slot.high = value;
                slot.low_default = false;
            }
            "FR" => {
                slot.low = None;
                slot.high = None;
                slot.low_default = false;
            }
            "MI" => {
                slot.low = None;
                slot.low_default = false;
            }
            "PL" => slot.high = None,
            "BV" => {
                slot.low = Some(Frac::zero());
                slot.high = Some(Frac::one());
                slot.whole = true;
                slot.low_default = false;
            }
            _ => {
                let value = value.expect("an integer bound carries a value");
                if !value.is_whole() {
                    return Err(Rejected::new(
                        Code::FRACTIONAL_INTEGER_BOUND,
                        format!(
                            "{code} bound on column {:?} is {}, which is not an exact integer",
                            slot.name,
                            value.to_canonical()
                        ),
                    )
                    .on_line(at));
                }
                if code == "LI" {
                    slot.low = Some(value);
                    slot.low_default = false;
                } else {
                    slot.high = Some(value);
                }
                slot.whole = true;
            }
        }
        Ok(())
    }

    fn vector_name(&mut self, name: &str, at: usize, which: usize) -> Result<(), Rejected> {
        check_name(name, at)?;
        let (slot, label) = match which {
            0 => (&mut self.rhs_vector, "RHS"),
            1 => (&mut self.ranges_vector, "RANGES"),
            _ => (&mut self.bounds_vector, "BOUNDS"),
        };
        match slot {
            Some(existing) if existing == name => Ok(()),
            Some(existing) => Err(Rejected::new(
                Code::MULTIPLE_VECTORS,
                format!("{label} carries vector {name:?} after vector {existing:?}"),
            )
            .on_line(at)),
            None => {
                *slot = Some(name.to_owned());
                Ok(())
            }
        }
    }

    fn row(&self, name: &str, at: usize) -> Result<usize, Rejected> {
        self.row_names.find(name).ok_or_else(|| {
            Rejected::new(
                Code::UNKNOWN_ROW,
                format!("record names row {name:?}, which was never declared"),
            )
            .on_line(at)
        })
    }

    fn close(mut self) -> Result<Instance, Rejected> {
        if !self.seen.contains(&Part::End) {
            return Err(Rejected::new(
                Code::UNEXPECTED_END,
                "input ended before the terminating section".to_owned(),
            ));
        }
        for required in [Part::Rows, Part::Columns] {
            if !self.seen.contains(&required) {
                return Err(Rejected::new(
                    Code::SECTION_MISSING,
                    format!("required section {} is missing", required.word()),
                ));
            }
        }
        if self.objective.is_none() {
            return Err(Rejected::new(
                Code::NO_OBJECTIVE,
                "no free row was declared to serve as the objective".to_owned(),
            ));
        }
        if self.in_markers {
            return Err(Rejected::new(
                Code::MARKER_BALANCE,
                "an integrality marker opens and never closes".to_owned(),
            ));
        }

        let flip = self.written == Direction::Max;
        let columns = self
            .columns
            .iter()
            .map(|slot| Column {
                low: slot.low.clone(),
                high: slot.high.clone(),
                whole: slot.whole,
                cost: if flip {
                    slot.cost.negated()
                } else {
                    slot.cost.clone()
                },
                name: slot.name.clone(),
            })
            .collect();

        let mut rows = Vec::new();
        for slot in &self.rows {
            if slot.kept.is_none() {
                continue;
            }
            let (low, high) = sides(slot);
            rows.push(Constraint {
                low,
                high,
                name: slot.name.clone(),
            });
        }

        self.cells.sort_by_key(|entry| (entry.0, entry.1));
        let mut cells = Vec::new();
        let mut cancelled = Vec::new();
        for (column, row, value, contributions) in self.cells {
            if value.is_zero() {
                if contributions > 1 {
                    cancelled.push((column, row));
                }
                continue;
            }
            cells.push(Cell { column, row, value });
        }

        let constant = if flip {
            self.constant.negated()
        } else {
            self.constant
        };

        Ok(Instance {
            tag: self.layout.tag(),
            digest: self.digest,
            written: self.written,
            constant,
            columns,
            rows,
            cells,
            cancelled,
            dropped: self.dropped,
        })
    }
}

/// The exact bounds a row's sense, right-hand side, and range produce.
fn sides(slot: &RowSlot) -> (Option<Frac>, Option<Frac>) {
    let base = slot.rhs.clone().unwrap_or_else(Frac::zero);
    let Some(range) = slot.range.clone() else {
        return match slot.sense {
            Sense::Free => (None, None),
            Sense::Less => (None, Some(base)),
            Sense::More => (Some(base), None),
            Sense::Equal => (Some(base.clone()), Some(base)),
        };
    };
    let size = range.magnitude();
    match slot.sense {
        Sense::Free => (None, None),
        Sense::More => (Some(base.clone()), Some(base.plus(&size))),
        Sense::Less => (Some(base.minus(&size)), Some(base)),
        Sense::Equal => {
            if range.is_minus() {
                (Some(base.plus(&range)), Some(base))
            } else {
                (Some(base.clone()), Some(base.plus(&range)))
            }
        }
    }
}

/// One fixed-form window, trimmed at both ends.
fn slice(line: &str, from: usize, to: usize) -> &str {
    let bytes = line.as_bytes();
    let start = if from == 0 {
        0
    } else {
        (from - 1).min(bytes.len())
    };
    let end = to.min(bytes.len());
    if start >= end {
        return "";
    }
    line[start..end].trim_matches(|character| character == ' ' || character == '\t')
}

/// Strips one surrounding pair of apostrophes.
fn bare(text: &str) -> &str {
    if text.len() >= 2 && text.starts_with('\'') && text.ends_with('\'') {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

pub(crate) fn check_name(name: &str, at: usize) -> Result<(), Rejected> {
    if name.len() > MAX_NAME {
        return Err(Rejected::new(
            Code::NAME_LENGTH,
            format!("name is {} bytes, over the limit of {MAX_NAME}", name.len()),
        )
        .on_line(at));
    }
    Ok(())
}

pub(crate) fn number(token: &str, at: usize) -> Result<Frac, Rejected> {
    use crate::vtext::NumberError;
    match Frac::from_decimal_token(token, MAX_TOKEN, MAX_BITS) {
        Ok(value) => Ok(value),
        Err(error) => {
            let code = match error {
                NumberError::TokenTooLong => Code::TOKEN_LENGTH,
                NumberError::TooWide | NumberError::ExponentTooLarge => Code::TOKEN_WIDTH,
                _ => Code::BAD_NUMBER,
            };
            Err(Rejected::new(code, format!("{error}: {token:?}")).on_line(at))
        }
    }
}

pub(crate) fn representable(value: &Frac, what: &str, at: usize) -> Result<(), Rejected> {
    if value.inside_binary64() {
        return Ok(());
    }
    Err(Rejected::new(
        Code::NOT_REPRESENTABLE,
        format!(
            "{what} is {}, which does not round to a nonzero finite binary64",
            value.to_canonical()
        ),
    )
    .on_line(at))
}

fn read_direction(word: &str, at: usize) -> Result<Direction, Rejected> {
    match word {
        "MAX" | "MAXIMIZE" => Ok(Direction::Max),
        "MIN" | "MINIMIZE" => Ok(Direction::Min),
        _ => Err(Rejected::new(
            Code::UNKNOWN_SECTION,
            format!("objective sense {word:?} is not one of MIN, MINIMIZE, MAX, or MAXIMIZE"),
        )
        .on_line(at)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY_FILE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");
    const TINY_DUMP: &str = include_str!("../../../spec/v1/vectors/mps/tiny.canon");
    const RANGES_FILE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/ranges.mps");
    const RANGES_DUMP: &str = include_str!("../../../spec/v1/vectors/mps/ranges.canon");
    const EXACT_FILE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/exact.mps");
    const EXACT_DUMP: &str = include_str!("../../../spec/v1/vectors/mps/exact.canon");

    #[test]
    fn every_accepted_vector_renders_its_frozen_dump() {
        for (name, raw, expected) in [
            ("tiny", TINY_FILE, TINY_DUMP),
            ("ranges", RANGES_FILE, RANGES_DUMP),
            ("exact", EXACT_FILE, EXACT_DUMP),
        ] {
            let instance = match read(raw, Layout::Fixed) {
                Ok(instance) => instance,
                Err(error) => panic!("{name}: {error}"),
            };
            assert_eq!(instance.dump(), expected, "{name}");
        }
    }

    #[test]
    fn there_is_no_lenient_reading_of_the_ambiguous_bound() {
        let source = b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nBOUNDS\n UP bnd x -3\nENDATA\n";
        let refusal = read(source, Layout::Free).expect_err("refused");
        assert_eq!(refusal.code, Code::AMBIGUOUS_NEGATIVE_UPPER);
    }

    #[test]
    fn the_byte_shape_is_checked_before_any_line() {
        for source in [
            b"ROWS\r\n N obj\r\nCOLUMNS\r\nENDATA\r\n".as_slice(),
            b"ROWS\n N ob\xc3\xa9\nCOLUMNS\nENDATA\n".as_slice(),
            b"ROWS\n N obj\0\nCOLUMNS\nENDATA\n".as_slice(),
        ] {
            assert_eq!(
                read(source, Layout::Free).expect_err("refused").code,
                Code::NOT_TEXT
            );
        }
    }

    #[test]
    fn a_name_table_finds_what_it_holds_and_refuses_a_repeat() {
        let mut names = Names::default();
        assert!(names.insert("beta", 1));
        assert!(names.insert("alpha", 0));
        assert!(!names.insert("alpha", 2));
        assert_eq!(names.find("alpha"), Some(0));
        assert_eq!(names.find("beta"), Some(1));
        assert_eq!(names.find("gamma"), None);
    }

    #[test]
    fn the_ranges_table_is_applied_sign_by_sign() {
        let instance = read(RANGES_FILE, Layout::Fixed).expect("accepted");
        let bounds: Vec<(String, String)> = instance
            .rows
            .iter()
            .map(|row| {
                (
                    row.low
                        .as_ref()
                        .map_or_else(|| "*".to_owned(), |v| v.to_canonical()),
                    row.high
                        .as_ref()
                        .map_or_else(|| "*".to_owned(), |v| v.to_canonical()),
                )
            })
            .collect();
        let expected = [
            ("10", "14"),
            ("10", "14"),
            ("6", "10"),
            ("6", "10"),
            ("10", "14"),
            ("6", "10"),
            ("10", "10"),
        ];
        for (actual, wanted) in bounds.iter().zip(expected) {
            assert_eq!((actual.0.as_str(), actual.1.as_str()), wanted);
        }
    }
}
