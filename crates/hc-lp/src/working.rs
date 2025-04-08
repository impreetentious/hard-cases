//! The floating-point working model.
//!
//! The canonical model is exact and is what claims are about. This is the copy
//! the search moves around in, and it exists because a simplex over rationals
//! is not a simplex, it is a very slow proof. Everything here is derived from
//! the exact model and nothing flows back: a bound the search discovers is a
//! place to look, and becomes a claim only after exact machinery elsewhere
//! rechecks it against the original.
//!
//! The shape is the one a bounded-variable revised simplex wants. Every row
//! gets a logical variable of its own, so the constraint system is
//! `A x - s = 0` with the row's own bounds sitting on `s`. That turns a system
//! of two-sided rows into a square system with all the bounds on variables,
//! which is why the basis is always exactly as large as the row count and why
//! a ranged row needs no special case anywhere above this module.
//!
//! Logical columns are implicit. Storing an identity block would double the
//! coefficient count for no information, so a column index past the structural
//! count is answered by construction rather than by lookup.

use hc_core::{Model, RowId, VarId};
use hc_exact::Rational;

/// The value an absent bound takes in the working copy.
///
/// The exact model has no infinities and says so — an absent bound is an
/// absence, not a large number. Here an infinity is the honest encoding,
/// because every comparison the search performs against a bound wants exactly
/// the behaviour an infinity gives it, and the exact side never sees one.
pub const NO_BOUND: f64 = f64::INFINITY;

/// The floating-point copy of a model.
/// One row added to a working copy.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtraRow {
    /// The terms, over the model's variables.
    pub terms: Vec<(usize, f64)>,
    /// The lower side, or negative infinity.
    pub low: f64,
    /// The upper side, or infinity.
    pub high: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Working {
    row_count: usize,
    structural: usize,
    starts: Vec<usize>,
    indices: Vec<u32>,
    values: Vec<f64>,
    lower: Vec<f64>,
    upper: Vec<f64>,
    cost: Vec<f64>,
    integral: Vec<bool>,
    constant: f64,
    digest: [u8; 32],
}

/// Which kind of column an index names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A column of the original model.
    Structural(VarId),
    /// The logical variable standing for a row's activity.
    Logical(RowId),
}

impl Working {
    /// Derives the working copy from an exact model.
    #[must_use]
    pub fn from_model(model: &Model) -> Self {
        let structural = model.variables().len();
        let row_count = model.rows().len();
        let matrix = model.matrix();

        let mut starts = Vec::with_capacity(structural + 1);
        let mut indices = Vec::with_capacity(matrix.nonzeros());
        let mut values = Vec::with_capacity(matrix.nonzeros());
        for index in 0..structural {
            starts.push(indices.len());
            for (row, value) in matrix.column(VarId(index as u32)) {
                indices.push(row.0);
                values.push(value.to_binary64());
            }
        }
        starts.push(indices.len());

        let total = structural + row_count;
        let mut lower = Vec::with_capacity(total);
        let mut upper = Vec::with_capacity(total);
        let mut cost = Vec::with_capacity(total);
        let mut integral = Vec::with_capacity(total);

        for variable in model.variables() {
            lower.push(bound(variable.lower.as_ref(), -NO_BOUND));
            upper.push(bound(variable.upper.as_ref(), NO_BOUND));
            cost.push(variable.objective.to_binary64());
            integral.push(variable.integral);
        }
        for row in model.rows() {
            lower.push(bound(row.lower.as_ref(), -NO_BOUND));
            upper.push(bound(row.upper.as_ref(), NO_BOUND));
            cost.push(0.0);
            // A logical variable is the row's activity. Declaring it integer
            // would be a claim about the row that the model never made.
            integral.push(false);
        }

        Self {
            row_count,
            structural,
            starts,
            indices,
            values,
            lower,
            upper,
            cost,
            integral,
            constant: model.objective_constant().to_binary64(),
            digest: *model.digest(),
        }
    }

    /// A copy with some columns' bounds narrowed.
    ///
    /// This is how a search node differs from its parent. A branch does not
    /// change the model — the model is the theorem — it changes the region a
    /// particular subproblem is allowed to look in, and the honest way to
    /// express that is a working copy with tighter bounds and everything else
    /// identical.
    ///
    /// Narrowing only. A request that would widen a bound is ignored rather
    /// than applied, because a node that could relax its parent's assumptions
    /// would break the one property the tree's correctness rests on: that the
    /// children partition the parent.
    ///
    /// The digest is carried through unchanged. It names the file the model
    /// came from, and a subproblem of that model is still about that file.
    #[must_use]
    pub fn narrowed(&self, bounds: &[(usize, f64, f64)]) -> Self {
        let mut copy = self.clone();
        for (column, low, high) in bounds {
            if *column >= copy.lower.len() {
                continue;
            }
            if *low > copy.lower[*column] {
                copy.lower[*column] = *low;
            }
            if *high < copy.upper[*column] {
                copy.upper[*column] = *high;
            }
        }
        copy
    }

    /// A copy that keeps this region and answers a different question about it.
    ///
    /// Only the objective changes. Every bound, every row and every column is
    /// the one it already was, so a point optimal for the substituted objective
    /// is a point of this same region — which is the only property a caller is
    /// entitled to rely on here.
    ///
    /// The feasibility pump is what wants this. It walks by asking the
    /// relaxation for the point nearest an integral target, which is a distance
    /// question rather than the model's own, and the answer it gets back
    /// establishes nothing at all: it is a guess, and it reaches an incumbent
    /// only through the same exact screening every other candidate passes. A
    /// substituted objective is therefore safe in the one way that matters, and
    /// it would be unsafe the moment anything read an optimal *value* off it.
    /// Nothing does.
    ///
    /// A `costs` slice shorter than the columns leaves the rest at zero, and a
    /// longer one is truncated; neither is an error, because the caller is
    /// building a vector over the structural columns and the logicals have no
    /// cost to give.
    ///
    /// The digest is carried through unchanged, for the same reason
    /// [`narrowed`](Self::narrowed) carries it: it names the file the model came
    /// from, and asking a different question about that model does not make it a
    /// different file.
    #[must_use]
    pub fn with_costs(&self, costs: &[f64]) -> Self {
        let mut copy = self.clone();
        for slot in &mut copy.cost {
            *slot = 0.0;
        }
        for (slot, value) in copy.cost.iter_mut().zip(costs) {
            *slot = *value;
        }
        copy
    }

    /// One extra row: its terms over the model's variables, and its two sides.
    ///
    /// Named because the shape appears in three places and a tuple of a vector
    /// and two floats is the kind of thing that gets passed in the wrong order
    /// exactly once.
    #[must_use]
    pub fn extra_row(terms: Vec<(usize, f64)>, low: f64, high: f64) -> ExtraRow {
        ExtraRow { terms, low, high }
    }

    /// A copy with extra rows appended.
    ///
    /// A cut is a row the model does not have. Adding it to the working copy
    /// is how a relaxation is tightened; it is emphatically not how the model
    /// changes, and the identifiers a proof cites are still the model's. The
    /// extra rows go on the end so that nothing already numbered moves.
    #[must_use]
    pub fn with_rows(&self, extra: &[ExtraRow]) -> Self {
        let mut copy = self.clone();
        for ExtraRow { terms, low, high } in extra {
            let row = copy.row_count;
            copy.row_count += 1;
            // The logical column standing for the new row's activity goes on
            // the end of the columns, which is where every logical lives.
            copy.lower.push(*low);
            copy.upper.push(*high);
            copy.cost.push(0.0);
            copy.integral.push(false);
            for (column, value) in terms {
                if *column >= copy.structural || *value == 0.0 {
                    continue;
                }
                let at = copy.starts[*column + 1];
                copy.indices.insert(at, row as u32);
                copy.values.insert(at, *value);
                for start in copy.starts.iter_mut().skip(*column + 1) {
                    *start += 1;
                }
            }
        }
        copy
    }

    /// The number of rows, which is also the size of every basis.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.row_count
    }

    /// The number of columns from the original model.
    #[must_use]
    pub fn structural(&self) -> usize {
        self.structural
    }

    /// The total number of columns, structural and logical.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.structural + self.row_count
    }

    /// The number of stored coefficients, logical columns excluded.
    #[must_use]
    pub fn nonzeros(&self) -> usize {
        self.values.len()
    }

    /// The digest of the instance this copy was derived from.
    ///
    /// Carried so that a report or a trace can name the file rather than
    /// describing the model, and so a working copy can never be quietly paired
    /// with the wrong certificate.
    #[must_use]
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    /// Which kind of column an index names.
    ///
    /// # Panics
    ///
    /// Panics when the index is past the column count.
    #[must_use]
    pub fn kind(&self, column: usize) -> Kind {
        assert!(column < self.columns(), "column index is past the model");
        if column < self.structural {
            Kind::Structural(VarId(column as u32))
        } else {
            Kind::Logical(RowId((column - self.structural) as u32))
        }
    }

    /// One column's rows and coefficients, in ascending row order.
    ///
    /// A logical column is the single entry `-1` in its own row, produced here
    /// rather than stored: the constraint system is `A x - s = 0`, so the
    /// logical carries the negative sign and the row bounds live on it.
    ///
    /// # Panics
    ///
    /// Panics when the index is past the column count.
    pub fn column(&self, column: usize) -> Box<dyn Iterator<Item = (u32, f64)> + '_> {
        match self.kind(column) {
            Kind::Structural(variable) => {
                let span = self.starts[variable.index()]..self.starts[variable.index() + 1];
                Box::new(
                    self.indices[span.clone()]
                        .iter()
                        .copied()
                        .zip(self.values[span].iter().copied()),
                )
            }
            Kind::Logical(row) => Box::new(core::iter::once((row.0, -1.0))),
        }
    }

    /// One row's structural coefficients, as `(column, value)` pairs.
    ///
    /// Built by walking the columns, because the working copy stores by column
    /// and a second index would be a second thing to keep consistent. Callers
    /// that need this repeatedly should hoist it; the one caller that does —
    /// scaling — makes a bounded number of passes.
    pub fn structural_row(&self, row: usize) -> impl Iterator<Item = (usize, f64)> {
        (0..self.structural).flat_map(move |column| {
            let span = self.starts[column]..self.starts[column + 1];
            self.indices[span.clone()]
                .iter()
                .copied()
                .zip(self.values[span].iter().copied())
                .filter(move |(at, _)| *at as usize == row)
                .map(move |(_, value)| (column, value))
        })
    }

    /// A column's lower bound, possibly negative infinity.
    #[must_use]
    pub fn lower(&self, column: usize) -> f64 {
        self.lower[column]
    }

    /// A column's upper bound, possibly positive infinity.
    #[must_use]
    pub fn upper(&self, column: usize) -> f64 {
        self.upper[column]
    }

    /// A column's objective coefficient. Logical columns cost nothing.
    #[must_use]
    pub fn cost(&self, column: usize) -> f64 {
        self.cost[column]
    }

    /// Whether the model declares a column integer.
    #[must_use]
    pub fn integral(&self, column: usize) -> bool {
        self.integral[column]
    }

    /// The objective constant.
    #[must_use]
    pub fn constant(&self) -> f64 {
        self.constant
    }

    /// Whether a column is fixed, its two bounds being equal.
    #[must_use]
    pub fn is_fixed(&self, column: usize) -> bool {
        self.lower[column] == self.upper[column]
    }

    /// Whether a column has no finite bound in either direction.
    #[must_use]
    pub fn is_free(&self, column: usize) -> bool {
        self.lower[column] == -NO_BOUND && self.upper[column] == NO_BOUND
    }

    /// Whether a column's bounds exclude every value.
    ///
    /// This is not an error here. It means the model is contradictory, and
    /// producing the contradiction is exact machinery's job, not the search's.
    #[must_use]
    pub fn is_empty_range(&self, column: usize) -> bool {
        self.lower[column] > self.upper[column]
    }

    /// The objective at a point over all columns, including the constant.
    ///
    /// # Panics
    ///
    /// Panics when the point's length is not the column count.
    #[must_use]
    pub fn objective_at(&self, point: &[f64]) -> f64 {
        assert_eq!(
            point.len(),
            self.columns(),
            "point length differs from the column count"
        );
        let mut total = self.constant;
        for (column, value) in point.iter().enumerate() {
            total += self.cost[column] * value;
        }
        total
    }

    /// The residual of `A x - s` at a point, one entry per row.
    ///
    /// Zero in every entry is what feasibility of the equality system means. A
    /// residual is a diagnostic and never a claim: an exact recomputation over
    /// the original model is what decides whether a point is feasible.
    ///
    /// # Panics
    ///
    /// Panics when the point's length is not the column count.
    #[must_use]
    pub fn residual_at(&self, point: &[f64]) -> Vec<f64> {
        assert_eq!(
            point.len(),
            self.columns(),
            "point length differs from the column count"
        );
        let mut residual = vec![0.0; self.row_count];
        for (column, &value) in point.iter().enumerate() {
            if value == 0.0 {
                continue;
            }
            for (row, coefficient) in self.column(column) {
                residual[row as usize] += coefficient * value;
            }
        }
        residual
    }

    /// Whether every column sits inside its bounds, within a fence.
    ///
    /// # Panics
    ///
    /// Panics when the point's length is not the column count.
    #[must_use]
    pub fn within_bounds(&self, point: &[f64], fence: f64) -> bool {
        assert_eq!(
            point.len(),
            self.columns(),
            "point length differs from the column count"
        );
        (0..self.columns()).all(|column| {
            point[column] >= self.lower[column] - fence
                && point[column] <= self.upper[column] + fence
        })
    }
}

/// A bound in the working copy, or the infinity that stands for its absence.
fn bound(value: Option<&Rational>, absent: f64) -> f64 {
    value.map_or(absent, Rational::to_binary64)
}

#[cfg(test)]
mod narrowing_tests {
    use super::*;
    use hc_core::Model;
    use hc_io::mps::{Form, read};

    const TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

    fn model() -> Model {
        read(TINY, Form::Fixed).expect("the frozen instance parses")
    }

    #[test]
    fn narrowing_tightens_and_leaves_everything_else_alone() {
        let working = Working::from_model(&model());
        let child = working.narrowed(&[(0, 2.0, 3.0)]);
        assert_eq!(child.lower(0), 2.0);
        assert_eq!(child.upper(0), 3.0);
        assert_eq!(child.rows(), working.rows());
        assert_eq!(child.columns(), working.columns());
        assert_eq!(child.cost(0), working.cost(0));
        assert_eq!(child.digest(), working.digest());
    }

    #[test]
    fn a_request_that_would_widen_a_bound_is_ignored() {
        let working = Working::from_model(&model());
        let (low, high) = (working.lower(0), working.upper(0));
        let child = working.narrowed(&[(0, low - 100.0, high + 100.0)]);
        assert_eq!(child.lower(0), low);
        assert_eq!(child.upper(0), high);
    }

    #[test]
    fn a_substituted_objective_keeps_the_region_and_the_instance() {
        let working = Working::from_model(&model());
        let costs: Vec<f64> = (0..working.columns()).map(|column| column as f64).collect();
        let asked = working.with_costs(&costs);
        for column in 0..working.columns() {
            assert_eq!(asked.cost(column), column as f64);
            assert_eq!(asked.lower(column), working.lower(column));
            assert_eq!(asked.upper(column), working.upper(column));
            assert_eq!(asked.integral(column), working.integral(column));
        }
        assert_eq!(asked.rows(), working.rows());
        assert_eq!(asked.columns(), working.columns());
        assert_eq!(asked.nonzeros(), working.nonzeros());
        assert_eq!(asked.digest(), working.digest());
    }

    #[test]
    fn a_short_cost_vector_zeroes_the_columns_it_does_not_reach() {
        let working = Working::from_model(&model());
        let asked = working.with_costs(&[7.0]);
        assert_eq!(asked.cost(0), 7.0);
        for column in 1..working.columns() {
            assert_eq!(asked.cost(column), 0.0);
        }
    }

    #[test]
    fn a_long_cost_vector_is_truncated_rather_than_growing_the_shape() {
        let working = Working::from_model(&model());
        let costs = vec![1.0; working.columns() + 8];
        let asked = working.with_costs(&costs);
        assert_eq!(asked.columns(), working.columns());
        assert_eq!(asked.rows(), working.rows());
    }

    #[test]
    fn narrowing_composes_so_a_path_of_assumptions_accumulates() {
        let working = Working::from_model(&model());
        let child = working
            .narrowed(&[(0, 1.0, 8.0)])
            .narrowed(&[(0, 3.0, 5.0)]);
        assert_eq!(child.lower(0), 3.0);
        assert_eq!(child.upper(0), 5.0);
    }

    #[test]
    fn a_column_that_does_not_exist_is_ignored_rather_than_a_panic() {
        let working = Working::from_model(&model());
        let child = working.narrowed(&[(9_999, 0.0, 1.0)]);
        assert_eq!(child.columns(), working.columns());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_core::Tol;

    /// A model with one bounded integer column, one free column, a ranged row,
    /// and an equality row.
    fn sample() -> Model {
        let source = concat!(
            "ROWS\n N obj\n G r1\n E r2\n",
            "COLUMNS\n",
            " m MARKER INTORG\n",
            " x obj -1 r1 2\n",
            " x r2 1\n",
            " m MARKER INTEND\n",
            " y obj 3 r1 1\n",
            " y r2 -1\n",
            "RHS\n rhs r1 4 r2 1\n",
            " rhs obj 5\n",
            "RANGES\n rng r1 6\n",
            "BOUNDS\n UI b x 9\n FR b y\n",
            "ENDATA\n"
        );
        hc_io::mps::read(source.as_bytes(), hc_io::mps::Form::Free).expect("accepted")
    }

    #[test]
    fn the_shape_is_one_logical_column_per_row() {
        let working = Working::from_model(&sample());
        assert_eq!(working.rows(), 2);
        assert_eq!(working.structural(), 2);
        assert_eq!(working.columns(), 4);
        assert_eq!(working.kind(0), Kind::Structural(VarId(0)));
        assert_eq!(working.kind(1), Kind::Structural(VarId(1)));
        assert_eq!(working.kind(2), Kind::Logical(RowId(0)));
        assert_eq!(working.kind(3), Kind::Logical(RowId(1)));
    }

    #[test]
    fn a_logical_column_is_a_negative_one_in_its_own_row() {
        let working = Working::from_model(&sample());
        let logical: Vec<(u32, f64)> = working.column(2).collect();
        assert_eq!(logical, vec![(0, -1.0)]);
        let other: Vec<(u32, f64)> = working.column(3).collect();
        assert_eq!(other, vec![(1, -1.0)]);
        // And it is not stored, so it costs nothing in the coefficient count.
        assert_eq!(working.nonzeros(), 4);
    }

    #[test]
    fn structural_columns_carry_the_models_coefficients() {
        let working = Working::from_model(&sample());
        let first: Vec<(u32, f64)> = working.column(0).collect();
        assert_eq!(first, vec![(0, 2.0), (1, 1.0)]);
        let second: Vec<(u32, f64)> = working.column(1).collect();
        assert_eq!(second, vec![(0, 1.0), (1, -1.0)]);
    }

    #[test]
    fn row_bounds_move_onto_the_logical_columns() {
        let working = Working::from_model(&sample());
        // A `G` row ranged by six spans four to ten.
        assert_eq!(working.lower(2), 4.0);
        assert_eq!(working.upper(2), 10.0);
        // An equality row is a fixed logical.
        assert_eq!(working.lower(3), 1.0);
        assert_eq!(working.upper(3), 1.0);
        assert!(working.is_fixed(3));
        assert!(!working.is_fixed(2));
    }

    #[test]
    fn absent_bounds_become_infinities_only_on_this_side() {
        let model = sample();
        let working = Working::from_model(&model);
        // The free column has neither bound; the exact model records absence
        // rather than an infinity, and the working copy records an infinity.
        assert!(model.variables()[1].lower.is_none());
        assert!(model.variables()[1].upper.is_none());
        assert_eq!(working.lower(1), -NO_BOUND);
        assert_eq!(working.upper(1), NO_BOUND);
        assert!(working.is_free(1));
        assert!(!working.is_free(0));
    }

    #[test]
    fn integrality_and_costs_come_across_and_logicals_carry_neither() {
        let working = Working::from_model(&sample());
        assert!(working.integral(0));
        assert!(!working.integral(1));
        assert!(!working.integral(2));
        assert!(!working.integral(3));
        assert_eq!(working.cost(0), -1.0);
        assert_eq!(working.cost(1), 3.0);
        assert_eq!(working.cost(2), 0.0);
        assert_eq!(working.cost(3), 0.0);
        assert_eq!(working.constant(), -5.0);
    }

    #[test]
    fn the_residual_is_zero_exactly_when_the_logicals_hold_the_activities() {
        let working = Working::from_model(&sample());
        // Pick a structural point and set each logical to its row activity.
        let x = 3.0;
        let y = 2.0;
        let first = 2.0 * x + y;
        let second = x - y;
        let point = [x, y, first, second];
        assert_eq!(working.residual_at(&point), vec![0.0, 0.0]);

        // Move one logical off its activity and the residual says which row.
        let broken = [x, y, first + 1.0, second];
        let residual = working.residual_at(&broken);
        assert_eq!(residual[0], -1.0);
        assert_eq!(residual[1], 0.0);
    }

    #[test]
    fn the_objective_includes_the_constant_and_ignores_the_logicals() {
        let working = Working::from_model(&sample());
        let point = [3.0, 2.0, 8.0, 1.0];
        // -1 * 3 + 3 * 2 + constant of -5.
        assert_eq!(working.objective_at(&point), -2.0);
    }

    #[test]
    fn bound_checking_uses_the_fence_it_is_handed() {
        let working = Working::from_model(&sample());
        let tol = Tol::solver();
        let inside = [0.0, 0.0, 4.0, 1.0];
        assert!(working.within_bounds(&inside, tol.feas));
        let just_below = [0.0, 0.0, 4.0 - tol.feas / 2.0, 1.0];
        assert!(working.within_bounds(&just_below, tol.feas));
        let well_below = [0.0, 0.0, 3.0, 1.0];
        assert!(!working.within_bounds(&well_below, tol.feas));
    }

    #[test]
    fn a_contradictory_column_is_reported_rather_than_refused() {
        let model = hc_io::mps::read(
            b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nBOUNDS\n LO b x 5\n UP b x 2\nENDATA\n",
            hc_io::mps::Form::Free,
        )
        .expect("accepted");
        let working = Working::from_model(&model);
        assert!(working.is_empty_range(0));
        assert!(!working.is_empty_range(1));
    }

    #[test]
    fn the_working_copy_names_the_instance_it_came_from() {
        let model = sample();
        let working = Working::from_model(&model);
        assert_eq!(working.digest(), model.digest());
    }

    #[test]
    fn a_model_with_no_rows_still_has_a_well_formed_copy() {
        let model = hc_io::mps::read(b"ROWS\n N obj\nCOLUMNS\nENDATA\n", hc_io::mps::Form::Free)
            .expect("accepted");
        let working = Working::from_model(&model);
        assert_eq!(working.rows(), 0);
        assert_eq!(working.columns(), 0);
        assert_eq!(working.residual_at(&[]), Vec::<f64>::new());
        assert_eq!(working.objective_at(&[]), 0.0);
    }
}
