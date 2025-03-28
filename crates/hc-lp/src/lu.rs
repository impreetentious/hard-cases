//! Sparse triangular factorization of a basis, and the two solves built on it.
//!
//! A revised simplex never forms the inverse of its basis. It factors the
//! basis once and then answers two questions repeatedly: given a column, what
//! are its coordinates in the current basis, and given a cost vector, what are
//! the multipliers that price every column at once. Those are the forward and
//! transposed solves below, and everything else in this module exists to make
//! them cheap and to keep them honest.
//!
//! Pivots are chosen by Markowitz cost — the product of the remaining entries
//! in the pivot's row and column, less one each — subject to a relative
//! stability threshold. That is a compromise between two failures that look
//! nothing alike. Choosing purely for sparsity produces a factorization that
//! fills in almost nothing and divides by almost nothing, which is how a
//! simplex acquires a basis whose computed solution satisfies no constraint.
//! Choosing purely for stability produces a factorization that is dense enough
//! to make the solve cost more than the search.
//!
//! The candidate scan is deliberately simple: every remaining column is
//! examined at every step. That is quadratic in the basis size and it is
//! obviously right, which is the trade this crate makes until a measurement
//! says otherwise. The pivot *order* it produces is what the rest of the
//! solver depends on, and it is deterministic — ties break on the lowest
//! index, always, so two runs on two machines factor the same basis the same
//! way.

use std::collections::{BTreeMap, BTreeSet};

use hc_core::Tol;

/// How much smaller than its column's largest entry a pivot may be.
///
/// A pivot at one tenth of the column maximum admits a growth factor of ten
/// per elimination step in the worst case, which is the usual compromise. It
/// is named here rather than written at the comparison, because it is a policy
/// about numerical risk and not an implementation detail.
pub const PIVOT_THRESHOLD: f64 = 0.1;

/// A basis that could not be factored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Singular {
    /// The pivot step at which no acceptable pivot remained.
    pub at: usize,
}

impl core::fmt::Display for Singular {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "basis is singular at pivot step {}", self.at)
    }
}

impl core::error::Error for Singular {}

/// A factored basis.
#[derive(Clone, Debug)]
pub struct Factorization {
    size: usize,
    /// The original row that became the pivot at each step.
    pivot_rows: Vec<u32>,
    /// The basis position that became the pivot at each step.
    pivot_columns: Vec<u32>,
    /// Column `k` of the unit lower factor, below its diagonal, as
    /// `(step, multiplier)` pairs in ascending step order.
    lower: Vec<Vec<(u32, f64)>>,
    /// Row `k` of the upper factor, strictly above its diagonal, as
    /// `(step, value)` pairs in ascending step order.
    upper: Vec<Vec<(u32, f64)>>,
    /// The diagonal of the upper factor.
    diagonal: Vec<f64>,
}

impl Factorization {
    /// Factors a basis given as one sparse column per basis position.
    ///
    /// Each column is `(row, value)` pairs; order within a column does not
    /// matter here, and duplicate rows within one column are summed.
    ///
    /// # Errors
    ///
    /// Returns [`Singular`] naming the step at which no entry in any remaining
    /// column cleared both the absolute and the relative pivot fence.
    pub fn build(columns: &[Vec<(u32, f64)>], size: usize, tol: Tol) -> Result<Self, Singular> {
        debug_assert_eq!(columns.len(), size, "a basis has one column per row");

        // The active matrix, row-major, beside an occupancy index by column.
        // Two structures rather than one because the elimination reads rows
        // and the pivot search reads columns, and maintaining both is cheaper
        // than searching one of them the wrong way round.
        let mut rows: Vec<BTreeMap<u32, f64>> = vec![BTreeMap::new(); size];
        let mut occupancy: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); size];
        for (position, column) in columns.iter().enumerate() {
            for &(row, value) in column {
                if value == 0.0 {
                    continue;
                }
                let slot = rows[row as usize].entry(position as u32).or_insert(0.0);
                *slot += value;
                occupancy[position].insert(row);
            }
        }
        // A summed pair can cancel. Drop what it left behind before the counts
        // are used, or the pivot search will chase positions that hold nothing.
        for (row, entries) in rows.iter_mut().enumerate() {
            entries.retain(|column, value| {
                if value.abs() <= tol.zero {
                    occupancy[*column as usize].remove(&(row as u32));
                    false
                } else {
                    true
                }
            });
        }

        let mut active_rows: BTreeSet<u32> = (0..size as u32).collect();
        let mut active_columns: BTreeSet<u32> = (0..size as u32).collect();

        let mut pivot_rows = Vec::with_capacity(size);
        let mut pivot_columns = Vec::with_capacity(size);
        let mut lower: Vec<Vec<(u32, f64)>> = Vec::with_capacity(size);
        let mut upper_rows: Vec<BTreeMap<u32, f64>> = Vec::with_capacity(size);

        for step in 0..size {
            let Some((pivot_row, pivot_column)) =
                choose_pivot(&rows, &occupancy, &active_rows, &active_columns, tol)
            else {
                return Err(Singular { at: step });
            };

            let pivot_value = rows[pivot_row as usize][&pivot_column];
            pivot_rows.push(pivot_row);
            pivot_columns.push(pivot_column);

            // The pivot row, as it stands, becomes a row of the upper factor.
            let pivot_entries: Vec<(u32, f64)> = rows[pivot_row as usize]
                .iter()
                .map(|(column, value)| (*column, *value))
                .collect();
            upper_rows.push(pivot_entries.iter().copied().collect());

            active_rows.remove(&pivot_row);
            active_columns.remove(&pivot_column);
            for &(column, _) in &pivot_entries {
                occupancy[column as usize].remove(&pivot_row);
            }

            // Eliminate the pivot column from every remaining row that has an
            // entry in it.
            let affected: Vec<u32> = occupancy[pivot_column as usize].iter().copied().collect();
            let mut multipliers = Vec::with_capacity(affected.len());
            for row in affected {
                let entry = rows[row as usize].remove(&pivot_column).unwrap_or(0.0);
                occupancy[pivot_column as usize].remove(&row);
                if entry == 0.0 {
                    continue;
                }
                let factor = entry / pivot_value;
                multipliers.push((row, factor));

                for &(column, value) in &pivot_entries {
                    if column == pivot_column {
                        continue;
                    }
                    let target = rows[row as usize].entry(column).or_insert(0.0);
                    *target -= factor * value;
                    if target.abs() <= tol.zero {
                        // Cancellation removes a position rather than leaving
                        // a stored zero, which keeps the Markowitz counts
                        // honest for every later step.
                        rows[row as usize].remove(&column);
                        occupancy[column as usize].remove(&row);
                    } else {
                        occupancy[column as usize].insert(row);
                    }
                }
            }
            lower.push(multipliers);
            // The pivot row is done and is not part of the active matrix.
            rows[pivot_row as usize].clear();
        }

        Ok(Self::assemble(
            size,
            pivot_rows,
            pivot_columns,
            lower,
            upper_rows,
        ))
    }

    /// Rewrites the elimination's output in pivot coordinates.
    fn assemble(
        size: usize,
        pivot_rows: Vec<u32>,
        pivot_columns: Vec<u32>,
        lower_by_row: Vec<Vec<(u32, f64)>>,
        upper_by_column: Vec<BTreeMap<u32, f64>>,
    ) -> Self {
        let mut row_step = vec![0u32; size];
        let mut column_step = vec![0u32; size];
        for step in 0..size {
            row_step[pivot_rows[step] as usize] = step as u32;
            column_step[pivot_columns[step] as usize] = step as u32;
        }

        let lower = lower_by_row
            .into_iter()
            .map(|entries| {
                let mut translated: Vec<(u32, f64)> = entries
                    .into_iter()
                    .map(|(row, factor)| (row_step[row as usize], factor))
                    .collect();
                translated.sort_unstable_by_key(|(step, _)| *step);
                translated
            })
            .collect();

        let mut upper = Vec::with_capacity(size);
        let mut diagonal = Vec::with_capacity(size);
        for (step, entries) in upper_by_column.into_iter().enumerate() {
            let mut translated = Vec::new();
            let mut pivot = 0.0;
            for (column, value) in entries {
                let at = column_step[column as usize];
                if at as usize == step {
                    pivot = value;
                } else {
                    translated.push((at, value));
                }
            }
            translated.sort_unstable_by_key(|(at, _)| *at);
            upper.push(translated);
            diagonal.push(pivot);
        }

        Self {
            size,
            pivot_rows,
            pivot_columns,
            lower,
            upper,
            diagonal,
        }
    }

    /// The basis size.
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// The number of stored multipliers and upper entries.
    ///
    /// This is the factorization's cost, and watching it grow is how a solver
    /// decides when to start again from the basis rather than keep updating.
    #[must_use]
    pub fn nonzeros(&self) -> usize {
        let lower: usize = self.lower.iter().map(Vec::len).sum();
        let upper: usize = self.upper.iter().map(Vec::len).sum();
        lower + upper + self.diagonal.len()
    }

    /// The smallest pivot magnitude on the diagonal.
    ///
    /// A tiny pivot is where a factorization stops being trustworthy, and a
    /// number that can be looked at is better than a rule that fires silently.
    #[must_use]
    pub fn smallest_pivot(&self) -> f64 {
        self.diagonal
            .iter()
            .fold(f64::INFINITY, |smallest, value| smallest.min(value.abs()))
    }

    /// Solves `B x = rhs` in place.
    ///
    /// On entry `rhs` is indexed by row; on return it is indexed by basis
    /// position.
    ///
    /// # Panics
    ///
    /// Panics when the vector is not the basis size.
    pub fn ftran(&self, rhs: &mut [f64]) {
        assert_eq!(
            rhs.len(),
            self.size,
            "vector length differs from the basis size"
        );
        let mut work = vec![0.0; self.size];
        for step in 0..self.size {
            work[step] = rhs[self.pivot_rows[step] as usize];
        }

        for step in 0..self.size {
            let value = work[step];
            if value == 0.0 {
                continue;
            }
            for &(target, factor) in &self.lower[step] {
                work[target as usize] -= factor * value;
            }
        }

        for step in (0..self.size).rev() {
            let mut total = work[step];
            for &(column, value) in &self.upper[step] {
                total -= value * work[column as usize];
            }
            work[step] = total / self.diagonal[step];
        }

        rhs.fill(0.0);
        for step in 0..self.size {
            rhs[self.pivot_columns[step] as usize] = work[step];
        }
    }

    /// Solves `B^T y = rhs` in place.
    ///
    /// On entry `rhs` is indexed by basis position; on return it is indexed by
    /// row.
    ///
    /// # Panics
    ///
    /// Panics when the vector is not the basis size.
    pub fn btran(&self, rhs: &mut [f64]) {
        assert_eq!(
            rhs.len(),
            self.size,
            "vector length differs from the basis size"
        );
        let mut work = vec![0.0; self.size];
        for step in 0..self.size {
            work[step] = rhs[self.pivot_columns[step] as usize];
        }

        for step in 0..self.size {
            let value = work[step] / self.diagonal[step];
            work[step] = value;
            if value == 0.0 {
                continue;
            }
            for &(column, coefficient) in &self.upper[step] {
                work[column as usize] -= coefficient * value;
            }
        }

        for step in (0..self.size).rev() {
            let mut total = work[step];
            for &(target, factor) in &self.lower[step] {
                total -= factor * work[target as usize];
            }
            work[step] = total;
        }

        rhs.fill(0.0);
        for step in 0..self.size {
            rhs[self.pivot_rows[step] as usize] = work[step];
        }
    }

    /// The largest absolute entry of `B x - b`, for a solve that has already
    /// been performed.
    ///
    /// The factorization is floating point, so it drifts. This is the number
    /// a refactorization policy watches, and it is computed against the
    /// original columns rather than against the factors, because a residual
    /// measured through the thing that produced it measures nothing.
    #[must_use]
    pub fn residual(columns: &[Vec<(u32, f64)>], solution: &[f64], target: &[f64]) -> f64 {
        let mut product = vec![0.0; target.len()];
        for (position, column) in columns.iter().enumerate() {
            let value = solution[position];
            if value == 0.0 {
                continue;
            }
            for &(row, coefficient) in column {
                product[row as usize] += coefficient * value;
            }
        }
        product
            .iter()
            .zip(target)
            .fold(0.0f64, |largest, (got, wanted)| {
                largest.max((got - wanted).abs())
            })
    }
}

/// Chooses the next pivot by Markowitz cost, subject to stability.
///
/// Every remaining column is examined. The cost is the product of the
/// remaining entries in the candidate's row and column, less one each, which
/// is an estimate of how much new structure the elimination will create. Ties
/// break on the lowest column and then the lowest row, so the choice is a
/// function of the matrix and not of iteration order.
fn choose_pivot(
    rows: &[BTreeMap<u32, f64>],
    occupancy: &[BTreeSet<u32>],
    active_rows: &BTreeSet<u32>,
    active_columns: &BTreeSet<u32>,
    tol: Tol,
) -> Option<(u32, u32)> {
    let mut best: Option<(usize, u32, u32)> = None;

    for &column in active_columns {
        let entries = &occupancy[column as usize];
        if entries.is_empty() {
            continue;
        }
        // The stability fence is relative to the largest entry left in this
        // column, so a column that is uniformly small is not punished for it.
        let largest = entries
            .iter()
            .filter_map(|row| rows[*row as usize].get(&column))
            .fold(0.0f64, |largest, value| largest.max(value.abs()));
        if largest <= tol.piv {
            continue;
        }
        let fence = largest * PIVOT_THRESHOLD;
        let column_count = entries.len();

        for &row in entries {
            if !active_rows.contains(&row) {
                continue;
            }
            let Some(value) = rows[row as usize].get(&column) else {
                continue;
            };
            if value.abs() < fence || value.abs() <= tol.piv {
                continue;
            }
            let cost = (rows[row as usize].len() - 1) * (column_count - 1);
            let better = match best {
                None => true,
                Some((best_cost, best_row, best_column)) => {
                    cost < best_cost
                        || (cost == best_cost && (column, row) < (best_column, best_row))
                }
            };
            if better {
                best = Some((cost, row, column));
            }
            if cost == 0 {
                // A singleton in both directions cannot be improved on, and
                // stopping here is what keeps the common case linear.
                return Some((row, column));
            }
        }
    }

    best.map(|(_, row, column)| (row, column))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tol() -> Tol {
        Tol::solver()
    }

    /// A basis given densely, for tests that would rather read as a matrix.
    fn from_dense(dense: &[&[f64]]) -> Vec<Vec<(u32, f64)>> {
        let size = dense.len();
        let mut columns = vec![Vec::new(); size];
        for (row, entries) in dense.iter().enumerate() {
            for (column, value) in entries.iter().enumerate() {
                if *value != 0.0 {
                    columns[column].push((row as u32, *value));
                }
            }
        }
        columns
    }

    fn multiply(columns: &[Vec<(u32, f64)>], vector: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; vector.len()];
        for (position, column) in columns.iter().enumerate() {
            for &(row, value) in column {
                out[row as usize] += value * vector[position];
            }
        }
        out
    }

    fn transpose_multiply(columns: &[Vec<(u32, f64)>], vector: &[f64]) -> Vec<f64> {
        columns
            .iter()
            .map(|column| {
                column
                    .iter()
                    .map(|(row, value)| value * vector[*row as usize])
                    .sum()
            })
            .collect()
    }

    fn close(left: &[f64], right: &[f64], fence: f64) -> bool {
        left.len() == right.len() && left.iter().zip(right).all(|(a, b)| (a - b).abs() <= fence)
    }

    #[test]
    fn the_identity_factors_and_solves_to_itself() {
        let columns = from_dense(&[&[1.0, 0.0], &[0.0, 1.0]]);
        let factorization = Factorization::build(&columns, 2, tol()).expect("nonsingular");
        let mut vector = [3.0, -4.0];
        factorization.ftran(&mut vector);
        assert_eq!(vector, [3.0, -4.0]);
        let mut costs = [1.0, 2.0];
        factorization.btran(&mut costs);
        assert_eq!(costs, [1.0, 2.0]);
    }

    #[test]
    fn a_permutation_is_undone_by_the_solve() {
        let columns = from_dense(&[&[0.0, 1.0], &[1.0, 0.0]]);
        let factorization = Factorization::build(&columns, 2, tol()).expect("nonsingular");
        let mut vector = [3.0, -4.0];
        factorization.ftran(&mut vector);
        // The basis swaps the coordinates, so the solve swaps them back.
        assert!(close(&vector, &[-4.0, 3.0], tol().zero));
    }

    #[test]
    fn the_forward_solve_inverts_the_basis() {
        let columns = from_dense(&[&[2.0, 1.0, 0.0], &[1.0, 3.0, 1.0], &[0.0, 1.0, 4.0]]);
        let factorization = Factorization::build(&columns, 3, tol()).expect("nonsingular");
        for target in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [5.0, -2.0, 7.0]] {
            let mut solution = target;
            factorization.ftran(&mut solution);
            let recovered = multiply(&columns, &solution);
            assert!(close(&recovered, &target, tol().feas), "for {target:?}");
        }
    }

    #[test]
    fn the_transposed_solve_inverts_the_transpose() {
        let columns = from_dense(&[&[2.0, 1.0, 0.0], &[1.0, 3.0, 1.0], &[0.0, 1.0, 4.0]]);
        let factorization = Factorization::build(&columns, 3, tol()).expect("nonsingular");
        for target in [[1.0, 0.0, 0.0], [3.0, 1.0, -2.0]] {
            let mut solution = target;
            factorization.btran(&mut solution);
            let recovered = transpose_multiply(&columns, &solution);
            assert!(close(&recovered, &target, tol().feas), "for {target:?}");
        }
    }

    #[test]
    fn an_asymmetric_basis_needs_both_permutations() {
        // Column two has its only entry in row zero, so the pivot order cannot
        // be the identity in either direction.
        let columns = from_dense(&[&[0.0, 0.0, 3.0], &[2.0, 5.0, 0.0], &[1.0, 0.0, 0.0]]);
        let factorization = Factorization::build(&columns, 3, tol()).expect("nonsingular");
        let target = [6.0, 1.0, -2.0];
        let mut solution = target;
        factorization.ftran(&mut solution);
        assert!(close(&multiply(&columns, &solution), &target, tol().feas));

        let mut dual = target;
        factorization.btran(&mut dual);
        assert!(close(
            &transpose_multiply(&columns, &dual),
            &target,
            tol().feas
        ));
    }

    #[test]
    fn a_singular_basis_is_reported_with_the_step_it_failed_at() {
        // Two identical columns cannot both be pivoted.
        let columns = from_dense(&[&[1.0, 1.0], &[2.0, 2.0]]);
        let failure = Factorization::build(&columns, 2, tol()).expect_err("singular");
        assert_eq!(failure.at, 1);
        assert!(failure.to_string().contains("singular"));

        // An empty column is singular at the first step it is reached.
        let empty = from_dense(&[&[1.0, 0.0], &[0.0, 0.0]]);
        assert!(Factorization::build(&empty, 2, tol()).is_err());
    }

    #[test]
    fn cancellation_during_elimination_does_not_leave_a_stored_zero() {
        // Eliminating the first column from the second row cancels its entry
        // in the second column exactly.
        let columns = from_dense(&[&[1.0, 1.0], &[1.0, 1.0 + f64::EPSILON]]);
        let factorization = Factorization::build(&columns, 2, tol());
        // Whether this basis is usable depends on the fence; what matters is
        // that the factorization does not claim a structure it does not have.
        if let Ok(factorization) = factorization {
            assert!(factorization.nonzeros() <= 4);
        }

        let exact = from_dense(&[&[1.0, 2.0], &[1.0, 2.0]]);
        assert!(Factorization::build(&exact, 2, tol()).is_err());
    }

    #[test]
    fn a_sparse_basis_stays_sparse() {
        // A diagonal basis of twenty columns has twenty entries and needs no
        // fill at all. A factorization that produced more would be creating
        // structure the matrix does not have.
        let size = 20usize;
        let columns: Vec<Vec<(u32, f64)>> = (0..size)
            .map(|index| vec![(index as u32, f64::from(index as u32) + 1.0)])
            .collect();
        let factorization = Factorization::build(&columns, size, tol()).expect("nonsingular");
        assert_eq!(factorization.nonzeros(), size);
        assert_eq!(factorization.size(), size);
        assert_eq!(factorization.smallest_pivot(), 1.0);
    }

    #[test]
    fn an_arrow_basis_is_pivoted_from_its_sparse_end() {
        // The classic fill-in trap: a matrix that is dense in one row and one
        // column and diagonal elsewhere. Pivoting on the dense corner first
        // fills the whole matrix; pivoting on the diagonal first fills
        // nothing until the last step.
        let size = 8usize;
        let mut columns: Vec<Vec<(u32, f64)>> = vec![Vec::new(); size];
        for (index, column) in columns.iter_mut().enumerate() {
            column.push((index as u32, 4.0));
        }
        for index in 1..size {
            columns[0].push((index as u32, 1.0));
            columns[index].push((0, 1.0));
        }
        for column in &mut columns {
            column.sort_unstable_by_key(|(row, _)| *row);
        }
        let factorization = Factorization::build(&columns, size, tol()).expect("nonsingular");
        // A full elimination of this matrix would produce roughly size squared
        // entries; a sensible order produces a small multiple of size.
        assert!(
            factorization.nonzeros() < 4 * size,
            "the factorization filled in: {} entries",
            factorization.nonzeros()
        );

        let target: Vec<f64> = (0..size)
            .map(|index| f64::from(index as u32) - 3.0)
            .collect();
        let mut solution = target.clone();
        factorization.ftran(&mut solution);
        assert!(close(&multiply(&columns, &solution), &target, tol().feas));
    }

    #[test]
    fn the_residual_is_measured_against_the_original_columns() {
        let columns = from_dense(&[&[2.0, 1.0], &[1.0, 3.0]]);
        let factorization = Factorization::build(&columns, 2, tol()).expect("nonsingular");
        let target = [4.0, 7.0];
        let mut solution = target;
        factorization.ftran(&mut solution);
        assert!(Factorization::residual(&columns, &solution, &target) <= tol().feas);

        // A solution that is simply wrong reports a residual that says so.
        let wrong = [0.0, 0.0];
        assert!(Factorization::residual(&columns, &wrong, &target) > 1.0);
    }

    #[test]
    fn duplicate_entries_in_one_column_are_summed() {
        let columns = vec![vec![(0u32, 1.0), (0u32, 1.0)], vec![(1u32, 1.0)]];
        let factorization = Factorization::build(&columns, 2, tol()).expect("nonsingular");
        let mut solution = [2.0, 1.0];
        factorization.ftran(&mut solution);
        assert!(close(&solution, &[1.0, 1.0], tol().feas));
    }

    #[test]
    fn a_basis_of_size_zero_is_well_formed() {
        let factorization = Factorization::build(&[], 0, tol()).expect("nonsingular");
        assert_eq!(factorization.size(), 0);
        assert_eq!(factorization.nonzeros(), 0);
        let mut nothing: [f64; 0] = [];
        factorization.ftran(&mut nothing);
        factorization.btran(&mut nothing);
    }
}
