//! Exact linear algebra, for rebuilding what a basis implies.
//!
//! Dense elimination over rationals. Dense because the bases a certificate is
//! assembled from are small enough for it and because a sparse exact
//! elimination has an ordering heuristic, and an ordering heuristic in code
//! that decides what is true is a thing that has to be argued about rather
//! than read.
//!
//! Exact arithmetic removes the usual reason to choose a pivot — there is no
//! stability to lose — and replaces it with a different one. Entries grow: a
//! quotient of two rationals with hundred-bit parts has parts of two hundred
//! bits, and a careless pivot order compounds that at every step. So the pivot
//! is the smallest admissible entry by bit length, which is a size decision
//! and cannot affect the answer, only how long it takes to reach.

use hc_exact::Rational;

/// Why an exact solve could not be completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The system has no unique solution: some column is a combination of the
    /// others, exactly.
    ///
    /// This is not the same as the float factorization refusing a basis. A
    /// basis can factor comfortably in floating point and be singular in
    /// exact arithmetic; that is precisely the case worth catching here.
    Singular {
        /// The elimination step at which no nonzero pivot remained.
        at: usize,
    },
    /// The system's shape does not match its right-hand side.
    Shape,
}

impl core::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Singular { at } => {
                write!(formatter, "the system is exactly singular at step {at}")
            }
            Self::Shape => {
                formatter.write_str("the system's shape does not match its right-hand side")
            }
        }
    }
}

impl core::error::Error for Failure {}

/// A square system, stored by column.
///
/// By column because that is how a basis arrives: one sparse column per basis
/// position. The elimination works on a dense copy built here, once.
#[derive(Clone, Debug)]
pub struct System {
    size: usize,
    /// Row-major dense entries.
    rows: Vec<Vec<Rational>>,
}

impl System {
    /// Builds a system from sparse columns.
    ///
    /// Duplicate entries within one column are summed, which matches how the
    /// model itself treats them.
    ///
    /// # Errors
    ///
    /// Returns [`Failure::Shape`] when a column names a row outside the system
    /// or the column count is not the size.
    pub fn from_columns(size: usize, columns: &[Vec<(u32, Rational)>]) -> Result<Self, Failure> {
        if columns.len() != size {
            return Err(Failure::Shape);
        }
        let mut rows = vec![vec![Rational::zero(); size]; size];
        for (position, column) in columns.iter().enumerate() {
            for (row, value) in column {
                let index = *row as usize;
                if index >= size {
                    return Err(Failure::Shape);
                }
                rows[index][position] = rows[index][position].add(value);
            }
        }
        Ok(Self { size, rows })
    }

    /// The system's size.
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// The transposed system.
    #[must_use]
    pub fn transposed(&self) -> Self {
        let mut rows = vec![vec![Rational::zero(); self.size]; self.size];
        for (index, row) in self.rows.iter().enumerate() {
            for (position, value) in row.iter().enumerate() {
                rows[position][index] = value.clone();
            }
        }
        Self {
            size: self.size,
            rows,
        }
    }

    /// Solves the system exactly.
    ///
    /// # Errors
    ///
    /// Returns [`Failure`] when the right-hand side is the wrong length or the
    /// system is exactly singular.
    pub fn solve(&self, rhs: &[Rational]) -> Result<Vec<Rational>, Failure> {
        if rhs.len() != self.size {
            return Err(Failure::Shape);
        }
        let mut rows = self.rows.clone();
        let mut right = rhs.to_vec();
        // Where each original column ends up, so the answer can be put back in
        // the caller's order after column swaps.
        let mut order: Vec<usize> = (0..self.size).collect();

        for step in 0..self.size {
            // The smallest admissible entry by bit length. Exact arithmetic
            // has no stability to protect, so the only thing a pivot choice
            // can buy is smaller numbers later.
            let mut best: Option<(u64, usize, usize)> = None;
            for (row_index, row) in rows.iter().enumerate().skip(step) {
                for (column_index, value) in row.iter().enumerate().skip(step) {
                    if value.is_zero() {
                        continue;
                    }
                    let weight = value.bit_len();
                    match best {
                        Some((smallest, _, _)) if weight >= smallest => {}
                        _ => best = Some((weight, row_index, column_index)),
                    }
                }
            }
            let Some((_, pivot_row, pivot_column)) = best else {
                return Err(Failure::Singular { at: step });
            };

            rows.swap(step, pivot_row);
            right.swap(step, pivot_row);
            if pivot_column != step {
                for row in &mut rows {
                    row.swap(step, pivot_column);
                }
                order.swap(step, pivot_column);
            }

            let pivot = rows[step][step].clone();
            for index in (step + 1)..self.size {
                let head = rows[index][step].clone();
                if head.is_zero() {
                    continue;
                }
                let factor = head.checked_div(&pivot).expect("the pivot is nonzero");
                // The pivot row is read while the target row is written, so
                // the two are separated before the sweep rather than borrowed
                // at once.
                let pivot_row = rows[step][step..].to_vec();
                for (offset, value) in pivot_row.iter().enumerate() {
                    if value.is_zero() {
                        continue;
                    }
                    let column = step + offset;
                    let scaled = factor.mul(value);
                    rows[index][column] = rows[index][column].sub(&scaled);
                }
                let scaled = factor.mul(&right[step]);
                right[index] = right[index].sub(&scaled);
            }
        }

        // Back substitution.
        let mut answer = vec![Rational::zero(); self.size];
        for step in (0..self.size).rev() {
            let mut total = right[step].clone();
            for (column, value) in answer.iter().enumerate().skip(step + 1) {
                total = total.sub(&rows[step][column].mul(value));
            }
            answer[step] = total
                .checked_div(&rows[step][step])
                .expect("the pivot is nonzero");
        }

        // Undo the column swaps.
        let mut out = vec![Rational::zero(); self.size];
        for (position, original) in order.iter().enumerate() {
            out[*original] = answer[position].clone();
        }
        Ok(out)
    }

    /// The product of the system with a vector, for checking a solve.
    ///
    /// # Errors
    ///
    /// Returns [`Failure::Shape`] when the vector is the wrong length.
    pub fn apply(&self, vector: &[Rational]) -> Result<Vec<Rational>, Failure> {
        if vector.len() != self.size {
            return Err(Failure::Shape);
        }
        Ok(self
            .rows
            .iter()
            .map(|row| {
                let mut total = Rational::zero();
                for (value, coordinate) in row.iter().zip(vector) {
                    if value.is_zero() || coordinate.is_zero() {
                        continue;
                    }
                    total = total.add(&value.mul(coordinate));
                }
                total
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_exact::BigInt;

    fn number(value: i64) -> Rational {
        Rational::from_i64(value)
    }

    fn ratio(top: i64, bottom: i64) -> Rational {
        Rational::new(BigInt::from_i64(top), BigInt::from_i64(bottom)).expect("nonzero")
    }

    /// A system given by rows, which is how a reader thinks of a matrix.
    fn from_rows(rows: &[&[i64]]) -> System {
        let size = rows.len();
        let mut columns = vec![Vec::new(); size];
        for (index, row) in rows.iter().enumerate() {
            for (position, value) in row.iter().enumerate() {
                if *value != 0 {
                    columns[position].push((index as u32, number(*value)));
                }
            }
        }
        System::from_columns(size, &columns).expect("a square system")
    }

    #[test]
    fn the_identity_returns_its_right_hand_side() {
        let system = from_rows(&[&[1, 0], &[0, 1]]);
        let answer = system.solve(&[number(3), number(-4)]).expect("nonsingular");
        assert_eq!(answer, vec![number(3), number(-4)]);
    }

    #[test]
    fn a_solve_reproduces_its_right_hand_side_exactly() {
        // Exactly, not nearly. That is the entire reason this exists beside a
        // floating-point factorization that is faster and almost right.
        let system = from_rows(&[&[2, 1, 0], &[1, 3, 1], &[0, 1, 4]]);
        for target in [
            vec![number(1), number(0), number(0)],
            vec![number(5), number(-2), number(7)],
            vec![ratio(1, 3), ratio(-2, 7), ratio(11, 13)],
        ] {
            let answer = system.solve(&target).expect("nonsingular");
            assert_eq!(system.apply(&answer).expect("right shape"), target);
        }
    }

    #[test]
    fn the_answer_is_the_exact_rational_and_not_a_rounding() {
        // One third has no finite binary expansion, so an implementation that
        // went through a float would be visibly wrong here.
        let system = from_rows(&[&[3]]);
        let answer = system.solve(&[number(1)]).expect("nonsingular");
        assert_eq!(answer[0].to_canonical_string(), "1/3");

        let two = from_rows(&[&[1, 1], &[1, -1]]);
        let answer = two.solve(&[number(1), ratio(1, 3)]).expect("nonsingular");
        assert_eq!(answer[0].to_canonical_string(), "2/3");
        assert_eq!(answer[1].to_canonical_string(), "1/3");
    }

    #[test]
    fn a_transposed_system_solves_the_transposed_problem() {
        let system = from_rows(&[&[2, 1, 0], &[1, 3, 1], &[0, 1, 4]]);
        let target = vec![number(3), number(1), number(-2)];
        let answer = system.transposed().solve(&target).expect("nonsingular");
        assert_eq!(
            system.transposed().apply(&answer).expect("right shape"),
            target
        );

        // And the two solves are adjoint, exactly.
        let left = vec![ratio(1, 2), ratio(-3, 5), number(2)];
        let right = vec![number(1), ratio(2, 7), ratio(-1, 3)];
        let forward = system.solve(&left).expect("nonsingular");
        let backward = system.transposed().solve(&right).expect("nonsingular");
        let one = forward
            .iter()
            .zip(&right)
            .fold(Rational::zero(), |total, (a, b)| total.add(&a.mul(b)));
        let other = left
            .iter()
            .zip(&backward)
            .fold(Rational::zero(), |total, (a, b)| total.add(&a.mul(b)));
        assert_eq!(one, other);
    }

    #[test]
    fn an_exactly_singular_system_is_reported_rather_than_solved() {
        let system = from_rows(&[&[1, 2], &[2, 4]]);
        assert_eq!(
            system.solve(&[number(1), number(2)]),
            Err(Failure::Singular { at: 1 })
        );
        assert!(Failure::Singular { at: 1 }.to_string().contains("singular"));
    }

    #[test]
    fn a_system_that_a_float_would_call_singular_is_solved_exactly() {
        // Rows that differ by one part in a very large number. A floating
        // point factorization sees a rank-one matrix; exact arithmetic sees a
        // perfectly ordinary one and returns the enormous answer it implies.
        let tiny = ratio(1, 1_000_000_000_000_000);
        let mut columns = vec![Vec::new(); 2];
        columns[0].push((0u32, number(1)));
        columns[0].push((1u32, number(1)));
        columns[1].push((0u32, number(1)));
        columns[1].push((1u32, number(1).add(&tiny)));
        let system = System::from_columns(2, &columns).expect("square");
        let answer = system
            .solve(&[number(1), number(1)])
            .expect("nonsingular exactly");
        assert_eq!(
            system.apply(&answer).expect("right shape"),
            vec![number(1), number(1)]
        );
        assert_eq!(answer[1].to_canonical_string(), "0");
    }

    #[test]
    fn column_swaps_are_undone_before_the_answer_is_returned() {
        // A system whose first column is empty forces a column exchange, which
        // is where an implementation that forgets to permute back returns a
        // correctly computed answer in the wrong order.
        let system = from_rows(&[&[0, 3], &[5, 0]]);
        let answer = system.solve(&[number(6), number(10)]).expect("nonsingular");
        assert_eq!(answer, vec![number(2), number(2)]);
        assert_eq!(
            system.apply(&answer).expect("right shape"),
            vec![number(6), number(10)]
        );

        let three = from_rows(&[&[0, 0, 2], &[0, 4, 0], &[7, 0, 0]]);
        let answer = three
            .solve(&[number(2), number(4), number(7)])
            .expect("nonsingular");
        assert_eq!(answer, vec![number(1), number(1), number(1)]);
    }

    #[test]
    fn duplicate_entries_in_one_column_are_summed() {
        let columns = vec![
            vec![(0u32, number(1)), (0u32, number(1))],
            vec![(1u32, number(1))],
        ];
        let system = System::from_columns(2, &columns).expect("square");
        let answer = system.solve(&[number(4), number(1)]).expect("nonsingular");
        assert_eq!(answer, vec![number(2), number(1)]);
    }

    #[test]
    fn a_mismatched_shape_is_reported_rather_than_guessed_at() {
        assert_eq!(System::from_columns(2, &[]).unwrap_err(), Failure::Shape);
        let columns = vec![vec![(5u32, number(1))], vec![(0u32, number(1))]];
        assert_eq!(
            System::from_columns(2, &columns).unwrap_err(),
            Failure::Shape
        );
        let system = from_rows(&[&[1, 0], &[0, 1]]);
        assert_eq!(system.solve(&[number(1)]).unwrap_err(), Failure::Shape);
        assert_eq!(system.apply(&[number(1)]).unwrap_err(), Failure::Shape);
    }

    #[test]
    fn a_system_of_size_zero_is_well_formed() {
        let system = System::from_columns(0, &[]).expect("square");
        assert_eq!(system.size(), 0);
        assert_eq!(
            system.solve(&[]).expect("nonsingular"),
            Vec::<Rational>::new()
        );
    }
}
