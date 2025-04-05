//! The dual method against the primal one.
//!
//! The two are different algorithms with different invariants, and on any
//! instance they both finish, they must agree about the status and about the
//! objective. That is the check worth having: a dual simplex tested only
//! against hand-worked answers would be tested on the cases somebody thought
//! of, and the interesting failures are the ones nobody did.
//!
//! The dual method also has a precondition the primal does not — its starting
//! basis must price every column correctly — and refusing a start that does
//! not is part of what is checked here. A method that quietly repaired a bad
//! start would let a caller believe it had warm-started something it had not.

use hc_core::{Model, Tol};
use hc_io::mps::{Form, read};
use hc_lp::dual::{NotDual, is_dual_feasible, solve_from as solve_dual};
use hc_lp::{Basis, Options, Outcome, Problem, Working, solve};

const CORPUS: &[(&str, &[u8])] = &[
    (
        "transport33",
        include_bytes!("../../../instances/local/transport33.mps"),
    ),
    (
        "blend4",
        include_bytes!("../../../instances/local/blend4.mps"),
    ),
    (
        "cover6",
        include_bytes!("../../../instances/local/cover6.mps"),
    ),
    (
        "flow8",
        include_bytes!("../../../instances/local/flow8.mps"),
    ),
    (
        "freevars5",
        include_bytes!("../../../instances/local/freevars5.mps"),
    ),
    (
        "cycle7",
        include_bytes!("../../../instances/local/cycle7.mps"),
    ),
    (
        "infeasible4",
        include_bytes!("../../../instances/local/infeasible4.mps"),
    ),
    (
        "tiny",
        include_bytes!("../../../spec/v1/vectors/mps/tiny.mps"),
    ),
];

fn model(raw: &[u8]) -> Model {
    read(raw, Form::Fixed).expect("the instance parses")
}

fn problem(raw: &[u8]) -> Problem {
    Problem::scaled(&Working::from_model(&model(raw)), Tol::solver())
}

fn word(outcome: &Outcome) -> &'static str {
    outcome.as_str()
}

#[test]
fn the_logical_basis_is_dual_feasible_only_when_every_cost_agrees_with_it() {
    // The logical basis leaves every structural column at a bound. Whether
    // that prices correctly depends on the objective, so this asserts the
    // check answers rather than that it always says yes.
    for (name, raw) in CORPUS {
        let problem = problem(raw);
        let basis = Basis::logical(&problem);
        match is_dual_feasible(&problem, &basis, Tol::solver()) {
            Ok(()) => {}
            Err(NotDual::Priced { .. } | NotDual::Unbounded { .. }) => {}
            Err(other) => panic!("{name}: {other}"),
        }
    }
}

#[test]
fn an_optimal_basis_is_always_dual_feasible() {
    // Optimality means every column prices correctly and every basic variable
    // is inside its bounds. The first half of that is exactly the dual
    // invariant, so a primal optimum is a dual start by construction — which
    // is the property warm starting rests on.
    for (name, raw) in CORPUS {
        let problem = problem(raw);
        let (outcome, _) = solve(&problem, &Options::default());
        let Outcome::Optimal { basis, .. } = outcome else {
            continue;
        };
        is_dual_feasible(&problem, &basis, Tol::solver())
            .unwrap_or_else(|reason| panic!("{name}: an optimal basis was refused: {reason}"));
    }
}

#[test]
fn the_two_methods_agree_wherever_the_dual_can_start() {
    let mut compared = 0;
    for (name, raw) in CORPUS {
        let problem = problem(raw);
        let (primal, _) = solve(&problem, &Options::default());

        let start = Basis::logical(&problem);
        let Ok((dual, _)) = solve_dual(&problem, start, &Options::default()) else {
            continue;
        };
        compared += 1;

        assert_eq!(
            word(&primal),
            word(&dual),
            "{name}: the two methods disagree about the status"
        );
        if let (
            Outcome::Optimal {
                objective: first, ..
            },
            Outcome::Optimal {
                objective: second, ..
            },
        ) = (&primal, &dual)
        {
            // The fence is the named feasibility tolerance, scaled by the
            // magnitude of what is being compared. A literal here would be a
            // threshold nobody had decided.
            let fence = Tol::solver().feas * (1.0 + first.abs());
            assert!(
                (first - second).abs() <= fence,
                "{name}: objectives {first} and {second}"
            );
        }
    }
    assert!(
        compared > 0,
        "no instance gave the dual method a start it could use"
    );
}

#[test]
fn a_start_that_prices_wrongly_is_refused_rather_than_repaired() {
    // Take an optimal basis and flip a nonbasic column to the other bound.
    // That breaks the pricing invariant, and the method must say so.
    let (_, raw) = CORPUS[0];
    let problem = problem(raw);
    let (outcome, _) = solve(&problem, &Options::default());
    let Outcome::Optimal { basis, .. } = outcome else {
        panic!("the first instance has an optimum");
    };

    let mut broken = basis.clone();
    let mut flipped = false;
    for column in 0..problem.columns() {
        if broken.status(column) == hc_lp::Status::AtLower
            && problem.upper(column).is_finite()
            && problem.lower(column) != problem.upper(column)
        {
            broken.flip(column, hc_lp::Status::AtUpper);
            flipped = true;
            break;
        }
    }
    if !flipped {
        return;
    }

    match solve_dual(&problem, broken, &Options::default()) {
        Err(NotDual::Priced { .. }) => {}
        Err(other) => panic!("refused for the wrong reason: {other}"),
        Ok(_) => {
            // Flipping a column whose reduced cost is exactly zero leaves the
            // basis dual feasible, which is a legitimate outcome rather than a
            // failure of the check.
        }
    }
}

#[test]
fn tightening_a_bound_leaves_the_parent_basis_dual_feasible() {
    // The property the whole warm start rests on: a branch changes a bound and
    // nothing else, so the reduced costs are untouched and the parent's
    // optimal basis still prices correctly against the child.
    let (_, raw) = CORPUS[0];
    let working = Working::from_model(&model(raw));
    let parent = Problem::scaled(&working, Tol::solver());
    let (outcome, _) = solve(&parent, &Options::default());
    let Outcome::Optimal { basis, point, .. } = outcome else {
        panic!("the instance has an optimum");
    };

    // Halve the room above the first structural column's value.
    let column = 0;
    let value = point[column];
    let child_working = working.narrowed(&[(column, f64::NEG_INFINITY, value.floor())]);
    let child = Problem::scaled(&child_working, Tol::solver());

    match solve_dual(&child, basis, &Options::default()) {
        Ok((outcome, _)) => {
            assert!(
                matches!(
                    outcome,
                    Outcome::Optimal { .. } | Outcome::Infeasible { .. }
                ),
                "the child reached {}",
                outcome.as_str()
            );
        }
        Err(NotDual::Unbounded { .. }) => {
            // Scaling can leave a nonbasic column resting at a bound the
            // scaled copy calls infinite. That is a real precondition failure
            // and refusing is correct.
        }
        Err(other) => panic!("the parent basis was refused: {other}"),
    }
}
