//! The dual simplex, over bounded variables.
//!
//! The primal method keeps every basic variable inside its bounds and works to
//! remove the columns that price better. The dual method keeps every nonbasic
//! column priced correctly and works to bring the basic variables inside their
//! bounds. Same pivots, opposite invariant, and the swap is the entire reason
//! it exists here.
//!
//! A branch changes one bound and nothing else. The parent's optimal basis
//! therefore still prices every column correctly — the reduced costs depend on
//! the matrix and the objective, and neither moved — while one basic variable
//! is now outside the tightened bound. That is exactly a dual-feasible start
//! with a little primal infeasibility, which is the situation this method is
//! for. Solving a child from cold instead means rediscovering the parent's
//! answer before improving on it.
//!
//! Two things are worth stating about what happens when it fails. A dual
//! method that runs out of admissible entering columns has proved the primal
//! *infeasible*, and the row it stopped on is the witness — that is a real
//! answer, not a breakdown. And a start that is not dual feasible is refused up
//! front rather than repaired, because repairing it silently would mean the
//! caller believing it warm-started something it did not.

use hc_core::Tol;

use crate::basis::{Basis, Status};
use crate::inverse::Inverse;
use crate::problem::Problem;
use crate::simplex::{Effort, Options, Outcome};

/// Why a basis could not be used as a dual start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotDual {
    /// A nonbasic column prices in the improving direction.
    ///
    /// Moving it would lower the objective, which means the basis is not dual
    /// feasible and the method's invariant does not hold at the start.
    Priced {
        /// The offending column.
        column: usize,
    },
    /// A nonbasic column sits at a bound the model does not have.
    Unbounded {
        /// The offending column.
        column: usize,
    },
    /// The basis is singular.
    Singular,
}

impl core::fmt::Display for NotDual {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Priced { column } => {
                write!(
                    formatter,
                    "column {column} prices in the improving direction"
                )
            }
            Self::Unbounded { column } => {
                write!(
                    formatter,
                    "column {column} rests at a bound it does not have"
                )
            }
            Self::Singular => formatter.write_str("the basis is singular"),
        }
    }
}

impl core::error::Error for NotDual {}

/// Whether a basis is dual feasible for a problem.
///
/// # Errors
///
/// Returns [`NotDual`] naming the first column that breaks the invariant.
pub fn is_dual_feasible(problem: &Problem, basis: &Basis, tol: Tol) -> Result<(), NotDual> {
    let inverse = basis.factor(problem, tol).map_err(|_| NotDual::Singular)?;
    let multipliers = basis.multipliers(problem, &inverse);
    check_pricing(problem, basis, &multipliers, tol)
}

/// The pricing half of the check, given multipliers already in hand.
fn check_pricing(
    problem: &Problem,
    basis: &Basis,
    multipliers: &[f64],
    tol: Tol,
) -> Result<(), NotDual> {
    for column in 0..problem.columns() {
        let cost = basis.reduced_cost(problem, multipliers, column);
        match basis.status(column) {
            Status::Basic | Status::Fixed => {}
            // A column at its lower bound may only rise, so a negative reduced
            // cost would improve the objective and the basis is not optimal
            // for the dual.
            Status::AtLower => {
                if cost < -tol.opt {
                    return Err(NotDual::Priced { column });
                }
                if !problem.lower(column).is_finite() {
                    return Err(NotDual::Unbounded { column });
                }
            }
            Status::AtUpper => {
                if cost > tol.opt {
                    return Err(NotDual::Priced { column });
                }
                if !problem.upper(column).is_finite() {
                    return Err(NotDual::Unbounded { column });
                }
            }
            // A free column must price at zero from either side.
            Status::Free => {
                if cost.abs() > tol.opt {
                    return Err(NotDual::Priced { column });
                }
            }
        }
    }
    Ok(())
}

/// Solves a problem by the dual method, from a dual-feasible basis.
///
/// # Errors
///
/// Returns [`NotDual`] when the starting basis does not satisfy the
/// invariant. The caller decides what to do about that; this method will not
/// quietly do something else.
pub fn solve_from(
    problem: &Problem,
    start: Basis,
    options: &Options,
) -> Result<(Outcome, Effort), NotDual> {
    let tol = options.tol;
    let mut basis = start;
    let mut effort = Effort::default();

    let mut inverse = basis.factor(problem, tol).map_err(|_| NotDual::Singular)?;
    {
        let multipliers = basis.multipliers(problem, &inverse);
        check_pricing(problem, &basis, &multipliers, tol)?;
    }

    loop {
        if effort.iterations >= options.iteration_limit {
            let point = basis.primal_point(problem, &inverse);
            let feasible = basis.is_primal_feasible(problem, &point, tol);
            let objective = problem.objective_at(&point);
            return Ok((
                Outcome::EffortLimit {
                    basis,
                    point,
                    objective,
                    feasible,
                },
                effort,
            ));
        }

        let point = basis.primal_point(problem, &inverse);
        let multipliers = basis.multipliers(problem, &inverse);

        // The leaving row: the basic variable furthest outside its bounds.
        // Ties go to the lowest position, which is the project's final
        // tie-break everywhere.
        let Some(leaving) = worst_violation(problem, &basis, &point, tol) else {
            let objective = problem.objective_at(&point);
            return Ok((
                Outcome::Optimal {
                    basis,
                    point,
                    objective,
                    multipliers,
                },
                effort,
            ));
        };

        let row = leaving.row;
        // The direction the basic variable must move to re-enter its bounds.
        // Below its lower bound it must rise; above its upper bound it must
        // fall.
        let rises = leaving.below;

        // The pivot row of the transformed matrix, which is what decides the
        // entering column.
        let pivot_row = transformed_row(problem, &basis, &inverse, row);

        let Some(entering) = dual_ratio(problem, &basis, &multipliers, &pivot_row, rises, tol)
        else {
            // No admissible entering column. The row is a witness that the
            // primal has no feasible point, and the multipliers of that row
            // are the certificate candidate.
            let mut unit = vec![0.0; problem.rows()];
            unit[row] = if rises { -1.0 } else { 1.0 };
            inverse.btran(&mut unit);
            return Ok((
                Outcome::Infeasible {
                    basis,
                    infeasibility: leaving.amount,
                    multipliers: unit,
                },
                effort,
            ));
        };

        // The entering column, transformed, which is what the basis update
        // needs and what gives the pivot element.
        let mut alpha = vec![0.0; problem.rows()];
        for &(at, value) in problem.column(entering) {
            alpha[at as usize] = value;
        }
        inverse.ftran(&mut alpha);
        if alpha[row].abs() <= tol.piv {
            // The ratio test admitted a pivot the factorization cannot use.
            // Refactor and try again rather than dividing by it.
            match basis.factor(problem, tol) {
                Ok(fresh) => {
                    inverse = fresh;
                    effort.refactorizations += 1;
                    effort.iterations += 1;
                    continue;
                }
                Err(_) => {
                    return Ok((
                        Outcome::Numerical {
                            basis,
                            note: String::from("the dual pivot element vanished"),
                        },
                        effort,
                    ));
                }
            }
        }

        // The leaving column rests at whichever bound it was outside of.
        let rests_at = if rises {
            Status::AtLower
        } else {
            Status::AtUpper
        };
        basis.pivot(row, entering, rests_at);

        if inverse.update(row, &alpha, tol).is_err() {
            match basis.factor(problem, tol) {
                Ok(fresh) => {
                    inverse = fresh;
                    effort.refactorizations += 1;
                }
                Err(_) => {
                    return Ok((
                        Outcome::Numerical {
                            basis,
                            note: String::from("the basis became unusable"),
                        },
                        effort,
                    ));
                }
            }
        }
        effort.iterations += 1;
    }
}

/// A basic variable outside its bounds.
struct Violation {
    /// Its position in the basis.
    row: usize,
    /// Whether it is below its lower bound rather than above its upper.
    below: bool,
    /// How far outside it is.
    amount: f64,
}

/// The basic variable furthest outside its bounds.
fn worst_violation(problem: &Problem, basis: &Basis, point: &[f64], tol: Tol) -> Option<Violation> {
    let mut worst: Option<Violation> = None;
    for (row, column) in basis.basic().iter().enumerate() {
        let value = point[*column];
        let low = problem.lower(*column);
        let high = problem.upper(*column);
        let (below, amount) = if value < low - tol.feas {
            (true, low - value)
        } else if value > high + tol.feas {
            (false, value - high)
        } else {
            continue;
        };
        let better = worst.as_ref().is_none_or(|seen| amount > seen.amount);
        if better {
            worst = Some(Violation { row, below, amount });
        }
    }
    worst
}

/// One row of the basis-transformed matrix.
///
/// A transposed solve against a unit vector gives the row multipliers, and one
/// pass over the columns turns those into the row itself.
fn transformed_row(problem: &Problem, basis: &Basis, inverse: &Inverse, row: usize) -> Vec<f64> {
    let mut unit = vec![0.0; problem.rows()];
    unit[row] = 1.0;
    inverse.btran(&mut unit);

    let mut out = vec![0.0; problem.columns()];
    for (column, slot) in out.iter_mut().enumerate() {
        if basis.status(column).is_basic() {
            continue;
        }
        let mut total = 0.0;
        for &(at, value) in problem.column(column) {
            total += unit[at as usize] * value;
        }
        *slot = total;
    }
    out
}

/// The dual ratio test: which nonbasic column enters.
///
/// A nonbasic column is admissible when moving it in its permitted direction
/// moves the leaving basic variable the way it needs to go. Among those, the
/// one whose reduced cost divided by its pivot element is smallest in
/// magnitude is the one that can enter without any other column's reduced cost
/// crossing zero — which is what keeps the basis dual feasible.
fn dual_ratio(
    problem: &Problem,
    basis: &Basis,
    multipliers: &[f64],
    pivot_row: &[f64],
    rises: bool,
    tol: Tol,
) -> Option<usize> {
    let mut best: Option<(usize, f64, f64)> = None;

    for (column, alpha) in pivot_row.iter().enumerate().take(problem.columns()) {
        if basis.status(column).is_basic() || problem.is_fixed(column) {
            continue;
        }
        let alpha = *alpha;
        if alpha.abs() <= tol.piv {
            continue;
        }

        // The sign the basic variable moves in when this column increases.
        // Increasing a nonbasic column by one changes the leaving basic
        // variable by minus its coefficient in the transformed row.
        let effect = -alpha;
        let admissible = match basis.status(column) {
            // A column at its lower bound may only rise.
            Status::AtLower => (effect > 0.0) == rises,
            // A column at its upper bound may only fall, which reverses the
            // effect.
            Status::AtUpper => (effect < 0.0) == rises,
            // A free column may move either way, so it is always admissible.
            Status::Free => true,
            _ => false,
        };
        if !admissible {
            continue;
        }

        let cost = basis.reduced_cost(problem, multipliers, column);
        let ratio = (cost / alpha).abs();
        let better = match &best {
            None => true,
            // Ties go to the larger pivot, which is the numerically safer
            // choice, and then to the lowest index.
            Some((_, seen, magnitude)) => {
                ratio < *seen - tol.opt
                    || ((ratio - *seen).abs() <= tol.opt && alpha.abs() > *magnitude)
            }
        };
        if better {
            best = Some((column, ratio, alpha.abs()));
        }
    }

    best.map(|(column, _, _)| column)
}
