//! Scaling the working copy, and the reason it can never reach a claim.
//!
//! A model whose coefficients span many orders of magnitude makes a floating
//! point search behave badly: pivots that look acceptable are not, feasibility
//! fences mean different things in different rows, and a factorization drifts
//! faster than any policy can catch. Scaling rewrites the working copy so the
//! entries sit closer together, and rewrites nothing else.
//!
//! Every scale factor is a power of two. That is the load-bearing choice in
//! this module. Multiplying a binary floating point number by a power of two
//! is exact — the significand does not move — so scaling introduces no
//! rounding error at all, and unscaling recovers the original value bit for
//! bit rather than approximately. A scaling that rounded would put a
//! perturbation between the model on disk and the model the search believes
//! in, and every later disagreement would have to be traced through it.
//!
//! The exact model is never scaled. Not scaled and then unscaled, not scaled
//! into a copy that a proof reads — never touched. A certificate is about the
//! bytes of a file, and a scale factor is a fact about how the search chose to
//! look at them. The trap named for this rule checks that the exact model and
//! its canonical dump are identical before and after, because "we would
//! notice" is not a mechanism.

use hc_core::Tol;

use crate::working::Working;

/// How many alternating row and column passes to make.
pub const SCALING_PASSES: usize = 10;

/// The relative improvement in spread below which further passes are pointless.
pub const SCALING_CONVERGENCE: f64 = 0.01;

/// Multiplicative factors for the rows and columns of a working copy.
///
/// Every factor is a power of two, so applying and undoing them is exact.
#[derive(Clone, Debug, PartialEq)]
pub struct Scaling {
    row: Vec<f64>,
    column: Vec<f64>,
}

impl Scaling {
    /// The scaling that changes nothing.
    #[must_use]
    pub fn identity(rows: usize, columns: usize) -> Self {
        Self {
            row: vec![1.0; rows],
            column: vec![1.0; columns],
        }
    }

    /// Geometric scaling: alternating passes that centre each row and column
    /// on the geometric mean of its extreme entries.
    ///
    /// Only structural coefficients take part. A logical column is the
    /// identity entry standing for a row's activity, and its factor is fixed
    /// at the reciprocal of its row's so that the entry stays exactly minus
    /// one — which keeps the basis structure the search relies on and moves
    /// the row's bounds instead.
    #[must_use]
    pub fn geometric(working: &Working, tol: Tol) -> Self {
        let rows = working.rows();
        let structural = working.structural();
        let mut row = vec![1.0f64; rows];
        let mut column = vec![1.0f64; structural];

        if rows == 0 || structural == 0 {
            return Self::finish(row, column);
        }

        let mut previous = spread(working, &row, &column, tol);
        for _ in 0..SCALING_PASSES {
            // The factors for a pass are computed against the state at the
            // start of it and applied together, so a row is never balanced
            // against a column that has already moved within the same pass.
            let mut row_factors = vec![1.0f64; rows];
            for (index, slot) in row_factors.iter_mut().enumerate() {
                let scale = row[index];
                *slot = balance(
                    working
                        .structural_row(index)
                        .map(|(variable, value)| value.abs() * scale * column[variable]),
                    tol,
                )
                .unwrap_or(1.0);
            }
            for (scale, factor) in row.iter_mut().zip(&row_factors) {
                *scale *= factor;
            }

            let mut column_factors = vec![1.0f64; structural];
            for (index, slot) in column_factors.iter_mut().enumerate() {
                let scale = column[index];
                *slot = balance(
                    working
                        .column(index)
                        .map(|(at, value)| value.abs() * row[at as usize] * scale),
                    tol,
                )
                .unwrap_or(1.0);
            }
            for (scale, factor) in column.iter_mut().zip(&column_factors) {
                *scale *= factor;
            }

            let current = spread(working, &row, &column, tol);
            if previous <= 0.0 || (previous - current) / previous < SCALING_CONVERGENCE {
                break;
            }
            previous = current;
        }

        Self::finish(row, column)
    }

    /// Snaps every factor to a power of two and appends the logical factors.
    fn finish(row: Vec<f64>, column: Vec<f64>) -> Self {
        let row: Vec<f64> = row.into_iter().map(nearest_power_of_two).collect();
        let mut column: Vec<f64> = column.into_iter().map(nearest_power_of_two).collect();
        column.reserve(row.len());
        for factor in &row {
            // The logical's factor is the reciprocal of its row's, so the
            // identity entry survives scaling untouched.
            column.push(1.0 / factor);
        }
        Self { row, column }
    }

    /// The factor applied to a row.
    #[must_use]
    pub fn row(&self, index: usize) -> f64 {
        self.row[index]
    }

    /// The factor applied to a column, structural or logical.
    #[must_use]
    pub fn column(&self, index: usize) -> f64 {
        self.column[index]
    }

    /// Whether every factor is one.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.row.iter().all(|factor| *factor == 1.0)
            && self.column.iter().all(|factor| *factor == 1.0)
    }

    /// A column of the scaled matrix.
    #[must_use]
    pub fn scaled_column(&self, working: &Working, index: usize) -> Vec<(u32, f64)> {
        working
            .column(index)
            .map(|(at, value)| (at, self.row[at as usize] * value * self.column[index]))
            .collect()
    }

    /// A column's bounds in the scaled copy.
    #[must_use]
    pub fn scaled_bounds(&self, working: &Working, index: usize) -> (f64, f64) {
        let factor = self.column[index];
        (working.lower(index) / factor, working.upper(index) / factor)
    }

    /// A column's objective coefficient in the scaled copy.
    #[must_use]
    pub fn scaled_cost(&self, working: &Working, index: usize) -> f64 {
        working.cost(index) * self.column[index]
    }

    /// Recovers an unscaled primal point in place.
    ///
    /// # Panics
    ///
    /// Panics when the point is not the column count.
    pub fn recover_primal(&self, point: &mut [f64]) {
        assert_eq!(
            point.len(),
            self.column.len(),
            "point length differs from the column count"
        );
        for (value, factor) in point.iter_mut().zip(&self.column) {
            *value *= factor;
        }
    }

    /// Recovers unscaled row multipliers in place.
    ///
    /// # Panics
    ///
    /// Panics when the vector is not the row count.
    pub fn recover_dual(&self, multipliers: &mut [f64]) {
        assert_eq!(
            multipliers.len(),
            self.row.len(),
            "vector length differs from the row count"
        );
        for (value, factor) in multipliers.iter_mut().zip(&self.row) {
            *value *= factor;
        }
    }

    /// The ratio of the largest to the smallest structural entry after
    /// scaling, which is the number scaling exists to reduce.
    #[must_use]
    pub fn spread(&self, working: &Working, tol: Tol) -> f64 {
        spread(working, &self.row, &self.column, tol)
    }
}

/// The factor that centres a row or column on the geometric mean of its
/// extremes, or nothing when it holds no usable entry.
fn balance(magnitudes: impl Iterator<Item = f64>, tol: Tol) -> Option<f64> {
    let mut smallest = f64::INFINITY;
    let mut largest = 0.0f64;
    for magnitude in magnitudes {
        if magnitude <= tol.zero {
            continue;
        }
        smallest = smallest.min(magnitude);
        largest = largest.max(magnitude);
    }
    if largest == 0.0 || !smallest.is_finite() {
        return None;
    }
    Some(1.0 / (smallest * largest).sqrt())
}

/// The ratio of the largest to the smallest structural entry under a scaling.
fn spread(working: &Working, row: &[f64], column: &[f64], tol: Tol) -> f64 {
    let mut smallest = f64::INFINITY;
    let mut largest = 0.0f64;
    for (index, factor) in column.iter().enumerate().take(working.structural()) {
        for (at, value) in working.column(index) {
            let magnitude = (row[at as usize] * value * factor).abs();
            if magnitude <= tol.zero {
                continue;
            }
            smallest = smallest.min(magnitude);
            largest = largest.max(magnitude);
        }
    }
    if largest == 0.0 || !smallest.is_finite() {
        1.0
    } else {
        largest / smallest
    }
}

/// The power of two nearest a positive value, in the geometric sense.
///
/// Powers of two are the whole point: multiplying by one moves no significand
/// bit, so a scaled model is the original model exactly, seen through a change
/// of units.
fn nearest_power_of_two(value: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        return 1.0;
    }
    let exponent = value.log2().round();
    let snapped = exponent.exp2();
    if snapped.is_finite() && snapped > 0.0 {
        snapped
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_core::Model;

    fn model(source: &str) -> Model {
        hc_io::mps::read(source.as_bytes(), hc_io::mps::Form::Free).expect("accepted")
    }

    /// A model whose coefficients span eight orders of magnitude.
    fn wide() -> Model {
        model(concat!(
            "ROWS\n N obj\n L r1\n L r2\n G r3\n",
            "COLUMNS\n",
            " x obj 1 r1 10000\n",
            " x r2 0.001\n",
            " y obj 1 r1 20000\n",
            " y r3 0.002\n",
            " z obj 1 r2 30000000\n",
            " z r3 4\n",
            "RHS\n rhs r1 100000 r2 3\n rhs r3 1\n",
            "ENDATA\n"
        ))
    }

    #[test]
    fn every_factor_is_a_power_of_two() {
        let working = Working::from_model(&wide());
        let scaling = Scaling::geometric(&working, Tol::solver());
        for index in 0..working.rows() {
            let factor = scaling.row(index);
            assert_eq!(
                factor.log2(),
                factor.log2().round(),
                "row {index} factor {factor}"
            );
        }
        for index in 0..working.columns() {
            let factor = scaling.column(index);
            assert_eq!(
                factor.log2(),
                factor.log2().round(),
                "column {index} factor {factor}"
            );
        }
    }

    #[test]
    fn scaling_narrows_the_spread_it_exists_to_narrow() {
        let working = Working::from_model(&wide());
        let tol = Tol::solver();
        let before = Scaling::identity(working.rows(), working.columns()).spread(&working, tol);
        let scaling = Scaling::geometric(&working, tol);
        let after = scaling.spread(&working, tol);
        assert!(
            before > 1.0e9,
            "the fixture is not wide enough to be worth scaling"
        );
        assert!(
            after < before,
            "scaling made no difference: {before} to {after}"
        );
        assert!(
            after < before / 100.0,
            "scaling barely helped: {before} to {after}"
        );
    }

    #[test]
    fn a_logical_column_survives_scaling_untouched() {
        // The identity entry has to stay exactly minus one, or the basis
        // structure the search relies on stops being an identity block.
        let working = Working::from_model(&wide());
        let scaling = Scaling::geometric(&working, Tol::solver());
        for row in 0..working.rows() {
            let column = working.structural() + row;
            let scaled = scaling.scaled_column(&working, column);
            assert_eq!(scaled, vec![(row as u32, -1.0)], "logical for row {row}");
        }
    }

    #[test]
    fn scaling_and_unscaling_recover_the_original_bit_for_bit() {
        // Not approximately. Every factor is a power of two, so the round trip
        // is exact, and a test that allowed a fence here would be hiding the
        // property the module is built around.
        let working = Working::from_model(&wide());
        let scaling = Scaling::geometric(&working, Tol::solver());
        let original: Vec<f64> = (0..working.columns())
            .map(|index| f64::from(index as u32) * 0.375 - 1.25)
            .collect();

        let mut scaled: Vec<f64> = original
            .iter()
            .enumerate()
            .map(|(index, value)| value / scaling.column(index))
            .collect();
        scaling.recover_primal(&mut scaled);
        assert_eq!(scaled, original);

        let duals: Vec<f64> = (0..working.rows())
            .map(|index| f64::from(index as u32) * 0.5 + 0.125)
            .collect();
        let mut scaled_duals: Vec<f64> = duals
            .iter()
            .enumerate()
            .map(|(index, value)| value / scaling.row(index))
            .collect();
        scaling.recover_dual(&mut scaled_duals);
        assert_eq!(scaled_duals, duals);
    }

    #[test]
    fn the_scaled_model_is_the_same_model_in_different_units() {
        // A point feasible in the scaled copy maps to one feasible in the
        // unscaled copy, with the same objective. That is the only property
        // that makes scaling legitimate at all.
        let working = Working::from_model(&wide());
        let scaling = Scaling::geometric(&working, Tol::solver());

        let point: Vec<f64> = (0..working.columns())
            .map(|index| index as f64 + 1.0)
            .collect();
        let mut scaled_point: Vec<f64> = point
            .iter()
            .enumerate()
            .map(|(index, value)| value / scaling.column(index))
            .collect();

        // The scaled residual is the unscaled residual, row by row, times the
        // row's factor — so zero in one is zero in the other.
        let residual = working.residual_at(&point);
        let mut scaled_residual = vec![0.0; working.rows()];
        for (index, coordinate) in scaled_point.iter().enumerate() {
            for (at, value) in scaling.scaled_column(&working, index) {
                scaled_residual[at as usize] += value * coordinate;
            }
        }
        for row in 0..working.rows() {
            let expected = residual[row] * scaling.row(row);
            assert!(
                (scaled_residual[row] - expected).abs()
                    <= Tol::solver().zero * expected.abs().max(1.0),
                "row {row}: {} against {expected}",
                scaled_residual[row]
            );
        }

        // The objective is invariant, because the cost scales inversely to the
        // variable it multiplies.
        let scaled_objective: f64 = (0..working.columns())
            .map(|index| scaling.scaled_cost(&working, index) * scaled_point[index])
            .sum::<f64>()
            + working.constant();
        assert!((scaled_objective - working.objective_at(&point)).abs() <= Tol::solver().feas);

        scaling.recover_primal(&mut scaled_point);
        assert_eq!(scaled_point, point);
    }

    #[test]
    fn bounds_move_with_their_columns() {
        let working = Working::from_model(&wide());
        let scaling = Scaling::geometric(&working, Tol::solver());
        for index in 0..working.columns() {
            let (low, high) = scaling.scaled_bounds(&working, index);
            let factor = scaling.column(index);
            assert_eq!(low * factor, working.lower(index), "column {index} lower");
            assert_eq!(high * factor, working.upper(index), "column {index} upper");
        }
    }

    #[test]
    fn a_model_with_nothing_to_scale_gets_the_identity() {
        let empty = model("ROWS\n N obj\nCOLUMNS\nENDATA\n");
        let working = Working::from_model(&empty);
        let scaling = Scaling::geometric(&working, Tol::solver());
        assert!(scaling.is_identity());
        assert_eq!(scaling.spread(&working, Tol::solver()), 1.0);
    }

    #[test]
    fn a_uniform_matrix_is_left_alone_or_moved_uniformly() {
        let uniform = model(
            "ROWS\n N obj\n L r1\n L r2\nCOLUMNS\n x obj 1 r1 1\n x r2 1\n y obj 1 r1 1\n y r2 1\nRHS\n rhs r1 1 r2 1\nENDATA\n",
        );
        let working = Working::from_model(&uniform);
        let scaling = Scaling::geometric(&working, Tol::solver());
        assert_eq!(scaling.spread(&working, Tol::solver()), 1.0);
    }

    #[test]
    fn the_nearest_power_of_two_is_the_geometric_one() {
        assert_eq!(nearest_power_of_two(1.0), 1.0);
        assert_eq!(nearest_power_of_two(3.0), 4.0);
        assert_eq!(nearest_power_of_two(2.8), 2.0);
        assert_eq!(nearest_power_of_two(2.9), 4.0);
        assert_eq!(nearest_power_of_two(0.3), 0.25);
        assert_eq!(nearest_power_of_two(0.0), 1.0);
        assert_eq!(nearest_power_of_two(-5.0), 1.0);
        assert_eq!(nearest_power_of_two(f64::INFINITY), 1.0);
        assert_eq!(nearest_power_of_two(f64::NAN), 1.0);
    }
}
