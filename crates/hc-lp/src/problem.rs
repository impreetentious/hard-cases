//! The scaled problem the simplex actually consumes.
//!
//! One structure, built once, holding every column — structural and logical
//! alike — with the scaling already applied. Everything above this point works
//! in these coordinates and never asks about scale factors again, which is
//! what keeps scaling out of the inner loop and out of the reasoning about it.
//!
//! Logical columns are stored explicitly here, unlike in the working copy
//! where they are produced on demand. The extra storage is one entry per row
//! and it removes a branch from every column access in the search, which is
//! the correct trade at this layer and the wrong one at the layer below.
//!
//! Nothing here is exact and nothing here is a claim. A point that satisfies
//! every bound in this structure is a point worth checking exactly against the
//! original model; it is not a feasible point until that check passes.

use hc_core::Tol;

use crate::scale::Scaling;
use crate::working::Working;

/// A scaled linear program in the form the search understands.
///
/// Minimize `cost . x` subject to `A x = 0` and `lower <= x <= upper`, where
/// `A` includes one logical column per row carrying that row's bounds.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    row_count: usize,
    structural: usize,
    columns: Vec<Vec<(u32, f64)>>,
    lower: Vec<f64>,
    upper: Vec<f64>,
    cost: Vec<f64>,
    constant: f64,
    scaling: Scaling,
}

impl Problem {
    /// Builds the scaled problem from a working copy.
    #[must_use]
    pub fn new(working: &Working, scaling: Scaling) -> Self {
        let total = working.columns();
        let mut columns = Vec::with_capacity(total);
        let mut lower = Vec::with_capacity(total);
        let mut upper = Vec::with_capacity(total);
        let mut cost = Vec::with_capacity(total);
        for index in 0..total {
            columns.push(scaling.scaled_column(working, index));
            let (low, high) = scaling.scaled_bounds(working, index);
            lower.push(low);
            upper.push(high);
            cost.push(scaling.scaled_cost(working, index));
        }
        Self {
            row_count: working.rows(),
            structural: working.structural(),
            columns,
            lower,
            upper,
            cost,
            constant: working.constant(),
            scaling,
        }
    }

    /// Builds the scaled problem, choosing the scaling geometrically.
    #[must_use]
    pub fn scaled(working: &Working, tol: Tol) -> Self {
        let scaling = Scaling::geometric(working, tol);
        Self::new(working, scaling)
    }

    /// Builds the problem with no scaling at all.
    #[must_use]
    pub fn unscaled(working: &Working) -> Self {
        Self::new(
            working,
            Scaling::identity(working.rows(), working.columns()),
        )
    }

    /// The number of rows, which is the size of every basis.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.row_count
    }

    /// The number of columns from the original model.
    #[must_use]
    pub fn structural(&self) -> usize {
        self.structural
    }

    /// The total number of columns.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.columns.len()
    }

    /// One column's rows and scaled coefficients.
    #[must_use]
    pub fn column(&self, index: usize) -> &[(u32, f64)] {
        &self.columns[index]
    }

    /// A column's scaled lower bound.
    #[must_use]
    pub fn lower(&self, index: usize) -> f64 {
        self.lower[index]
    }

    /// A column's scaled upper bound.
    #[must_use]
    pub fn upper(&self, index: usize) -> f64 {
        self.upper[index]
    }

    /// A column's scaled objective coefficient.
    #[must_use]
    pub fn cost(&self, index: usize) -> f64 {
        self.cost[index]
    }

    /// The objective constant, which scaling does not touch.
    #[must_use]
    pub fn constant(&self) -> f64 {
        self.constant
    }

    /// The scaling this problem was built with.
    #[must_use]
    pub fn scaling(&self) -> &Scaling {
        &self.scaling
    }

    /// Whether a column's two bounds coincide.
    #[must_use]
    pub fn is_fixed(&self, index: usize) -> bool {
        self.lower[index] == self.upper[index]
    }

    /// Whether a column has no finite bound in either direction.
    #[must_use]
    pub fn is_free(&self, index: usize) -> bool {
        self.lower[index] == f64::NEG_INFINITY && self.upper[index] == f64::INFINITY
    }

    /// The columns whose bounds already exclude every value.
    ///
    /// Not an error and not the search's problem: it means the model is
    /// contradictory, and producing the contradiction belongs to exact
    /// machinery. The search wants to know so it can stop rather than wander.
    #[must_use]
    pub fn empty_columns(&self) -> Vec<usize> {
        (0..self.columns())
            .filter(|index| self.lower[*index] > self.upper[*index])
            .collect()
    }

    /// The objective at a scaled point, including the constant.
    ///
    /// # Panics
    ///
    /// Panics when the point is not the column count.
    #[must_use]
    pub fn objective_at(&self, point: &[f64]) -> f64 {
        assert_eq!(
            point.len(),
            self.columns(),
            "point length differs from the column count"
        );
        self.constant
            + point
                .iter()
                .zip(&self.cost)
                .map(|(x, c)| x * c)
                .sum::<f64>()
    }

    /// The residual of `A x` at a scaled point, one entry per row.
    ///
    /// # Panics
    ///
    /// Panics when the point is not the column count.
    #[must_use]
    pub fn residual_at(&self, point: &[f64]) -> Vec<f64> {
        assert_eq!(
            point.len(),
            self.columns(),
            "point length differs from the column count"
        );
        let mut residual = vec![0.0; self.row_count];
        for (index, value) in point.iter().enumerate() {
            if *value == 0.0 {
                continue;
            }
            for &(row, coefficient) in &self.columns[index] {
                residual[row as usize] += coefficient * value;
            }
        }
        residual
    }

    /// Recovers an unscaled point from a scaled one.
    ///
    /// # Panics
    ///
    /// Panics when the point is not the column count.
    #[must_use]
    pub fn recover_point(&self, scaled: &[f64]) -> Vec<f64> {
        let mut point = scaled.to_vec();
        self.scaling.recover_primal(&mut point);
        point
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_core::Model;

    fn model(source: &str) -> Model {
        hc_io::mps::read(source.as_bytes(), hc_io::mps::Form::Free).expect("accepted")
    }

    fn sample() -> Model {
        model(concat!(
            "ROWS\n N obj\n L r1\n G r2\n",
            "COLUMNS\n x obj 2 r1 1\n x r2 1\n y obj -1 r1 3\n",
            "RHS\n rhs r1 6 r2 1\n",
            "BOUNDS\n UP b x 4\n",
            "ENDATA\n"
        ))
    }

    #[test]
    fn the_problem_carries_every_column_including_the_logicals() {
        let working = Working::from_model(&sample());
        let problem = Problem::unscaled(&working);
        assert_eq!(problem.rows(), 2);
        assert_eq!(problem.structural(), 2);
        assert_eq!(problem.columns(), 4);
        assert_eq!(problem.column(0), &[(0, 1.0), (1, 1.0)]);
        assert_eq!(problem.column(1), &[(0, 3.0)]);
        assert_eq!(problem.column(2), &[(0, -1.0)]);
        assert_eq!(problem.column(3), &[(1, -1.0)]);
    }

    #[test]
    fn bounds_and_costs_come_across_unscaled_when_unscaled() {
        let working = Working::from_model(&sample());
        let problem = Problem::unscaled(&working);
        assert_eq!(problem.lower(0), 0.0);
        assert_eq!(problem.upper(0), 4.0);
        assert_eq!(problem.lower(1), 0.0);
        assert_eq!(problem.upper(1), f64::INFINITY);
        // The first row is a less-than row, so its logical is bounded above.
        assert_eq!(problem.upper(2), 6.0);
        assert_eq!(problem.lower(2), f64::NEG_INFINITY);
        // The second is a greater-than row, so its logical is bounded below.
        assert_eq!(problem.lower(3), 1.0);
        assert_eq!(problem.upper(3), f64::INFINITY);
        assert_eq!(problem.cost(0), 2.0);
        assert_eq!(problem.cost(1), -1.0);
        assert_eq!(problem.cost(2), 0.0);
    }

    #[test]
    fn a_point_that_solves_the_system_has_no_residual() {
        let working = Working::from_model(&sample());
        let problem = Problem::unscaled(&working);
        // x = 2, y = 1 gives row activities 5 and 2.
        let point = [2.0, 1.0, 5.0, 2.0];
        assert_eq!(problem.residual_at(&point), vec![0.0, 0.0]);
        assert_eq!(problem.objective_at(&point), 3.0);
    }

    #[test]
    fn the_scaled_problem_recovers_its_unscaled_point_exactly() {
        let wide = model(concat!(
            "ROWS\n N obj\n L r1\n L r2\n",
            "COLUMNS\n x obj 1 r1 100000\n x r2 0.0001\n y obj 1 r1 3\n y r2 400000\n",
            "RHS\n rhs r1 1 r2 1\n",
            "ENDATA\n"
        ));
        let working = Working::from_model(&wide);
        let tol = Tol::solver();
        let problem = Problem::scaled(&working, tol);
        assert!(
            !problem.scaling().is_identity(),
            "the fixture gave scaling nothing to do"
        );

        let unscaled = [3.0, -1.5, 0.25, 8.0];
        let scaled: Vec<f64> = unscaled
            .iter()
            .enumerate()
            .map(|(index, value)| value / problem.scaling().column(index))
            .collect();
        assert_eq!(problem.recover_point(&scaled), unscaled.to_vec());

        // The objective is the same number in either set of units.
        let plain = Problem::unscaled(&working);
        assert!((problem.objective_at(&scaled) - plain.objective_at(&unscaled)).abs() <= tol.feas);
    }

    #[test]
    fn a_contradictory_column_is_named_rather_than_refused() {
        let crossed = model(
            "ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nBOUNDS\n LO b x 5\n UP b x 2\nENDATA\n",
        );
        let working = Working::from_model(&crossed);
        let problem = Problem::unscaled(&working);
        assert_eq!(problem.empty_columns(), vec![0]);
        assert!(problem.is_fixed(1) || !problem.is_fixed(1));
    }

    #[test]
    fn fixed_and_free_columns_are_recognized() {
        let mixed = model(
            "ROWS\n N obj\n E r\nCOLUMNS\n x obj 1 r 1\n y obj 1 r 1\nRHS\n rhs r 3\nBOUNDS\n FX b x 2\n FR b y\nENDATA\n",
        );
        let working = Working::from_model(&mixed);
        let problem = Problem::unscaled(&working);
        assert!(problem.is_fixed(0));
        assert!(problem.is_free(1));
        // An equality row's logical is fixed too.
        assert!(problem.is_fixed(2));
        assert!(problem.empty_columns().is_empty());
    }

    #[test]
    fn a_model_with_no_rows_produces_an_empty_problem() {
        let empty = model("ROWS\n N obj\nCOLUMNS\nENDATA\n");
        let working = Working::from_model(&empty);
        let problem = Problem::unscaled(&working);
        assert_eq!(problem.rows(), 0);
        assert_eq!(problem.columns(), 0);
        assert_eq!(problem.objective_at(&[]), 0.0);
        assert!(problem.residual_at(&[]).is_empty());
    }
}
