//! The reference solver against hand-worked cases.
//!
//! Every optimum below was worked out by hand, on paper, from the geometry.
//! That is the only way an oracle's fixtures can be written: a fixture
//! produced by running the code it is meant to check is not a fixture, it is a
//! screenshot.
//!
//! Each case is also checked for internal consistency — the point is feasible,
//! and its objective is the one reported — so a fixture that happens to agree
//! with a wrong answer still has to survive being recomputed a second way.

use hc_bench::ref_lp::{Outcome, is_feasible, objective_at, solve};
use hc_core::Model;
use hc_io::mps::{Form, read};

/// A fence loose enough for a dense tableau and tight enough to catch a real
/// disagreement. The oracle is a hint, not a claim, so this is a comparison
/// tolerance rather than a threshold anything is proved against.
const FENCE: f64 = 1e-6;

fn model(source: &str) -> Model {
    read(source.as_bytes(), Form::Free).expect("the fixture is inside the accepted subset")
}

fn optimum(outcome: &Outcome) -> (f64, &[f64]) {
    match outcome {
        Outcome::Optimal { objective, point } => (*objective, point),
        other => panic!("expected an optimum, got {other:?}"),
    }
}

#[test]
fn a_two_variable_maximization_lands_on_its_vertex() {
    // Maximize 3x + 2y subject to x + y <= 4 and x + 3y <= 6, both variables
    // nonnegative. The vertices are (0,0), (4,0), (3,1) and (0,2), giving
    // objectives 0, 12, 11 and 4. The optimum is 12 at (4,0), which the
    // canonical minimizing model reports as -12.
    let instance = model(
        "OBJSENSE MAX\nROWS\n N obj\n L c1\n L c2\nCOLUMNS\n x obj 3 c1 1\n x c2 1\n y obj 2 c1 1\n y c2 3\nRHS\n r c1 4 c2 6\nENDATA\n",
    );
    let outcome = solve(&instance);
    let (objective, point) = optimum(&outcome);
    assert!(
        (objective - -12.0).abs() < FENCE,
        "objective was {objective}"
    );
    assert!((point[0] - 4.0).abs() < FENCE, "x was {}", point[0]);
    assert!(point[1].abs() < FENCE, "y was {}", point[1]);
    assert!(is_feasible(&instance, point, FENCE));
    assert!((objective_at(&instance, point) - objective).abs() < FENCE);
}

#[test]
fn an_equality_system_with_one_solution_is_found() {
    // x + y = 3 and x - y = 1 have the single solution (2, 1). The objective
    // cannot move anything, so any correct solver returns that point.
    let instance = model(
        "ROWS\n N obj\n E c1\n E c2\nCOLUMNS\n x obj 1 c1 1\n x c2 1\n y obj 1 c1 1\n y c2 -1\nRHS\n r c1 3 c2 1\nENDATA\n",
    );
    let (objective, point) = {
        let outcome = solve(&instance);
        let (objective, point) = optimum(&outcome);
        (objective, point.to_vec())
    };
    assert!((point[0] - 2.0).abs() < FENCE, "x was {}", point[0]);
    assert!((point[1] - 1.0).abs() < FENCE, "y was {}", point[1]);
    assert!((objective - 3.0).abs() < FENCE);
    assert!(is_feasible(&instance, &point, FENCE));
}

#[test]
fn a_bounded_variable_stops_at_its_bound() {
    // Minimize -x with x in [0, 7] and no constraint able to stop it earlier.
    // The optimum is -7 at the upper bound, which is the case a solver that
    // ignores variable bounds reports as unbounded.
    let instance = model(
        "ROWS\n N obj\n L c1\nCOLUMNS\n x obj -1 c1 1\nRHS\n r c1 100\nBOUNDS\n UP b x 7\nENDATA\n",
    );
    let outcome = solve(&instance);
    let (objective, point) = optimum(&outcome);
    assert!(
        (objective - -7.0).abs() < FENCE,
        "objective was {objective}"
    );
    assert!((point[0] - 7.0).abs() < FENCE);
    assert!(is_feasible(&instance, point, FENCE));
}

#[test]
fn a_negative_lower_bound_is_shifted_rather_than_split() {
    // x in [-5, 5], minimize x. The optimum is -5. A conversion that forgot
    // the shift would report 0.
    let instance = model(
        "ROWS\n N obj\n L c1\nCOLUMNS\n x obj 1 c1 1\nRHS\n r c1 100\nBOUNDS\n LO b x -5\n UP b x 5\nENDATA\n",
    );
    let outcome = solve(&instance);
    let (objective, point) = optimum(&outcome);
    assert!(
        (objective - -5.0).abs() < FENCE,
        "objective was {objective}"
    );
    assert!((point[0] - -5.0).abs() < FENCE);
    assert!(is_feasible(&instance, point, FENCE));
}

#[test]
fn a_free_variable_is_split_and_recovered() {
    // x free, minimize x subject to x >= -3. The optimum is -3, and a solver
    // that dropped the negative part of the split would report 0.
    let instance = model(
        "ROWS\n N obj\n G c1\nCOLUMNS\n x obj 1 c1 1\nRHS\n r c1 -3\nBOUNDS\n FR b x\nENDATA\n",
    );
    let outcome = solve(&instance);
    let (objective, point) = optimum(&outcome);
    assert!(
        (objective - -3.0).abs() < FENCE,
        "objective was {objective}"
    );
    assert!((point[0] - -3.0).abs() < FENCE);
    assert!(is_feasible(&instance, point, FENCE));
}

#[test]
fn a_ranged_row_binds_on_both_sides() {
    // 2 <= x <= 6 written as a range on a row rather than as bounds, with the
    // objective pushing down. The optimum is 2.
    let instance = model(
        "ROWS\n N obj\n G c1\nCOLUMNS\n x obj 1 c1 1\nRHS\n r c1 2\nRANGES\n g c1 4\nENDATA\n",
    );
    let outcome = solve(&instance);
    let (objective, point) = optimum(&outcome);
    assert!((objective - 2.0).abs() < FENCE, "objective was {objective}");
    assert!(is_feasible(&instance, point, FENCE));

    // The same row with the objective reversed stops at the other side.
    let upward = model(
        "ROWS\n N obj\n G c1\nCOLUMNS\n x obj -1 c1 1\nRHS\n r c1 2\nRANGES\n g c1 4\nENDATA\n",
    );
    let outcome = solve(&upward);
    let (objective, _) = optimum(&outcome);
    assert!(
        (objective - -6.0).abs() < FENCE,
        "objective was {objective}"
    );
}

#[test]
fn an_objective_constant_is_carried_into_the_value() {
    // The right-hand side entry on the objective row supplies the negation of
    // the constant, so a constant of -5 shifts every objective by -5.
    let instance =
        model("ROWS\n N obj\n G c1\nCOLUMNS\n x obj 1 c1 1\nRHS\n r obj 5 c1 3\nENDATA\n");
    let outcome = solve(&instance);
    let (objective, point) = optimum(&outcome);
    assert!(
        (objective - -2.0).abs() < FENCE,
        "objective was {objective}"
    );
    assert!((objective_at(&instance, point) - objective).abs() < FENCE);
}

#[test]
fn contradictory_constraints_are_infeasible() {
    // x >= 2 and x <= 1 have no solution.
    let instance = model(
        "ROWS\n N obj\n G c1\n L c2\nCOLUMNS\n x obj 1 c1 1\n x c2 1\nRHS\n r c1 2 c2 1\nENDATA\n",
    );
    assert_eq!(solve(&instance), Outcome::Infeasible);
}

#[test]
fn crossed_variable_bounds_are_infeasible() {
    let instance = model(
        "ROWS\n N obj\n L c1\nCOLUMNS\n x obj 1 c1 1\nRHS\n r c1 100\nBOUNDS\n LO b x 5\n UP b x 2\nENDATA\n",
    );
    assert_eq!(solve(&instance), Outcome::Infeasible);
}

#[test]
fn an_objective_with_nothing_to_stop_it_is_unbounded() {
    // Minimize -x with x nonnegative and no upper bound, and a constraint that
    // only bounds a second variable.
    let instance =
        model("ROWS\n N obj\n L c1\nCOLUMNS\n x obj -1\n y obj 1 c1 1\nRHS\n r c1 4\nENDATA\n");
    assert_eq!(solve(&instance), Outcome::Unbounded);
}

#[test]
fn a_degenerate_vertex_does_not_cycle() {
    // Three constraints meeting at one point, which is where a simplex without
    // an anti-cycling rule can stall forever. Bland's rule is the reason this
    // terminates, and the case exists to keep that rule from being replaced by
    // something faster.
    let instance = model(
        "ROWS\n N obj\n L c1\n L c2\n L c3\nCOLUMNS\n x obj -1 c1 1\n x c2 1\n x c3 2\n y obj -1 c1 1\n y c2 2\n y c3 1\nRHS\n r c1 4 c2 6\n r c3 6\nENDATA\n",
    );
    let outcome = solve(&instance);
    let (objective, point) = optimum(&outcome);
    assert!(is_feasible(&instance, point, FENCE));
    assert!((objective_at(&instance, point) - objective).abs() < FENCE);
    // The optimum of -(x + y) over that region is at x = y = 2.
    assert!(
        (objective - -4.0).abs() < FENCE,
        "objective was {objective}"
    );
}

#[test]
fn an_empty_row_is_retained_and_decides_feasibility_by_itself() {
    // A row with no coefficients is either trivially satisfied or immediately
    // contradictory, and which one it is has to be decided by the same rules
    // that decide everything else.
    let satisfied =
        model("ROWS\n N obj\n G c1\n G c2\nCOLUMNS\n x obj 1 c1 1\nRHS\n r c1 1 c2 -1\nENDATA\n");
    assert!(matches!(solve(&satisfied), Outcome::Optimal { .. }));

    let contradictory =
        model("ROWS\n N obj\n G c1\n G c2\nCOLUMNS\n x obj 1 c1 1\nRHS\n r c1 1 c2 1\nENDATA\n");
    assert_eq!(solve(&contradictory), Outcome::Infeasible);
}

#[test]
fn integrality_is_ignored_because_this_is_the_linear_oracle() {
    // The relaxation optimum of this knapsack is fractional. The oracle
    // reports it, because relaxing is its job and the integer oracle is
    // exhaustive enumeration somewhere else entirely.
    let instance = model(
        "ROWS\n N obj\n L c1\nCOLUMNS\n m MARKER INTORG\n x obj -3 c1 2\n m MARKER INTEND\nRHS\n r c1 3\nBOUNDS\n UI b x 5\nENDATA\n",
    );
    let outcome = solve(&instance);
    let (objective, point) = optimum(&outcome);
    assert!((point[0] - 1.5).abs() < FENCE, "x was {}", point[0]);
    assert!(
        (objective - -4.5).abs() < FENCE,
        "objective was {objective}"
    );
    assert!(
        instance.variables()[0].integral,
        "the fixture should declare integrality"
    );
}
