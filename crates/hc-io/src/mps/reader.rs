//! The reader: bytes in, canonical exact model out.
//!
//! Every disagreement this format is famous for is settled in the normative
//! assets, and this module's job is to apply those settlements without adding
//! any of its own. Where two readings are both defensible, the file is refused
//! rather than read one way and solved: the cost is instance coverage, and the
//! alternative is proving a theorem about a model the author did not write.
//!
//! The order of work is fixed by the format. Rows are declared before the
//! coefficients that reference them, coefficients before the right-hand sides,
//! right-hand sides before the ranges that widen them, and bounds last. That
//! ordering is why this can be a single pass with no name resolution
//! afterwards, and it is why an out-of-order section is refused instead of
//! buffered.

use std::collections::BTreeMap;

use hc_core::{
    FormatTag, Model, ModelParts, Refusal, Rejection, Row, RowId, Sense, VarId, Variable,
    instance_digest,
};
use hc_exact::Rational;

use crate::mps::numeric::Limits;
use crate::mps::record::{Fields, Form, Line, Record, Shape, classify};
use crate::mps::section::{Section, Sequence, read_sense};

/// How to read a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Which variant's rules apply.
    pub form: Form,
    /// Whether to refuse the readings the format leaves ambiguous.
    ///
    /// Strict is the default and is what a certificate requires. The lenient
    /// mode exists so that a file outside the subset can still be looked at,
    /// and every thing it forgives is recorded by name — a lenient parse may
    /// be solved, and it may not carry a claim unless the verifier's strict
    /// parser independently accepts the same bytes.
    pub strict: bool,
    /// The caps and fences.
    pub limits: Limits,
}

impl Options {
    /// Strict reading under a variant's rules.
    #[must_use]
    pub const fn strict(form: Form) -> Self {
        Self {
            form,
            strict: true,
            limits: Limits::SPEC,
        }
    }

    /// Lenient reading under a variant's rules.
    #[must_use]
    pub const fn lenient(form: Form) -> Self {
        Self {
            form,
            strict: false,
            limits: Limits::SPEC,
        }
    }

    /// The format tag the digest is taken under.
    #[must_use]
    pub const fn tag(self) -> FormatTag {
        match self.form {
            Form::Fixed => FormatTag::MpsFixed,
            Form::Free => FormatTag::MpsFree,
        }
    }
}

/// A model, and whatever a lenient parse had to forgive to produce it.
#[derive(Clone, Debug)]
pub struct Parsed {
    /// The canonical exact model.
    pub model: Model,
    /// One entry per ambiguity a lenient parse resolved rather than refused.
    ///
    /// Empty after a strict parse, always. A non-empty list is what makes a
    /// model ineligible to carry a claim on its own.
    pub forgiven: Vec<String>,
}

/// Reads a file strictly under a variant's rules.
///
/// # Errors
///
/// Returns a refusal naming the rule that refused the file.
pub fn read(raw: &[u8], form: Form) -> Result<Model, Refusal> {
    read_with(raw, Options::strict(form)).map(|parsed| parsed.model)
}

/// Reads a file under explicit options.
///
/// # Errors
///
/// Returns a refusal naming the rule that refused the file.
pub fn read_with(raw: &[u8], options: Options) -> Result<Parsed, Refusal> {
    options.limits.admit_file(raw.len())?;
    let text = admit_text(raw)?;
    let digest = instance_digest(options.tag().byte(), raw);
    Builder::new(options, digest).run(text)
}

/// Refuses input that is not ASCII text with line-feed endings.
///
/// The restriction is derived rather than chosen. The canonical dump is ASCII
/// and writes names into it verbatim, so a name that cannot appear in the dump
/// cannot appear in a model. And a file with carriage returns is a different
/// byte sequence, hence a different instance under the digest; accepting both
/// and normalizing would make two digests produce one model.
fn admit_text(raw: &[u8]) -> Result<&str, Refusal> {
    for (offset, &byte) in raw.iter().enumerate() {
        let acceptable = byte == b'\n' || byte == b'\t' || (b' '..=b'~').contains(&byte);
        if !acceptable {
            return Err(Refusal::new(
                Rejection::INSTANCE_NOT_TEXT,
                format!("byte {byte:#04x} at offset {offset} is not accepted input"),
            ));
        }
    }
    // Every byte is ASCII, so this cannot fail; the conversion is written as a
    // fallible one anyway rather than through an unchecked call.
    core::str::from_utf8(raw).map_err(|error| {
        Refusal::new(
            Rejection::INSTANCE_NOT_TEXT,
            format!("input is not text: {error}"),
        )
    })
}

/// How a row was declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowSense {
    Free,
    Less,
    Greater,
    Equal,
}

/// A declared row, with whatever has been said about it so far.
#[derive(Clone, Debug)]
struct RowDecl {
    name: String,
    sense: RowSense,
    /// The row's position in the canonical model, absent for a free row.
    retained: Option<usize>,
    right_hand_side: Option<Rational>,
    range: Option<Rational>,
}

/// A declared column, with whatever has been said about it so far.
#[derive(Clone, Debug)]
struct ColumnDecl {
    name: String,
    integral: bool,
    objective: Rational,
    lower: Option<Rational>,
    upper: Option<Rational>,
    /// Whether the lower bound is still the default of zero.
    lower_is_default: bool,
    /// Bound codes already applied, so a repeat of one can be refused.
    applied: Vec<&'static str>,
}

/// The reader's state.
struct Builder {
    options: Options,
    digest: [u8; 32],
    sequence: Sequence,
    forgiven: Vec<String>,

    rows: Vec<RowDecl>,
    row_index: BTreeMap<String, usize>,
    objective_row: Option<usize>,
    retained_rows: usize,
    discarded: Vec<String>,

    columns: Vec<ColumnDecl>,
    column_index: BTreeMap<String, usize>,
    current_column: Option<usize>,

    /// Accumulated coefficients, with how many records contributed to each.
    entries: BTreeMap<(usize, usize), (Rational, usize)>,
    stored: usize,

    objective_constant: Rational,
    original_sense: Sense,

    /// The one vector name each of these sections is allowed to carry.
    rhs_vector: Option<String>,
    ranges_vector: Option<String>,
    bounds_vector: Option<String>,

    /// Marker depth, which may only ever be zero or one.
    inside_markers: bool,
}

impl Builder {
    fn new(options: Options, digest: [u8; 32]) -> Self {
        Self {
            options,
            digest,
            sequence: Sequence::new(),
            forgiven: Vec::new(),
            rows: Vec::new(),
            row_index: BTreeMap::new(),
            objective_row: None,
            retained_rows: 0,
            discarded: Vec::new(),
            columns: Vec::new(),
            column_index: BTreeMap::new(),
            current_column: None,
            entries: BTreeMap::new(),
            stored: 0,
            objective_constant: Rational::zero(),
            original_sense: Sense::Min,
            rhs_vector: None,
            ranges_vector: None,
            bounds_vector: None,
            inside_markers: false,
        }
    }

    fn run(mut self, text: &str) -> Result<Parsed, Refusal> {
        let mut section: Option<Section> = None;
        let mut awaiting_sense = false;

        for (offset, raw_line) in text.split('\n').enumerate() {
            let number = offset + 1;
            // Content after the terminator is ignored for parsing. It is still
            // inside the digest, so it can never be used to smuggle in a
            // second model.
            if section == Some(Section::EndData) {
                continue;
            }
            match classify(self.options.form, raw_line, number)? {
                Line::Ignorable => {}
                Line::Section { name, tail } => {
                    let opened = self.sequence.admit(name, number)?;
                    if opened == Section::ObjSense && !tail.is_empty() {
                        // The free form may write the sense on the header
                        // line. The fixed form may not, because the tail would
                        // be content past a window on a line that is not a
                        // record.
                        if self.options.form == Form::Fixed {
                            return Err(Refusal::new(
                                Rejection::INSTANCE_UNKNOWN_SECTION,
                                "the fixed form writes the objective sense on its own record"
                                    .to_owned(),
                            )
                            .at(number));
                        }
                        self.original_sense = read_sense(tail, number)?;
                        awaiting_sense = false;
                    } else {
                        awaiting_sense = opened == Section::ObjSense;
                    }
                    section = Some(opened);
                }
                Line::Record(record) => {
                    let Some(open) = section else {
                        return Err(Refusal::new(
                            Rejection::INSTANCE_SECTION_MISSING,
                            "a record appears before any section header".to_owned(),
                        )
                        .at(number));
                    };
                    self.record(open, &record, &mut awaiting_sense)?;
                }
            }
        }

        self.sequence.admit_end()?;
        self.finish()
    }

    fn record(
        &mut self,
        section: Section,
        record: &Record<'_>,
        awaiting_sense: &mut bool,
    ) -> Result<(), Refusal> {
        match section {
            // The instance name carries no model content, and neither does a
            // record inside it.
            Section::Name | Section::EndData => Ok(()),
            Section::ObjSense => {
                let fields = record.fields(Shape::Indicated)?;
                let word = if fields.indicator.is_empty() {
                    fields.name1
                } else {
                    fields.indicator
                };
                self.original_sense = read_sense(word, record.line())?;
                *awaiting_sense = false;
                Ok(())
            }
            Section::Rows => self.row_record(record),
            Section::Columns => self.column_record(record),
            Section::Rhs => self.right_hand_side_record(record),
            Section::Ranges => self.range_record(record),
            Section::Bounds => self.bound_record(record),
        }
    }

    fn row_record(&mut self, record: &Record<'_>) -> Result<(), Refusal> {
        let fields = record.fields(Shape::Indicated)?;
        let line = record.line();
        let sense = match fields.indicator {
            "N" => RowSense::Free,
            "L" => RowSense::Less,
            "G" => RowSense::Greater,
            "E" => RowSense::Equal,
            other => {
                return Err(Refusal::new(
                    Rejection::INSTANCE_UNKNOWN_SECTION,
                    format!("row sense {other:?} is not one of N, L, G, or E"),
                )
                .at(line));
            }
        };
        let name = fields.name1;
        self.options.limits.admit_name(name, line)?;
        if self.row_index.contains_key(name) {
            return Err(Refusal::new(
                Rejection::INSTANCE_DUPLICATE_ROW,
                format!("row {name:?} is declared twice"),
            )
            .at(line));
        }

        let position = self.rows.len();
        let retained = if sense == RowSense::Free {
            // The first free row is the objective. Every later one is
            // discarded, and the discarding is recorded so both parsers agree
            // it happened.
            if self.objective_row.is_none() {
                self.objective_row = Some(position);
            } else {
                self.discarded.push(name.to_owned());
            }
            None
        } else {
            let index = self.retained_rows;
            self.retained_rows += 1;
            Some(index)
        };

        self.row_index.insert(name.to_owned(), position);
        self.rows.push(RowDecl {
            name: name.to_owned(),
            sense,
            retained,
            right_hand_side: None,
            range: None,
        });
        Ok(())
    }

    fn column_record(&mut self, record: &Record<'_>) -> Result<(), Refusal> {
        let fields = record.fields(Shape::Plain)?;
        let line = record.line();

        if let Some(marker) = self.marker(record, &fields)? {
            return self.apply_marker(marker, line);
        }

        let name = fields.name1;
        self.options.limits.admit_name(name, line)?;
        let column = match self.column_index.get(name) {
            Some(&existing) => {
                // A column resumed after another has started is refused.
                // Resuming and restarting are both defensible readings and
                // they build different matrices.
                if self.current_column != Some(existing) {
                    return Err(Refusal::new(
                        Rejection::INSTANCE_COLUMN_SPLIT,
                        format!("column {name:?} resumes after another column started"),
                    )
                    .at(line));
                }
                existing
            }
            None => {
                let index = self.columns.len();
                self.column_index.insert(name.to_owned(), index);
                self.columns.push(ColumnDecl {
                    name: name.to_owned(),
                    integral: self.inside_markers,
                    objective: Rational::zero(),
                    lower: Some(Rational::zero()),
                    upper: None,
                    lower_is_default: true,
                    applied: Vec::new(),
                });
                index
            }
        };
        self.current_column = Some(column);

        self.coefficient(column, fields.name2, fields.value1, line)?;
        if !fields.name3.is_empty() || !fields.value2.is_empty() {
            self.coefficient(column, fields.name3, fields.value2, line)?;
        }
        Ok(())
    }

    /// The marker code a record carries, when it is a marker record.
    ///
    /// The two forms are recognized separately and precisely. In the fixed
    /// form the literal sits in the third window and the code in the fifth. In
    /// the free form a marker record is exactly three fields: a label, the
    /// literal, and the code. Anything else in those positions is not a marker
    /// record and is read as an ordinary entry, which is what the format says.
    fn marker(
        &self,
        record: &Record<'_>,
        fields: &Fields<'_>,
    ) -> Result<Option<&'static str>, Refusal> {
        let (literal, code) = match self.options.form {
            Form::Fixed => (unquote(fields.name2), unquote(fields.name3)),
            Form::Free => {
                if record.arity() != 3 {
                    return Ok(None);
                }
                (unquote(fields.name2), unquote(fields.value1))
            }
        };
        if literal != "MARKER" {
            return Ok(None);
        }
        match code {
            "INTORG" => Ok(Some("INTORG")),
            "INTEND" => Ok(Some("INTEND")),
            _ => Ok(None),
        }
    }

    fn apply_marker(&mut self, code: &str, line: usize) -> Result<(), Refusal> {
        let opening = code == "INTORG";
        if opening == self.inside_markers {
            return Err(Refusal::new(
                Rejection::INSTANCE_MARKER_BALANCE,
                if opening {
                    "an integrality marker opens inside another".to_owned()
                } else {
                    "an integrality marker closes without an opening one".to_owned()
                },
            )
            .at(line));
        }
        self.inside_markers = opening;
        Ok(())
    }

    fn coefficient(
        &mut self,
        column: usize,
        row_name: &str,
        value_text: &str,
        line: usize,
    ) -> Result<(), Refusal> {
        if row_name.is_empty() && value_text.is_empty() {
            return Ok(());
        }
        let row = self.row(row_name, line)?;
        let value = self.options.limits.read_number(value_text, line)?;
        Limits::admit_value(&value, "a coefficient", line)?;

        if Some(row) == self.objective_row {
            let existing =
                core::mem::replace(&mut self.columns[column].objective, Rational::zero());
            self.columns[column].objective = existing.add(&value);
            return Ok(());
        }
        let Some(retained) = self.rows[row].retained else {
            // A discarded free row still absorbs its coefficients; they simply
            // go nowhere, exactly as the row does.
            return Ok(());
        };

        match self.entries.get_mut(&(column, retained)) {
            Some(slot) => {
                slot.0 = slot.0.add(&value);
                slot.1 += 1;
            }
            None => {
                self.options.limits.admit_nonzero(self.stored, line)?;
                self.stored += 1;
                self.entries.insert((column, retained), (value, 1));
            }
        }
        Ok(())
    }

    fn right_hand_side_record(&mut self, record: &Record<'_>) -> Result<(), Refusal> {
        let fields = record.fields(Shape::Plain)?;
        let line = record.line();
        self.admit_vector_name(fields.name1, line, VectorKind::Rhs)?;
        self.right_hand_side(fields.name2, fields.value1, line)?;
        if !fields.name3.is_empty() || !fields.value2.is_empty() {
            self.right_hand_side(fields.name3, fields.value2, line)?;
        }
        Ok(())
    }

    fn right_hand_side(
        &mut self,
        row_name: &str,
        value_text: &str,
        line: usize,
    ) -> Result<(), Refusal> {
        if row_name.is_empty() && value_text.is_empty() {
            return Ok(());
        }
        let row = self.row(row_name, line)?;
        let value = self.options.limits.read_number(value_text, line)?;
        Limits::admit_value(&value, "a right-hand side", line)?;

        if Some(row) == self.objective_row {
            // A right-hand side entry on the objective row supplies the
            // negation of the objective constant. This is the convention the
            // major readers share and the one most likely to shift every
            // reported objective by a constant with nothing looking wrong. A
            // second entry replaces the first.
            self.objective_constant = value.neg();
            return Ok(());
        }
        self.rows[row].right_hand_side = Some(value);
        Ok(())
    }

    fn range_record(&mut self, record: &Record<'_>) -> Result<(), Refusal> {
        let fields = record.fields(Shape::Plain)?;
        let line = record.line();
        self.admit_vector_name(fields.name1, line, VectorKind::Ranges)?;
        self.range(fields.name2, fields.value1, line)?;
        if !fields.name3.is_empty() || !fields.value2.is_empty() {
            self.range(fields.name3, fields.value2, line)?;
        }
        Ok(())
    }

    fn range(&mut self, row_name: &str, value_text: &str, line: usize) -> Result<(), Refusal> {
        if row_name.is_empty() && value_text.is_empty() {
            return Ok(());
        }
        let row = self.row(row_name, line)?;
        if self.rows[row].sense == RowSense::Free {
            return Err(Refusal::new(
                Rejection::INSTANCE_RANGE_ON_FREE_ROW,
                format!("row {row_name:?} has no sense to range"),
            )
            .at(line));
        }
        let value = self.options.limits.read_number(value_text, line)?;
        Limits::admit_value(&value, "a range", line)?;
        self.rows[row].range = Some(value);
        Ok(())
    }

    fn bound_record(&mut self, record: &Record<'_>) -> Result<(), Refusal> {
        let fields = record.fields(Shape::Indicated)?;
        let line = record.line();
        self.admit_vector_name(fields.name1, line, VectorKind::Bounds)?;

        let code = fields.indicator;
        let accepted = matches!(
            code,
            "LO" | "UP" | "FX" | "FR" | "MI" | "PL" | "BV" | "LI" | "UI"
        );
        if !accepted {
            return Err(Refusal::new(
                Rejection::INSTANCE_UNKNOWN_BOUND,
                format!("bound type {code:?} is not accepted"),
            )
            .at(line));
        }

        let column_name = fields.name2;
        let Some(&column) = self.column_index.get(column_name) else {
            return Err(Refusal::new(
                Rejection::INSTANCE_UNKNOWN_COLUMN,
                format!("bound names column {column_name:?}, which was never declared"),
            )
            .at(line));
        };

        // A repeat of one code on one column is refused; two different codes
        // are the ordinary way to give both sides.
        let already = self.columns[column].applied.contains(&code);
        if already {
            return Err(Refusal::new(
                Rejection::INSTANCE_DUPLICATE_BOUND,
                format!("column {column_name:?} already has a {code} bound"),
            )
            .at(line));
        }
        let stored: &'static str = match code {
            "LO" => "LO",
            "UP" => "UP",
            "FX" => "FX",
            "FR" => "FR",
            "MI" => "MI",
            "PL" => "PL",
            "BV" => "BV",
            "LI" => "LI",
            _ => "UI",
        };
        self.columns[column].applied.push(stored);

        let needs_value = matches!(code, "LO" | "UP" | "FX" | "LI" | "UI");
        let value = if needs_value {
            let value = self.options.limits.read_number(fields.value1, line)?;
            Limits::admit_value(&value, "a bound", line)?;
            Some(value)
        } else {
            None
        };

        self.apply_bound(column, code, value, line)
    }

    fn apply_bound(
        &mut self,
        column: usize,
        code: &str,
        value: Option<Rational>,
        line: usize,
    ) -> Result<(), Refusal> {
        match code {
            "LO" => {
                self.columns[column].lower = value;
                self.columns[column].lower_is_default = false;
            }
            "UP" => {
                let value = value.expect("UP carries a value");
                if value.is_negative() && self.columns[column].lower_is_default {
                    // Readers disagree about whether this implies a lower
                    // bound of negative infinity, and about whether it does so
                    // only for integer columns. Both readings are common and
                    // they produce different models.
                    if self.options.strict {
                        return Err(Refusal::new(
                            Rejection::INSTANCE_AMBIGUOUS_NEGATIVE_UPPER,
                            format!(
                                "column {:?} takes a negative upper bound while still at the default lower bound",
                                self.columns[column].name
                            ),
                        )
                        .at(line));
                    }
                    self.forgiven.push(format!(
                        "line {line}: negative upper bound on column {:?} read as removing the lower bound",
                        self.columns[column].name
                    ));
                    self.columns[column].lower = None;
                    self.columns[column].lower_is_default = false;
                }
                self.columns[column].upper = Some(value);
            }
            "FX" => {
                self.columns[column].lower = value.clone();
                self.columns[column].upper = value;
                self.columns[column].lower_is_default = false;
            }
            "FR" => {
                self.columns[column].lower = None;
                self.columns[column].upper = None;
                self.columns[column].lower_is_default = false;
            }
            "MI" => {
                // The historical reading that also zeroes the upper bound is
                // refused elsewhere by simply not being implemented here: it
                // turns an unbounded-below variable into a nonpositive one and
                // has no modern users.
                self.columns[column].lower = None;
                self.columns[column].lower_is_default = false;
            }
            "PL" => self.columns[column].upper = None,
            "BV" => {
                self.columns[column].lower = Some(Rational::zero());
                self.columns[column].upper = Some(Rational::one());
                self.columns[column].integral = true;
                self.columns[column].lower_is_default = false;
            }
            "LI" | "UI" => {
                let value = value.expect("an integer bound carries a value");
                if !value.is_integer() {
                    return Err(Refusal::new(
                        Rejection::INSTANCE_FRACTIONAL_INTEGER_BOUND,
                        format!(
                            "{code} bound on column {:?} is {}, which is not an exact integer",
                            self.columns[column].name,
                            value.to_canonical_string()
                        ),
                    )
                    .at(line));
                }
                if code == "LI" {
                    self.columns[column].lower = Some(value);
                    self.columns[column].lower_is_default = false;
                } else {
                    self.columns[column].upper = Some(value);
                }
                self.columns[column].integral = true;
            }
            _ => unreachable!("the bound code was checked against the accepted list"),
        }
        Ok(())
    }

    fn admit_vector_name(
        &mut self,
        name: &str,
        line: usize,
        kind: VectorKind,
    ) -> Result<(), Refusal> {
        self.options.limits.admit_name(name, line)?;
        let slot = match kind {
            VectorKind::Rhs => &mut self.rhs_vector,
            VectorKind::Ranges => &mut self.ranges_vector,
            VectorKind::Bounds => &mut self.bounds_vector,
        };
        match slot {
            Some(existing) if existing == name => Ok(()),
            Some(existing) => Err(Refusal::new(
                Rejection::INSTANCE_MULTIPLE_VECTORS,
                format!(
                    "{} carries vector {name:?} after vector {existing:?}",
                    kind.as_str()
                ),
            )
            .at(line)),
            None => {
                *slot = Some(name.to_owned());
                Ok(())
            }
        }
    }

    fn row(&self, name: &str, line: usize) -> Result<usize, Refusal> {
        self.row_index.get(name).copied().ok_or_else(|| {
            Refusal::new(
                Rejection::INSTANCE_UNKNOWN_ROW,
                format!("record names row {name:?}, which was never declared"),
            )
            .at(line)
        })
    }

    fn finish(self) -> Result<Parsed, Refusal> {
        if self.objective_row.is_none() {
            // Defaulting to a zero objective would answer a feasibility
            // question while appearing to answer an optimization one.
            return Err(Refusal::new(
                Rejection::INSTANCE_NO_OBJECTIVE,
                "no free row was declared to serve as the objective".to_owned(),
            ));
        }
        if self.inside_markers {
            return Err(Refusal::new(
                Rejection::INSTANCE_MARKER_BALANCE,
                "an integrality marker opens and never closes".to_owned(),
            ));
        }

        let negate = self.original_sense == Sense::Max;
        let variables = self
            .columns
            .iter()
            .map(|column| Variable {
                lower: column.lower.clone(),
                upper: column.upper.clone(),
                integral: column.integral,
                objective: if negate {
                    column.objective.neg()
                } else {
                    column.objective.clone()
                },
                name: column.name.clone(),
            })
            .collect();

        let mut rows = Vec::with_capacity(self.retained_rows);
        for declaration in &self.rows {
            if declaration.retained.is_none() {
                continue;
            }
            let (lower, upper) = row_bounds(declaration);
            rows.push(Row {
                lower,
                upper,
                name: declaration.name.clone(),
            });
        }

        let mut entries = Vec::new();
        let mut zero_sums = Vec::new();
        for ((column, row), (value, contributions)) in self.entries {
            let position = (VarId(column as u32), RowId(row as u32));
            if value.is_zero() {
                // A cancellation and an absence are the same matrix and
                // different files. A single explicit zero carries no
                // information; a pair that cancelled does.
                if contributions > 1 {
                    zero_sums.push(position);
                }
                continue;
            }
            entries.push((position.0, position.1, value));
        }

        let objective_constant = if negate {
            self.objective_constant.neg()
        } else {
            self.objective_constant
        };

        Ok(Parsed {
            model: Model::new(ModelParts {
                format: self.options.tag(),
                digest: self.digest,
                original_sense: self.original_sense,
                objective_constant,
                variables,
                rows,
                entries,
                zero_sums,
                discarded: self.discarded,
            }),
            forgiven: self.forgiven,
        })
    }
}

/// Which section a vector name came from.
#[derive(Clone, Copy, Debug)]
enum VectorKind {
    Rhs,
    Ranges,
    Bounds,
}

impl VectorKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Rhs => "RHS",
            Self::Ranges => "RANGES",
            Self::Bounds => "BOUNDS",
        }
    }
}

/// The exact bounds a row's sense, right-hand side, and range produce.
///
/// This is the single most consequential disagreement between readers of the
/// format. The sign of the range matters for an equality row and does not
/// matter for an inequality row, and a reader that has it backwards produces a
/// model that is feasible, solvable, wrong, and quiet about it.
fn row_bounds(declaration: &RowDecl) -> (Option<Rational>, Option<Rational>) {
    let base = declaration
        .right_hand_side
        .clone()
        .unwrap_or_else(Rational::zero);
    let Some(range) = declaration.range.clone() else {
        return match declaration.sense {
            RowSense::Free => (None, None),
            RowSense::Less => (None, Some(base)),
            RowSense::Greater => (Some(base), None),
            RowSense::Equal => (Some(base.clone()), Some(base)),
        };
    };
    let magnitude = range.abs();
    match declaration.sense {
        RowSense::Free => (None, None),
        RowSense::Greater => (Some(base.clone()), Some(base.add(&magnitude))),
        RowSense::Less => (Some(base.sub(&magnitude)), Some(base)),
        RowSense::Equal => {
            if range.is_negative() {
                (Some(base.add(&range)), Some(base))
            } else {
                // A zero range on an equality row is a no-op rather than an
                // error. It says nothing and is permitted to say nothing.
                (Some(base.clone()), Some(base.add(&range)))
            }
        }
    }
}

/// Strips one surrounding pair of apostrophes, if present.
fn unquote(text: &str) -> &str {
    text.strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .unwrap_or(text)
}
