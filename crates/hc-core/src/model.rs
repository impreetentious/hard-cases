//! The canonical exact model.
//!
//! This is what a certificate is about. Not a model the solver assembled from
//! convenience, and not a floating-point copy of one: a mathematical object
//! determined entirely by the bytes of a file and the rules that read them,
//! carrying the digest of those bytes so that any later claim can be checked
//! against the same file rather than against a description of it.
//!
//! Everything here is exact. The bounds are rationals, the coefficients are
//! rationals, the objective constant is a rational. The floating-point copy
//! the search works on is derived from this and is never the other way round.
//!
//! Two conventions are worth stating where they are implemented rather than
//! only where they are specified. The model always minimizes, with the
//! original sense recorded beside it, because a maximization negated at the
//! parse boundary is otherwise invisible and a sign error in that negation is
//! the classic silent failure. And every finite variable bound is a canonical
//! inequality with an identifier of its own, exactly like a row side, so that
//! a contradiction citing bounds is complete rather than relying on a
//! convention about which bounds a reader may assume.

use hc_exact::Rational;

use crate::ids::{FormatTag, IneqId, RowId, Sense, Side, VarId};

/// One variable of the canonical model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    /// The exact lower bound, absent when the variable is unbounded below.
    pub lower: Option<Rational>,
    /// The exact upper bound, absent when the variable is unbounded above.
    pub upper: Option<Rational>,
    /// Whether the model declares the variable integer.
    pub integral: bool,
    /// The exact objective coefficient, in the minimizing sense.
    pub objective: Rational,
    /// The label the file used. Never cited by a proof.
    pub name: String,
}

impl Variable {
    /// The bounds branching and presolve applicability tests use.
    ///
    /// For an integer variable these are the rounded bounds. The original
    /// rationals remain the axioms a proof cites; these are a search
    /// convenience and never appear in an inference.
    #[must_use]
    pub fn effective_bounds(&self) -> (Option<Rational>, Option<Rational>) {
        if !self.integral {
            return (self.lower.clone(), self.upper.clone());
        }
        (
            self.lower
                .as_ref()
                .map(|value| Rational::from_int(value.ceil())),
            self.upper
                .as_ref()
                .map(|value| Rational::from_int(value.floor())),
        )
    }

    /// Whether the variable's own bounds already exclude every value.
    ///
    /// This is not a parse error. It is an exactly contradictory model, and
    /// producing the contradiction is the presolve chain's job.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let (lower, upper) = self.effective_bounds();
        match (lower, upper) {
            (Some(low), Some(high)) => low > high,
            _ => false,
        }
    }
}

/// One retained row of the canonical model.
///
/// A row is a pair of optional exact bounds. The sense a file wrote it with is
/// gone by this point: an inequality with one absent side and an equality with
/// two equal sides are the same kind of object, and treating them alike is why
/// the range table has one place to be wrong instead of four.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The exact lower bound, absent when the row is unbounded below.
    pub lower: Option<Rational>,
    /// The exact upper bound, absent when the row is unbounded above.
    pub upper: Option<Rational>,
    /// The label the file used.
    pub name: String,
}

impl Row {
    /// Whether the row's two sides cross.
    #[must_use]
    pub fn is_contradictory(&self) -> bool {
        match (&self.lower, &self.upper) {
            (Some(low), Some(high)) => low > high,
            _ => false,
        }
    }
}

/// The constraint matrix, stored by column.
///
/// Compressed columns because the simplex asks for one at a time and because
/// the canonical dump is ordered the same way, so the storage and the thing
/// two parsers compare byte for byte cannot drift apart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matrix {
    /// One entry per column plus a terminator.
    starts: Vec<usize>,
    /// Row indices, ascending within each column.
    rows: Vec<RowId>,
    /// The coefficient beside each row index.
    values: Vec<Rational>,
}

impl Matrix {
    /// Builds the matrix from entries sorted by column and then by row.
    ///
    /// # Panics
    ///
    /// Panics when the entries are unsorted, hold a duplicate, or name a
    /// column past the declared count. Those are internal invariants of the
    /// caller that builds the model, not properties of an input file, so
    /// breaking one is a defect rather than a rejection.
    #[must_use]
    pub fn from_sorted(columns: usize, entries: Vec<(VarId, RowId, Rational)>) -> Self {
        let mut starts = vec![0usize; columns + 1];
        let mut rows = Vec::with_capacity(entries.len());
        let mut values = Vec::with_capacity(entries.len());
        let mut previous: Option<(VarId, RowId)> = None;

        for (column, row, value) in entries {
            assert!(
                column.index() < columns,
                "matrix entry names a column past the count"
            );
            if let Some(before) = previous {
                assert!(
                    (before.0, before.1) < (column, row),
                    "matrix entries are not sorted"
                );
            }
            previous = Some((column, row));
            starts[column.index() + 1] += 1;
            rows.push(row);
            values.push(value);
        }
        for index in 1..starts.len() {
            starts[index] += starts[index - 1];
        }
        Self {
            starts,
            rows,
            values,
        }
    }

    /// The number of columns.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    /// The number of stored coefficients.
    #[must_use]
    pub fn nonzeros(&self) -> usize {
        self.values.len()
    }

    /// One column's row indices and coefficients, in ascending row order.
    pub fn column(&self, variable: VarId) -> impl Iterator<Item = (RowId, &Rational)> {
        let span = self.span(variable);
        self.rows[span.clone()]
            .iter()
            .copied()
            .zip(&self.values[span])
    }

    /// Every stored coefficient, in column order and then row order.
    pub fn entries(&self) -> impl Iterator<Item = (VarId, RowId, &Rational)> {
        (0..self.columns()).flat_map(move |index| {
            let variable = VarId(index as u32);
            self.column(variable)
                .map(move |(row, value)| (variable, row, value))
        })
    }

    /// The coefficient at a position, when one is stored there.
    #[must_use]
    pub fn get(&self, variable: VarId, row: RowId) -> Option<&Rational> {
        let span = self.span(variable);
        let found = self.rows[span.clone()].binary_search(&row).ok()?;
        self.values.get(span.start + found)
    }

    /// The same coefficients grouped by row.
    ///
    /// Row activity is what every feasibility check computes, and computing it
    /// from a column store means touching every column for every row.
    #[must_use]
    pub fn by_row(&self, rows: usize) -> RowMajor {
        let mut counts = vec![0usize; rows + 1];
        for (_, row, _) in self.entries() {
            counts[row.index() + 1] += 1;
        }
        for index in 1..counts.len() {
            counts[index] += counts[index - 1];
        }
        let starts = counts.clone();
        let mut cursor = counts;
        let mut columns = vec![VarId(0); self.values.len()];
        let mut values = vec![Rational::zero(); self.values.len()];
        for (variable, row, value) in self.entries() {
            let at = cursor[row.index()];
            columns[at] = variable;
            values[at] = value.clone();
            cursor[row.index()] += 1;
        }
        RowMajor {
            starts,
            columns,
            values,
        }
    }

    fn span(&self, variable: VarId) -> core::ops::Range<usize> {
        let index = variable.index();
        self.starts[index]..self.starts[index + 1]
    }
}

/// The constraint matrix grouped by row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowMajor {
    starts: Vec<usize>,
    columns: Vec<VarId>,
    values: Vec<Rational>,
}

impl RowMajor {
    /// One row's column indices and coefficients, in ascending column order.
    pub fn row(&self, row: RowId) -> impl Iterator<Item = (VarId, &Rational)> {
        let span = self.starts[row.index()]..self.starts[row.index() + 1];
        self.columns[span.clone()]
            .iter()
            .copied()
            .zip(&self.values[span])
    }

    /// The number of rows.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }
}

/// One canonical inequality, written as a coefficient row against a lower
/// bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inequality {
    /// Whether the inequality comes from a row side or a variable bound.
    pub source: IneqSource,
    /// The side it comes from.
    pub side: Side,
}

/// Where a canonical inequality comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IneqSource {
    /// A retained row's side.
    Row(RowId),
    /// A variable's bound.
    Var(VarId),
}

/// The pieces a parser hands over to build a model.
///
/// A struct rather than a long argument list, because eleven positional
/// arguments of which four are vectors of similar shape is a call site nobody
/// can read and two of which will eventually be swapped.
#[derive(Clone, Debug)]
pub struct ModelParts {
    /// Which rules produced the model.
    pub format: FormatTag,
    /// The digest of the format tag followed by the file's exact bytes.
    pub digest: [u8; 32],
    /// The sense the file was written in.
    pub original_sense: Sense,
    /// The objective constant, in the minimizing sense.
    pub objective_constant: Rational,
    /// The variables, in canonical index order.
    pub variables: Vec<Variable>,
    /// The retained rows, in canonical index order.
    pub rows: Vec<Row>,
    /// The coefficients, sorted by column and then by row.
    pub entries: Vec<(VarId, RowId, Rational)>,
    /// Positions where duplicate entries cancelled exactly.
    pub zero_sums: Vec<(VarId, RowId)>,
    /// Names of free rows discarded after objective selection, in parse order.
    pub discarded: Vec<String>,
}

/// The canonical exact model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    format: FormatTag,
    digest: [u8; 32],
    original_sense: Sense,
    objective_constant: Rational,
    variables: Vec<Variable>,
    rows: Vec<Row>,
    matrix: Matrix,
    zero_sums: Vec<(VarId, RowId)>,
    discarded: Vec<String>,
    inequalities: Vec<Inequality>,
}

impl Model {
    /// Assembles a model and assigns its canonical inequality identifiers.
    ///
    /// # Panics
    ///
    /// Panics when the coefficient entries are unsorted or name a column that
    /// does not exist. Both are defects in the caller rather than properties
    /// of an input.
    #[must_use]
    pub fn new(parts: ModelParts) -> Self {
        let matrix = Matrix::from_sorted(parts.variables.len(), parts.entries);
        // Identifiers are assigned here and nowhere else, so the order is
        // written down once. Rows first in index order, lower side before
        // upper within a ranged row; then variables in index order, lower
        // bound before upper. Only finite sides take an identifier.
        let mut inequalities = Vec::new();
        for (index, row) in parts.rows.iter().enumerate() {
            let id = RowId(index as u32);
            if row.lower.is_some() {
                inequalities.push(Inequality {
                    source: IneqSource::Row(id),
                    side: Side::Lower,
                });
            }
            if row.upper.is_some() {
                inequalities.push(Inequality {
                    source: IneqSource::Row(id),
                    side: Side::Upper,
                });
            }
        }
        for (index, variable) in parts.variables.iter().enumerate() {
            let id = VarId(index as u32);
            if variable.lower.is_some() {
                inequalities.push(Inequality {
                    source: IneqSource::Var(id),
                    side: Side::Lower,
                });
            }
            if variable.upper.is_some() {
                inequalities.push(Inequality {
                    source: IneqSource::Var(id),
                    side: Side::Upper,
                });
            }
        }

        Self {
            format: parts.format,
            digest: parts.digest,
            original_sense: parts.original_sense,
            objective_constant: parts.objective_constant,
            variables: parts.variables,
            rows: parts.rows,
            matrix,
            zero_sums: parts.zero_sums,
            discarded: parts.discarded,
            inequalities,
        }
    }

    /// Which rules produced the model.
    #[must_use]
    pub fn format(&self) -> FormatTag {
        self.format
    }

    /// The digest of the format tag followed by the file's exact bytes.
    #[must_use]
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    /// The sense the file was written in. The model itself always minimizes.
    #[must_use]
    pub fn original_sense(&self) -> Sense {
        self.original_sense
    }

    /// The objective constant, in the minimizing sense.
    #[must_use]
    pub fn objective_constant(&self) -> &Rational {
        &self.objective_constant
    }

    /// The variables, in canonical index order.
    #[must_use]
    pub fn variables(&self) -> &[Variable] {
        &self.variables
    }

    /// The retained rows, in canonical index order.
    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The constraint matrix.
    #[must_use]
    pub fn matrix(&self) -> &Matrix {
        &self.matrix
    }

    /// Positions where duplicate entries cancelled exactly.
    ///
    /// A cancelled entry and an absent entry are the same matrix and different
    /// files, so the cancellation is recorded rather than forgotten and both
    /// parsers have to agree that it happened.
    #[must_use]
    pub fn zero_sums(&self) -> &[(VarId, RowId)] {
        &self.zero_sums
    }

    /// Names of free rows discarded after objective selection.
    #[must_use]
    pub fn discarded(&self) -> &[String] {
        &self.discarded
    }

    /// The canonical inequalities, in identifier order.
    #[must_use]
    pub fn inequalities(&self) -> &[Inequality] {
        &self.inequalities
    }

    /// One canonical inequality.
    #[must_use]
    pub fn inequality(&self, id: IneqId) -> Option<&Inequality> {
        self.inequalities.get(id.index())
    }

    /// The exact objective value at a point, including the constant.
    ///
    /// # Panics
    ///
    /// Panics when the point's length is not the model's variable count.
    #[must_use]
    pub fn objective_at(&self, point: &[Rational]) -> Rational {
        assert_eq!(
            point.len(),
            self.variables.len(),
            "point dimension does not match the model"
        );
        let mut total = self.objective_constant.clone();
        for (variable, value) in self.variables.iter().zip(point) {
            if variable.objective.is_zero() || value.is_zero() {
                continue;
            }
            total = total.add(&variable.objective.mul(value));
        }
        total
    }

    /// The exact activity of one row at a point.
    ///
    /// # Panics
    ///
    /// Panics when the point's length is not the model's variable count.
    #[must_use]
    pub fn row_activity(&self, row: RowId, point: &[Rational]) -> Rational {
        assert_eq!(
            point.len(),
            self.variables.len(),
            "point dimension does not match the model"
        );
        let mut total = Rational::zero();
        for (index, value) in point.iter().enumerate() {
            if value.is_zero() {
                continue;
            }
            if let Some(coefficient) = self.matrix.get(VarId(index as u32), row) {
                total = total.add(&coefficient.mul(value));
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(value: i64) -> Rational {
        Rational::from_i64(value)
    }

    /// A two-variable, two-row model used by several checks below.
    fn sample() -> Model {
        Model::new(ModelParts {
            format: FormatTag::MpsFixed,
            digest: [7; 32],
            original_sense: Sense::Min,
            objective_constant: number(5),
            variables: vec![
                Variable {
                    lower: Some(number(0)),
                    upper: Some(number(4)),
                    integral: true,
                    objective: number(-1),
                    name: "x".to_owned(),
                },
                Variable {
                    lower: Some(number(0)),
                    upper: None,
                    integral: false,
                    objective: number(2),
                    name: "y".to_owned(),
                },
            ],
            rows: vec![
                Row {
                    lower: None,
                    upper: Some(number(10)),
                    name: "r0".to_owned(),
                },
                Row {
                    lower: Some(number(1)),
                    upper: Some(number(3)),
                    name: "r1".to_owned(),
                },
            ],
            entries: vec![
                (VarId(0), RowId(0), number(2)),
                (VarId(0), RowId(1), number(1)),
                (VarId(1), RowId(1), number(3)),
            ],
            zero_sums: vec![(VarId(1), RowId(0))],
            discarded: vec!["free".to_owned()],
        })
    }

    #[test]
    fn identifiers_are_rows_then_variables_and_lower_before_upper() {
        let model = sample();
        let expected = [
            Inequality {
                source: IneqSource::Row(RowId(0)),
                side: Side::Upper,
            },
            Inequality {
                source: IneqSource::Row(RowId(1)),
                side: Side::Lower,
            },
            Inequality {
                source: IneqSource::Row(RowId(1)),
                side: Side::Upper,
            },
            Inequality {
                source: IneqSource::Var(VarId(0)),
                side: Side::Lower,
            },
            Inequality {
                source: IneqSource::Var(VarId(0)),
                side: Side::Upper,
            },
            Inequality {
                source: IneqSource::Var(VarId(1)),
                side: Side::Lower,
            },
        ];
        assert_eq!(model.inequalities(), expected.as_slice());
        assert_eq!(model.inequality(IneqId(0)), Some(&expected[0]));
        assert_eq!(model.inequality(IneqId(6)), None);
    }

    #[test]
    fn an_absent_bound_takes_no_identifier() {
        // The second variable is unbounded above and the first row has no
        // lower side, so neither contributes an axiom a proof could cite.
        let model = sample();
        assert_eq!(model.inequalities().len(), 6);
        assert!(
            !model
                .inequalities()
                .iter()
                .any(|item| item.source == IneqSource::Var(VarId(1)) && item.side == Side::Upper)
        );
    }

    #[test]
    fn the_matrix_stores_columns_and_answers_by_position() {
        let model = sample();
        let matrix = model.matrix();
        assert_eq!(matrix.columns(), 2);
        assert_eq!(matrix.nonzeros(), 3);
        let first: Vec<(RowId, String)> = matrix
            .column(VarId(0))
            .map(|(row, value)| (row, value.to_canonical_string()))
            .collect();
        assert_eq!(
            first,
            vec![(RowId(0), "2".to_owned()), (RowId(1), "1".to_owned())]
        );
        assert_eq!(
            matrix
                .get(VarId(1), RowId(1))
                .map(Rational::to_canonical_string),
            Some("3".to_owned())
        );
        assert_eq!(matrix.get(VarId(1), RowId(0)), None);
    }

    #[test]
    fn the_row_grouped_view_holds_the_same_coefficients() {
        let model = sample();
        let by_row = model.matrix().by_row(model.rows().len());
        assert_eq!(by_row.rows(), 2);
        let second: Vec<(VarId, String)> = by_row
            .row(RowId(1))
            .map(|(var, value)| (var, value.to_canonical_string()))
            .collect();
        assert_eq!(
            second,
            vec![(VarId(0), "1".to_owned()), (VarId(1), "3".to_owned())]
        );
        let first: Vec<VarId> = by_row.row(RowId(0)).map(|(var, _)| var).collect();
        assert_eq!(first, vec![VarId(0)]);
    }

    #[test]
    fn the_objective_and_row_activity_are_exact_and_include_the_constant() {
        let model = sample();
        let point = [
            number(3),
            Rational::new(hc_exact::BigInt::one(), hc_exact::BigInt::from_i64(3)).expect("nonzero"),
        ];
        // 5 + (-1)(3) + 2(1/3) = 2 + 2/3.
        assert_eq!(model.objective_at(&point).to_canonical_string(), "8/3");
        // Row one: 1(3) + 3(1/3) = 4.
        assert_eq!(
            model.row_activity(RowId(1), &point).to_canonical_string(),
            "4"
        );
        assert_eq!(
            model.row_activity(RowId(0), &point).to_canonical_string(),
            "6"
        );
    }

    #[test]
    fn effective_bounds_round_only_for_integer_variables() {
        let half =
            Rational::new(hc_exact::BigInt::one(), hc_exact::BigInt::from_i64(2)).expect("nonzero");
        let continuous = Variable {
            lower: Some(half.clone()),
            upper: Some(Rational::from_i64(3).add(&half)),
            integral: false,
            objective: Rational::zero(),
            name: "c".to_owned(),
        };
        let (low, high) = continuous.effective_bounds();
        assert_eq!(
            low.map(|value| value.to_canonical_string()),
            Some("1/2".to_owned())
        );
        assert_eq!(
            high.map(|value| value.to_canonical_string()),
            Some("7/2".to_owned())
        );

        let integral = Variable {
            integral: true,
            ..continuous
        };
        let (low, high) = integral.effective_bounds();
        assert_eq!(
            low.map(|value| value.to_canonical_string()),
            Some("1".to_owned())
        );
        assert_eq!(
            high.map(|value| value.to_canonical_string()),
            Some("3".to_owned())
        );
        assert!(!integral.is_empty());
    }

    #[test]
    fn a_crossed_bound_pair_is_reported_rather_than_refused() {
        // Neither of these is a parse error. They are exactly contradictory
        // models, and producing the contradiction belongs to presolve.
        let variable = Variable {
            lower: Some(number(3)),
            upper: Some(number(2)),
            integral: false,
            objective: Rational::zero(),
            name: "v".to_owned(),
        };
        assert!(variable.is_empty());
        let row = Row {
            lower: Some(number(3)),
            upper: Some(number(2)),
            name: "r".to_owned(),
        };
        assert!(row.is_contradictory());

        // An integer variable can be empty even when its rational bounds are
        // not crossed, because rounding closes the gap from both sides.
        let third =
            Rational::new(hc_exact::BigInt::one(), hc_exact::BigInt::from_i64(3)).expect("nonzero");
        let narrow = Variable {
            lower: Some(third.clone()),
            upper: Some(third.mul(&Rational::from_i64(2))),
            integral: true,
            objective: Rational::zero(),
            name: "n".to_owned(),
        };
        assert!(narrow.is_empty());
    }

    #[test]
    fn cancelled_entries_and_discarded_rows_are_carried_rather_than_forgotten() {
        let model = sample();
        assert_eq!(model.zero_sums(), &[(VarId(1), RowId(0))]);
        assert_eq!(model.discarded(), &["free".to_owned()]);
        assert_eq!(model.digest(), &[7u8; 32]);
        assert_eq!(model.format(), FormatTag::MpsFixed);
        assert_eq!(model.original_sense(), Sense::Min);
    }

    #[test]
    #[should_panic(expected = "not sorted")]
    fn unsorted_entries_are_a_defect_and_say_so() {
        let _ = Matrix::from_sorted(
            2,
            vec![
                (VarId(1), RowId(0), number(1)),
                (VarId(0), RowId(0), number(1)),
            ],
        );
    }

    #[test]
    fn an_empty_matrix_is_well_formed() {
        let matrix = Matrix::from_sorted(3, Vec::new());
        assert_eq!(matrix.columns(), 3);
        assert_eq!(matrix.nonzeros(), 0);
        assert_eq!(matrix.column(VarId(1)).count(), 0);
        assert_eq!(matrix.by_row(2).rows(), 2);
    }
}
