//! The basis inverse: a factorization plus the updates made since it was taken.
//!
//! A simplex changes one basis column per iteration. Refactoring from scratch
//! each time would dominate the run, so the factorization is kept and each
//! change is recorded as an elementary transformation applied on top of it.
//! Every solve then costs the base factorization plus one pass over the
//! updates, and the updates are discarded when they get expensive or when the
//! drift they accumulate gets large.
//!
//! The representation is the product form: each update stores the entering
//! column expressed in the *current* basis, which is exactly the vector the
//! iteration already computed in order to decide the pivot. Nothing extra is
//! calculated to build one. The alternative — rewriting the upper factor and
//! storing a row transformation instead — keeps the update file sparser on
//! long runs, and it is recorded as a deliberate deferral rather than an
//! oversight in ADR-0007.
//!
//! Everything here is floating point and everything here drifts. That is why
//! the drift is measured against the original columns rather than against the
//! factors, why the policy for starting again is explicit and named, and why
//! nothing this module produces is ever a claim.

use hc_core::Tol;

use crate::lu::{Factorization, Singular};

/// How many updates may accumulate before the basis is factored again.
///
/// Fixed rather than tuned. A refactorization interval that adapts to the
/// instance makes the search's arithmetic depend on the instance's history,
/// and two runs that took different routes to the same basis would then
/// compute different numbers from it.
pub const REFACTOR_INTERVAL: usize = 100;

/// The fraction of the feasibility fence a residual may reach before the basis
/// is factored again immediately.
///
/// A tenth, so that drift is caught while the computed solution is still well
/// inside the fence rather than at the moment it crosses it. A trigger that
/// fires at the fence is a trigger that fires after the damage.
pub const DRIFT_FRACTION: f64 = 0.1;

/// An update that could not be applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unstable {
    /// The basis position that was being replaced.
    pub position: usize,
    /// The pivot magnitude that was refused.
    pub pivot: f64,
}

impl core::fmt::Display for Unstable {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "basis update at position {} has pivot {}, which is too small to use",
            self.position, self.pivot
        )
    }
}

impl core::error::Error for Unstable {}

/// One elementary transformation.
#[derive(Clone, Debug)]
struct Eta {
    /// The basis position whose column was replaced.
    position: usize,
    /// The entering column in the basis coordinates of the moment, with the
    /// pivot entry removed.
    off_pivot: Vec<(u32, f64)>,
    /// The entry at the replaced position.
    pivot: f64,
}

/// A factored basis together with the updates applied since.
#[derive(Clone, Debug)]
pub struct Inverse {
    base: Factorization,
    updates: Vec<Eta>,
    /// Stored coefficients at the moment of the last factorization, so growth
    /// can be measured rather than guessed at.
    base_nonzeros: usize,
}

impl Inverse {
    /// Factors a basis and starts an empty update file.
    ///
    /// # Errors
    ///
    /// Returns [`Singular`] when the basis cannot be factored.
    pub fn factor(columns: &[Vec<(u32, f64)>], size: usize, tol: Tol) -> Result<Self, Singular> {
        let base = Factorization::build(columns, size, tol)?;
        let base_nonzeros = base.nonzeros();
        Ok(Self {
            base,
            updates: Vec::new(),
            base_nonzeros,
        })
    }

    /// The basis size.
    #[must_use]
    pub fn size(&self) -> usize {
        self.base.size()
    }

    /// How many updates have accumulated.
    #[must_use]
    pub fn updates(&self) -> usize {
        self.updates.len()
    }

    /// The stored coefficients in the factors and the updates together.
    #[must_use]
    pub fn nonzeros(&self) -> usize {
        self.base.nonzeros()
            + self
                .updates
                .iter()
                .map(|eta| eta.off_pivot.len() + 1)
                .sum::<usize>()
    }

    /// The factorization underneath the updates.
    #[must_use]
    pub fn base(&self) -> &Factorization {
        &self.base
    }

    /// Solves `B x = rhs` in place, where `B` is the updated basis.
    ///
    /// On entry `rhs` is indexed by row; on return it is indexed by basis
    /// position.
    ///
    /// # Panics
    ///
    /// Panics when the vector is not the basis size.
    pub fn ftran(&self, rhs: &mut [f64]) {
        self.base.ftran(rhs);
        for eta in &self.updates {
            let value = rhs[eta.position] / eta.pivot;
            rhs[eta.position] = value;
            if value == 0.0 {
                continue;
            }
            for &(index, coefficient) in &eta.off_pivot {
                rhs[index as usize] -= coefficient * value;
            }
        }
    }

    /// Solves `B^T y = rhs` in place, where `B` is the updated basis.
    ///
    /// On entry `rhs` is indexed by basis position; on return it is indexed by
    /// row. The updates are applied in the reverse of the order they were made,
    /// because a transpose reverses a product.
    ///
    /// # Panics
    ///
    /// Panics when the vector is not the basis size.
    pub fn btran(&self, rhs: &mut [f64]) {
        for eta in self.updates.iter().rev() {
            let mut total = rhs[eta.position];
            for &(index, coefficient) in &eta.off_pivot {
                total -= coefficient * rhs[index as usize];
            }
            rhs[eta.position] = total / eta.pivot;
        }
        self.base.btran(rhs);
    }

    /// Records that the column at `position` has been replaced.
    ///
    /// `entering` is the entering column already expressed in the current
    /// basis — the vector a simplex iteration computes in order to choose the
    /// pivot in the first place, so an update costs nothing beyond storing it.
    ///
    /// # Errors
    ///
    /// Returns [`Unstable`] when the pivot entry is too small to divide by. The
    /// caller's recourse is to factor again and retry, which is the first rung
    /// of the escalation ladder rather than a failure.
    ///
    /// # Panics
    ///
    /// Panics when the vector is not the basis size.
    pub fn update(&mut self, position: usize, entering: &[f64], tol: Tol) -> Result<(), Unstable> {
        assert_eq!(
            entering.len(),
            self.size(),
            "vector length differs from the basis size"
        );
        let pivot = entering[position];
        if pivot.abs() <= tol.piv {
            return Err(Unstable { position, pivot });
        }
        let off_pivot = entering
            .iter()
            .enumerate()
            .filter(|(index, value)| *index != position && value.abs() > tol.zero)
            .map(|(index, value)| (index as u32, *value))
            .collect();
        self.updates.push(Eta {
            position,
            off_pivot,
            pivot,
        });
        Ok(())
    }

    /// Whether the basis should be factored again before the next iteration.
    ///
    /// Two reasons, both explicit. The update file has reached its interval,
    /// or a measured residual has reached a fraction of the feasibility fence.
    /// Neither is a heuristic about how the solve is going; both are about how
    /// much the arithmetic can still be trusted.
    #[must_use]
    pub fn should_refactor(&self, residual: f64, tol: Tol) -> Option<Reason> {
        if residual > tol.feas * DRIFT_FRACTION {
            return Some(Reason::Drift);
        }
        if self.updates.len() >= REFACTOR_INTERVAL {
            return Some(Reason::Interval);
        }
        // A file that has outgrown the factorization underneath it costs more
        // to apply than the factorization does, at which point keeping it is
        // no longer an economy.
        if self.base_nonzeros > 0 && self.nonzeros() > self.base_nonzeros * 4 {
            return Some(Reason::Growth);
        }
        None
    }
}

/// Why a basis is being factored again.
///
/// Named rather than boolean, because the reason belongs in the trace: a run
/// that refactors on drift every few iterations is telling a different story
/// from one that refactors on the interval, and a boolean hides which.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The update file reached its interval.
    Interval,
    /// A measured residual reached a fraction of the feasibility fence.
    Drift,
    /// The update file outgrew the factorization underneath it.
    Growth,
}

impl Reason {
    /// The word this reason appears as in a trace.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Interval => "interval",
            Self::Drift => "drift",
            Self::Growth => "growth",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tol() -> Tol {
        Tol::solver()
    }

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

    /// Replaces one column of a basis and returns the new one.
    fn replaced(
        columns: &[Vec<(u32, f64)>],
        position: usize,
        column: &[(u32, f64)],
    ) -> Vec<Vec<(u32, f64)>> {
        let mut out = columns.to_vec();
        out[position] = column.to_vec();
        out
    }

    #[test]
    fn an_unchanged_basis_solves_as_its_factorization_does() {
        let columns = from_dense(&[&[2.0, 1.0], &[1.0, 3.0]]);
        let inverse = Inverse::factor(&columns, 2, tol()).expect("nonsingular");
        assert_eq!(inverse.updates(), 0);
        let target = [4.0, 7.0];
        let mut solution = target;
        inverse.ftran(&mut solution);
        assert!(close(&multiply(&columns, &solution), &target, tol().feas));
    }

    #[test]
    fn one_update_matches_factoring_the_replacement_from_scratch() {
        let columns = from_dense(&[&[2.0, 1.0, 0.0], &[1.0, 3.0, 1.0], &[0.0, 1.0, 4.0]]);
        let mut inverse = Inverse::factor(&columns, 3, tol()).expect("nonsingular");

        // The entering column, in row coordinates, and the same column
        // expressed in the current basis — which is what the update takes.
        let entering_column = vec![(0u32, 1.0), (1u32, -2.0), (2u32, 3.0)];
        let mut alpha = [0.0; 3];
        for &(row, value) in &entering_column {
            alpha[row as usize] = value;
        }
        inverse.ftran(&mut alpha);

        let position = 1usize;
        inverse
            .update(position, &alpha, tol())
            .expect("a usable pivot");
        assert_eq!(inverse.updates(), 1);

        let replacement = replaced(&columns, position, &entering_column);
        let fresh = Inverse::factor(&replacement, 3, tol()).expect("nonsingular");

        for target in [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [5.0, -2.0, 7.0],
        ] {
            let mut updated = target;
            inverse.ftran(&mut updated);
            let mut refactored = target;
            fresh.ftran(&mut refactored);
            assert!(
                close(&updated, &refactored, tol().feas),
                "forward, for {target:?}"
            );
            assert!(close(
                &multiply(&replacement, &updated),
                &target,
                tol().feas
            ));

            let mut updated_dual = target;
            inverse.btran(&mut updated_dual);
            let mut refactored_dual = target;
            fresh.btran(&mut refactored_dual);
            assert!(
                close(&updated_dual, &refactored_dual, tol().feas),
                "transposed"
            );
            assert!(close(
                &transpose_multiply(&replacement, &updated_dual),
                &target,
                tol().feas
            ));
        }
    }

    #[test]
    fn a_chain_of_updates_matches_factoring_the_result() {
        // Six replacements in a row, which is where an update scheme that gets
        // the ordering of its transformations backwards still passes a
        // single-update test and fails everything after it.
        let size = 5usize;
        let mut columns: Vec<Vec<(u32, f64)>> = (0..size)
            .map(|index| vec![(index as u32, 2.0 + index as f64)])
            .collect();
        let mut inverse = Inverse::factor(&columns, size, tol()).expect("nonsingular");

        let entering: [(usize, [f64; 5]); 6] = [
            (0, [1.0, 1.0, 0.0, 0.0, 0.0]),
            (3, [0.0, 2.0, 1.0, 1.0, 0.0]),
            (1, [1.0, 0.0, 0.0, 3.0, 1.0]),
            (4, [0.0, 0.0, 2.0, 1.0, 1.0]),
            (2, [1.0, 1.0, 1.0, 0.0, 0.0]),
            (0, [2.0, 0.0, 1.0, 0.0, 1.0]),
        ];

        for (step, (position, dense)) in entering.into_iter().enumerate() {
            let column: Vec<(u32, f64)> = dense
                .iter()
                .enumerate()
                .filter(|(_, value)| **value != 0.0)
                .map(|(row, value)| (row as u32, *value))
                .collect();

            let mut alpha = dense;
            inverse.ftran(&mut alpha);
            inverse
                .update(position, &alpha, tol())
                .expect("a usable pivot");
            columns = replaced(&columns, position, &column);

            let fresh = Inverse::factor(&columns, size, tol()).expect("nonsingular");
            for probe in 0..size {
                let mut unit = vec![0.0; size];
                unit[probe] = 1.0;
                let mut updated = unit.clone();
                inverse.ftran(&mut updated);
                let mut refactored = unit.clone();
                fresh.ftran(&mut refactored);
                assert!(
                    close(&updated, &refactored, tol().feas),
                    "step {step}, probe {probe}: forward solves diverged"
                );

                let mut updated_dual = unit.clone();
                inverse.btran(&mut updated_dual);
                let mut refactored_dual = unit;
                fresh.btran(&mut refactored_dual);
                assert!(
                    close(&updated_dual, &refactored_dual, tol().feas),
                    "step {step}, probe {probe}: transposed solves diverged"
                );
            }
        }
        assert_eq!(inverse.updates(), 6);
    }

    #[test]
    fn the_two_solves_stay_adjoint_across_updates() {
        let columns = from_dense(&[&[3.0, 1.0, 0.0], &[0.0, 2.0, 1.0], &[1.0, 0.0, 4.0]]);
        let mut inverse = Inverse::factor(&columns, 3, tol()).expect("nonsingular");
        let mut alpha = [1.0, 1.0, 1.0];
        inverse.ftran(&mut alpha);
        inverse.update(2, &alpha, tol()).expect("a usable pivot");

        let left = [0.5, -1.5, 2.0];
        let right = [1.0, 2.0, -3.0];
        let mut forward = left;
        inverse.ftran(&mut forward);
        let one: f64 = forward.iter().zip(&right).map(|(a, b)| a * b).sum();
        let mut backward = right;
        inverse.btran(&mut backward);
        let other: f64 = left.iter().zip(&backward).map(|(a, b)| a * b).sum();
        assert!((one - other).abs() <= tol().feas);
    }

    #[test]
    fn an_unusable_pivot_is_refused_rather_than_divided_by() {
        let columns = from_dense(&[&[1.0, 0.0], &[0.0, 1.0]]);
        let mut inverse = Inverse::factor(&columns, 2, tol()).expect("nonsingular");
        // An entering column with nothing at the replaced position would make
        // the new basis singular, and dividing by it would produce an inverse
        // whose entries are enormous and whose solutions are noise.
        let refusal = inverse.update(0, &[0.0, 1.0], tol()).expect_err("no pivot");
        assert_eq!(refusal.position, 0);
        assert!(refusal.to_string().contains("too small"));
        assert_eq!(
            inverse.updates(),
            0,
            "a refused update must not be recorded"
        );

        // And one just below the fence is refused for the same reason.
        assert!(inverse.update(0, &[tol().piv / 2.0, 1.0], tol()).is_err());
        assert!(inverse.update(0, &[tol().piv * 10.0, 1.0], tol()).is_ok());
    }

    #[test]
    fn the_refactorization_policy_names_its_reason() {
        let columns = from_dense(&[&[1.0, 0.0], &[0.0, 1.0]]);
        let mut inverse = Inverse::factor(&columns, 2, tol()).expect("nonsingular");
        assert_eq!(inverse.should_refactor(0.0, tol()), None);

        // Drift is checked first, because it is the reason that matters.
        assert_eq!(
            inverse.should_refactor(tol().feas, tol()),
            Some(Reason::Drift)
        );
        assert_eq!(
            inverse.should_refactor(tol().feas * DRIFT_FRACTION, tol()),
            None
        );

        for _ in 0..REFACTOR_INTERVAL {
            let mut alpha = [1.0, 0.0];
            inverse.ftran(&mut alpha);
            inverse.update(0, &alpha, tol()).expect("a usable pivot");
        }
        assert_eq!(inverse.should_refactor(0.0, tol()), Some(Reason::Interval));
        assert_eq!(Reason::Interval.as_str(), "interval");
        assert_eq!(Reason::Drift.as_str(), "drift");
        assert_eq!(Reason::Growth.as_str(), "growth");
    }

    #[test]
    fn a_growing_update_file_is_a_reason_of_its_own() {
        // A dense entering column against a diagonal basis: each update stores
        // as much as the whole factorization did.
        let size = 6usize;
        let columns: Vec<Vec<(u32, f64)>> =
            (0..size).map(|index| vec![(index as u32, 1.0)]).collect();
        let mut inverse = Inverse::factor(&columns, size, tol()).expect("nonsingular");
        assert_eq!(inverse.nonzeros(), size);

        for step in 0..5usize {
            // Each entering column is dense and distinct, so the basis stays
            // nonsingular while every update stores a full column.
            let mut dense = vec![1.0; size];
            dense[step] += 3.0 + step as f64;
            inverse.ftran(&mut dense);
            inverse.update(step, &dense, tol()).expect("a usable pivot");
        }
        assert!(inverse.nonzeros() > size * 4);
        assert_eq!(inverse.should_refactor(0.0, tol()), Some(Reason::Growth));
    }

    #[test]
    fn a_basis_of_size_zero_updates_vacuously() {
        let inverse = Inverse::factor(&[], 0, tol()).expect("nonsingular");
        assert_eq!(inverse.size(), 0);
        assert_eq!(inverse.updates(), 0);
        assert_eq!(inverse.should_refactor(0.0, tol()), None);
    }
}
