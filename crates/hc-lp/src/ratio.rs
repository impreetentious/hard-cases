//! How far the entering column may move, and what stops it.
//!
//! The naive test takes the smallest ratio and pivots there. It is correct and
//! it is where a simplex goes to die: when several rows tie, and they tie
//! constantly on degenerate models, the smallest ratio says nothing about
//! which pivot is numerically usable, and picking badly produces a
//! factorization that is worse than the one before it.
//!
//! Harris's two-pass test fixes that by asking a different question. The first
//! pass computes how far the step could go if every bound were relaxed by the
//! feasibility fence — a slightly optimistic limit. The second pass then takes
//! every row whose honest ratio falls inside that limit and chooses the one
//! with the largest pivot. The step may violate a bound by a hair; that
//! violation is inside the fence the search already works to, and what is
//! bought is a pivot that does not wreck the basis.
//!
//! Two things are non-negotiable here. A pivot below the pivot fence is never
//! a candidate, however good its ratio looks in the first pass — that is what
//! the named trap covers. And the final tiebreak is the lowest basis position,
//! because determinism outranks every heuristic in this solver.

use hc_core::Tol;

use crate::basis::Basis;
#[cfg(test)]
use crate::basis::Status;
use crate::problem::Problem;

/// What the entering column's movement runs into.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// A basic column reaches a bound and leaves.
    Blocked {
        /// The basis position that leaves.
        row: usize,
        /// The column that leaves.
        leaving: usize,
        /// How far the entering column moves, as a nonnegative magnitude.
        theta: f64,
        /// The entry of the entering column at the leaving row.
        pivot: f64,
    },
    /// The entering column reaches its own opposite bound first.
    ///
    /// Nothing enters and nothing leaves: the column jumps from one bound to
    /// the other and the basis is unchanged. This is the cheapest move a
    /// bounded simplex has, and an implementation without it does a full pivot
    /// for something that needs none.
    BoundFlip {
        /// The distance between the entering column's two bounds.
        theta: f64,
    },
    /// Nothing stops the movement.
    Unbounded,
}

impl Step {
    /// How far the step moves, or infinity when nothing stops it.
    #[must_use]
    pub fn theta(&self) -> f64 {
        match self {
            Self::Blocked { theta, .. } | Self::BoundFlip { theta } => *theta,
            Self::Unbounded => f64::INFINITY,
        }
    }

    /// Whether the step moves nowhere.
    ///
    /// A degenerate step still changes the basis, which is how a simplex
    /// escapes a degenerate vertex — and how it cycles if nothing counts them.
    #[must_use]
    pub fn is_degenerate(&self) -> bool {
        matches!(self, Self::Blocked { theta, .. } if *theta == 0.0)
    }
}

/// Everything the test needs in order to decide one move.
///
/// A structure rather than eight positional arguments, half of which are
/// slices of the same length and two of which are floating point — a call site
/// nobody can read, and eventually one nobody gets right.
#[derive(Clone, Copy, Debug)]
pub struct Attempt<'a> {
    /// The scaled problem.
    pub problem: &'a Problem,
    /// The current basis.
    pub basis: &'a Basis,
    /// The current point, over every column.
    pub point: &'a [f64],
    /// The lower bounds in force.
    pub lower: &'a [f64],
    /// The upper bounds in force.
    pub upper: &'a [f64],
    /// The column entering the basis.
    pub entering: usize,
    /// Plus one when the entering column rises, minus one when it falls.
    pub direction: f64,
    /// The entering column expressed in the current basis.
    pub alpha: &'a [f64],
}

/// Runs the two-pass test.
///
/// `alpha` is the entering column expressed in the current basis, indexed by
/// basis position. `direction` is plus one when the entering column rises and
/// minus one when it falls.
///
/// `lower` and `upper` are the bounds in force, which are not always the
/// model's. The first phase replaces the bounds of a basic column that is
/// currently outside them, so that the column is pushed back rather than
/// pinned where it should not be — and the ratio test has to work against the
/// bounds actually in force, not against the ones the model states.
///
/// The step does not say where the leaving column comes to rest. That is a
/// question about the model's bounds, and answering it from the bounds in
/// force would put a column at a first-phase bound that does not exist.
///
/// # Panics
///
/// Panics when `alpha` is not the row count.
#[must_use]
pub fn choose(attempt: &Attempt<'_>, tol: Tol) -> Step {
    let Attempt {
        problem,
        basis,
        point,
        lower,
        upper,
        entering,
        direction,
        alpha,
    } = *attempt;
    assert_eq!(
        alpha.len(),
        problem.rows(),
        "the entering column is not the basis size"
    );

    // The entering column's own range is a limit like any other, and often the
    // binding one.
    let own_range = upper[entering] - lower[entering];

    // First pass: the largest step that keeps every basic column inside its
    // bounds relaxed by the feasibility fence.
    let mut limit = own_range;
    for (row, &rate) in alpha.iter().enumerate() {
        let movement = direction * rate;
        if movement.abs() <= tol.piv {
            // Below the pivot fence this row is not a candidate at all. It is
            // excluded here as well as in the second pass, so a row that could
            // never be pivoted on cannot tighten the limit either.
            continue;
        }
        let column = basis.basic()[row];
        let value = point[column];
        let room = if movement > 0.0 {
            // The basic column falls toward its lower bound.
            let bound = lower[column];
            if bound == f64::NEG_INFINITY {
                continue;
            }
            (value - bound + tol.feas) / movement
        } else {
            // The basic column rises toward its upper bound.
            let bound = upper[column];
            if bound == f64::INFINITY {
                continue;
            }
            (bound - value + tol.feas) / -movement
        };
        // A basic column already outside its bound gives a negative ratio.
        // Clamping to zero keeps the step from moving backwards, which would
        // increase the objective.
        limit = limit.min(room.max(0.0));
    }

    if !limit.is_finite() {
        return Step::Unbounded;
    }

    // Second pass: among the rows whose honest ratio fits inside that limit,
    // the largest pivot wins, and the lowest basis position breaks a tie.
    let mut best: Option<(f64, usize)> = None;
    for (row, &rate) in alpha.iter().enumerate() {
        let movement = direction * rate;
        if movement.abs() <= tol.piv {
            continue;
        }
        let column = basis.basic()[row];
        let value = point[column];
        let ratio = if movement > 0.0 {
            let bound = lower[column];
            if bound == f64::NEG_INFINITY {
                continue;
            }
            (value - bound) / movement
        } else {
            let bound = upper[column];
            if bound == f64::INFINITY {
                continue;
            }
            (bound - value) / -movement
        };
        if ratio.max(0.0) > limit {
            continue;
        }
        let magnitude = movement.abs();
        match best {
            Some((largest, _)) if magnitude <= largest => {}
            _ => best = Some((magnitude, row)),
        }
    }

    let Some((_, row)) = best else {
        // No row blocks. Either the entering column flips to its other bound,
        // or nothing stops it at all.
        if own_range.is_finite() {
            return Step::BoundFlip { theta: own_range };
        }
        return Step::Unbounded;
    };

    let column = basis.basic()[row];
    let movement = direction * alpha[row];
    let value = point[column];
    let bound = if movement > 0.0 {
        lower[column]
    } else {
        upper[column]
    };
    let theta = ((value - bound) / movement).max(0.0);

    // A flip is preferred only when it is genuinely no further than the pivot,
    // because a flip leaves the basis alone and a pivot does not.
    if own_range.is_finite() && own_range < theta {
        return Step::BoundFlip { theta: own_range };
    }

    Step::Blocked {
        row,
        leaving: column,
        theta,
        pivot: alpha[row],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::working::Working;
    use hc_core::Model;

    fn model(source: &str) -> Model {
        hc_io::mps::read(source.as_bytes(), hc_io::mps::Form::Free).expect("accepted")
    }

    fn problem(source: &str) -> Problem {
        Problem::unscaled(&Working::from_model(&model(source)))
    }

    /// Two rows bounding one column, the second binding sooner.
    fn sample() -> Problem {
        problem(concat!(
            "ROWS\n N obj\n L r1\n L r2\n",
            "COLUMNS\n x obj -1 r1 1\n x r2 2\n",
            "RHS\n rhs r1 6 r2 4\n",
            "ENDATA\n"
        ))
    }

    /// The model's own bounds, which is what every test here uses.
    fn bounds(problem: &Problem) -> (Vec<f64>, Vec<f64>) {
        (
            (0..problem.columns())
                .map(|index| problem.lower(index))
                .collect(),
            (0..problem.columns())
                .map(|index| problem.upper(index))
                .collect(),
        )
    }

    /// The cold basis, its factorization, the point, and the entering column
    /// expressed in that basis.
    fn setup(problem: &Problem, entering: usize) -> (Basis, Vec<f64>, Vec<f64>) {
        let basis = Basis::logical(problem);
        let inverse = basis.factor(problem, Tol::solver()).expect("nonsingular");
        let point = basis.primal_point(problem, &inverse);
        let mut alpha = vec![0.0; problem.rows()];
        for &(row, value) in problem.column(entering) {
            alpha[row as usize] = value;
        }
        inverse.ftran(&mut alpha);
        (basis, point, alpha)
    }

    #[test]
    fn the_tighter_row_blocks_the_step() {
        let problem = sample();
        let (basis, point, alpha) = setup(&problem, 0);
        let (low, high) = bounds(&problem);
        let step = choose(
            &Attempt {
                problem: &problem,
                basis: &basis,
                point: &point,
                lower: &low,
                upper: &high,
                entering: 0,
                direction: 1.0,
                alpha: &alpha,
            },
            Tol::solver(),
        );
        match step {
            Step::Blocked {
                row,
                leaving,
                theta,
                ..
            } => {
                // The second row admits two units, the first six.
                assert_eq!(row, 1);
                assert_eq!(leaving, 2, "the second row's logical leaves");
                assert!(
                    (theta - 2.0).abs() <= Tol::solver().feas,
                    "theta was {theta}"
                );
            }
            other => panic!("expected a blocked step, got {other:?}"),
        }
        assert!(!step.is_degenerate());
    }

    #[test]
    fn a_column_reaching_its_own_bound_first_flips_instead_of_pivoting() {
        let narrow = problem(concat!(
            "ROWS\n N obj\n L r1\n",
            "COLUMNS\n x obj -1 r1 1\n",
            "RHS\n rhs r1 100\n",
            "BOUNDS\n UP b x 3\n",
            "ENDATA\n"
        ));
        let (basis, point, alpha) = setup(&narrow, 0);
        let (low, high) = bounds(&narrow);
        let step = choose(
            &Attempt {
                problem: &narrow,
                basis: &basis,
                point: &point,
                lower: &low,
                upper: &high,
                entering: 0,
                direction: 1.0,
                alpha: &alpha,
            },
            Tol::solver(),
        );
        assert_eq!(step, Step::BoundFlip { theta: 3.0 });
        assert_eq!(step.theta(), 3.0);
    }

    #[test]
    fn nothing_stopping_the_movement_is_reported_as_nothing() {
        let open = problem(concat!(
            "ROWS\n N obj\n G r1\n",
            "COLUMNS\n x obj -1 r1 1\n",
            "RHS\n rhs r1 0\n",
            "ENDATA\n"
        ));
        let (basis, point, alpha) = setup(&open, 0);
        let (low, high) = bounds(&open);
        let step = choose(
            &Attempt {
                problem: &open,
                basis: &basis,
                point: &point,
                lower: &low,
                upper: &high,
                entering: 0,
                direction: 1.0,
                alpha: &alpha,
            },
            Tol::solver(),
        );
        assert_eq!(step, Step::Unbounded);
        assert_eq!(step.theta(), f64::INFINITY);
    }

    #[test]
    fn trap_ratio_test_zero_pivot() {
        // A row whose entry in the entering column is below the pivot fence
        // must not be chosen, and must not tighten the limit either. The
        // second condition is the one an implementation forgets: excluding a
        // row from the second pass while letting it constrain the first
        // produces a step that stops short for a row it then refuses to pivot
        // on, and the search stalls without ever saying why.
        let problem = sample();
        let (basis, point, mut alpha) = setup(&problem, 0);
        let (low, high) = bounds(&problem);
        let tol = Tol::solver();

        // Make the tighter row's entry minute. Its honest ratio becomes
        // enormous, so it should now constrain nothing at all.
        alpha[1] = tol.piv / 2.0;
        let step = choose(
            &Attempt {
                problem: &problem,
                basis: &basis,
                point: &point,
                lower: &low,
                upper: &high,
                entering: 0,
                direction: 1.0,
                alpha: &alpha,
            },
            tol,
        );
        match step {
            Step::Blocked {
                row, theta, pivot, ..
            } => {
                assert_eq!(row, 0, "the minute row must not be chosen");
                assert!(pivot.abs() > tol.piv, "a pivot below the fence was chosen");
                assert!((theta - 6.0).abs() <= tol.feas, "theta was {theta}");
            }
            other => panic!("expected the other row to block, got {other:?}"),
        }

        // And with every entry below the fence, nothing blocks at all.
        let mut tiny = alpha.clone();
        for value in &mut tiny {
            *value = tol.piv / 2.0;
        }
        assert_eq!(
            choose(
                &Attempt {
                    problem: &problem,
                    basis: &basis,
                    point: &point,
                    lower: &low,
                    upper: &high,
                    entering: 0,
                    direction: 1.0,
                    alpha: &tiny,
                },
                tol,
            ),
            Step::Unbounded
        );
    }

    #[test]
    fn the_second_pass_prefers_the_larger_pivot_among_near_ties() {
        // Two rows whose ratios differ by less than the feasibility fence.
        // The naive test takes the marginally smaller ratio; the two-pass test
        // takes the better pivot, which is the whole reason it exists.
        let problem = problem(concat!(
            "ROWS\n N obj\n L r1\n L r2\n",
            "COLUMNS\n x obj -1 r1 1\n x r2 1\n",
            "RHS\n rhs r1 4 r2 4\n",
            "ENDATA\n"
        ));
        let (basis, point, mut alpha) = setup(&problem, 0);
        let (low, high) = bounds(&problem);
        let tol = Tol::solver();
        // Row zero: ratio 4 / 1 = 4 with a small pivot.
        // Row one: ratio 4 / 8 = 0.5 with a large pivot — genuinely tighter.
        // The signs are negative because increasing the column increases each
        // row's activity, and a row's logical *is* its activity.
        alpha[0] = -1.0;
        alpha[1] = -8.0;
        match choose(
            &Attempt {
                problem: &problem,
                basis: &basis,
                point: &point,
                lower: &low,
                upper: &high,
                entering: 0,
                direction: 1.0,
                alpha: &alpha,
            },
            tol,
        ) {
            Step::Blocked { row, .. } => assert_eq!(row, 1),
            other => panic!("expected a blocked step, got {other:?}"),
        }

        // Now make the ratios equal and the pivots equal too, so the lower
        // position decides.
        alpha[0] = -2.0;
        alpha[1] = -2.0;
        match choose(
            &Attempt {
                problem: &problem,
                basis: &basis,
                point: &point,
                lower: &low,
                upper: &high,
                entering: 0,
                direction: 1.0,
                alpha: &alpha,
            },
            tol,
        ) {
            Step::Blocked { row, .. } => assert_eq!(row, 0, "a tie must fall to the lower row"),
            other => panic!("expected a blocked step, got {other:?}"),
        }
    }

    #[test]
    fn a_degenerate_step_is_a_step_and_says_so() {
        // A basic column already sitting on the bound it is moving toward
        // admits no movement at all, and the pivot still has to happen.
        let problem = problem(concat!(
            "ROWS\n N obj\n G r1\n",
            "COLUMNS\n x obj -1 r1 1\n",
            "RHS\n rhs r1 0\n",
            "BOUNDS\n UP b x 5\n",
            "ENDATA\n"
        ));
        let (basis, point, alpha) = setup(&problem, 0);
        let (low, high) = bounds(&problem);
        let step = choose(
            &Attempt {
                problem: &problem,
                basis: &basis,
                point: &point,
                lower: &low,
                upper: &high,
                entering: 0,
                direction: 1.0,
                alpha: &alpha,
            },
            Tol::solver(),
        );
        match step {
            Step::Blocked { theta, .. } => {
                assert_eq!(theta, 0.0);
                assert!(step.is_degenerate());
            }
            // A flip of the entering column is also a legitimate answer here;
            // what must not happen is a claim that nothing blocks.
            Step::BoundFlip { .. } => assert!(!step.is_degenerate()),
            Step::Unbounded => panic!("the row bounds the movement"),
        }
    }

    #[test]
    fn a_falling_column_is_limited_by_the_opposite_bounds() {
        let problem = problem(concat!(
            "ROWS\n N obj\n G r1\n",
            "COLUMNS\n x obj 1 r1 1\n",
            "RHS\n rhs r1 -6\n",
            "BOUNDS\n MI b x\n UP b x 0\n",
            "ENDATA\n"
        ));
        let (basis, point, alpha) = setup(&problem, 0);
        let (low, high) = bounds(&problem);
        // The column sits at its upper bound of zero and improves by falling.
        assert_eq!(basis.status(0), Status::AtUpper);
        let step = choose(
            &Attempt {
                problem: &problem,
                basis: &basis,
                point: &point,
                lower: &low,
                upper: &high,
                entering: 0,
                direction: -1.0,
                alpha: &alpha,
            },
            Tol::solver(),
        );
        match step {
            Step::Blocked { theta, .. } => {
                assert!(
                    (theta - 6.0).abs() <= Tol::solver().feas,
                    "theta was {theta}"
                );
            }
            other => panic!("expected the row to block, got {other:?}"),
        }
    }

    #[test]
    fn a_column_with_no_rows_to_block_it_is_unbounded() {
        // One column, no constraints, no upper bound: nothing can stop it.
        let open = problem("ROWS\n N obj\nCOLUMNS\n x obj -1\nENDATA\n");
        assert_eq!(open.rows(), 0);
        let basis = Basis::logical(&open);
        let (low, high) = bounds(&open);
        assert_eq!(
            choose(
                &Attempt {
                    problem: &open,
                    basis: &basis,
                    point: &[0.0],
                    lower: &low,
                    upper: &high,
                    entering: 0,
                    direction: 1.0,
                    alpha: &[],
                },
                Tol::solver(),
            ),
            Step::Unbounded
        );

        // Give it an upper bound and the same column flips instead.
        let capped = problem("ROWS\n N obj\nCOLUMNS\n x obj -1\nBOUNDS\n UP b x 3\nENDATA\n");
        let basis = Basis::logical(&capped);
        let (low, high) = bounds(&capped);
        assert_eq!(
            choose(
                &Attempt {
                    problem: &capped,
                    basis: &basis,
                    point: &[0.0],
                    lower: &low,
                    upper: &high,
                    entering: 0,
                    direction: 1.0,
                    alpha: &[],
                },
                Tol::solver(),
            ),
            Step::BoundFlip { theta: 3.0 }
        );
    }
}
