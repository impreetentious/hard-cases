//! The bounded-variable primal simplex.
//!
//! Two phases over one problem. The first minimizes how far the basic columns
//! sit outside their bounds; the second minimizes the objective while keeping
//! them inside. There are no artificial columns and no penalty parameter: the
//! first phase is an ordinary bounded linear program obtained by moving the
//! bounds of whichever basic columns are currently violating them, so the same
//! pricing rule, the same ratio test, and the same escalation ladder serve
//! both. A big-M would have been fewer lines and would have made every model's
//! numerics depend on a constant nobody can choose well.
//!
//! Nothing this returns is a claim. `Optimal` means the search found a basis
//! whose reduced costs price nothing better in floating point; whether the
//! model has an optimum, and what it is, is settled by exact machinery reading
//! the original file. The statuses here are named to keep that distinction
//! visible — they carry *candidate* proof data, and the word candidate is not
//! decoration.

use hc_core::{Tol, Xoshiro256PlusPlus};

use crate::basis::{Basis, Status};
use crate::inverse::{Inverse, Reason};
use crate::lu::Factorization;
use crate::pricing::{Pricing, Rule};
use crate::problem::Problem;
use crate::ratio::{self, Attempt, Step};

/// What the search concluded.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// A basis nothing prices better, with the point and multipliers it
    /// implies.
    Optimal {
        /// The final basis.
        basis: Basis,
        /// The point, over every column, in scaled coordinates.
        point: Vec<f64>,
        /// The objective at that point.
        objective: f64,
        /// The row multipliers, which are a dual bound *candidate* and
        /// nothing more until exact assembly says otherwise.
        multipliers: Vec<f64>,
    },
    /// The first phase ended with infeasibility remaining.
    Infeasible {
        /// The final basis.
        basis: Basis,
        /// How far outside their bounds the basic columns sit.
        infeasibility: f64,
        /// The first-phase multipliers, which are a contradiction candidate.
        multipliers: Vec<f64>,
    },
    /// The objective improves without limit along a direction.
    Unbounded {
        /// The final basis.
        basis: Basis,
        /// A feasible point in scaled coordinates.
        point: Vec<f64>,
        /// A recession direction along which the objective strictly falls.
        ray: Vec<f64>,
        /// The column whose movement is unbounded.
        ///
        /// Carried so that exact assembly can rebuild the direction from the
        /// model rather than from this floating-point copy of it. A ray that
        /// went into a certificate as computed here would be a ray about the
        /// scaled problem, in rounded numbers, and neither of those is what a
        /// claim is about.
        entering: usize,
        /// Plus one when that column rises, minus one when it falls.
        direction: i8,
    },
    /// The iteration budget ran out.
    ///
    /// Whatever was in hand is preserved rather than discarded: a
    /// primal-feasible point is worth something even when optimality is not
    /// established, and throwing it away would turn a partial answer into no
    /// answer at all.
    EffortLimit {
        /// The basis reached.
        basis: Basis,
        /// The point, over every column, in scaled coordinates.
        point: Vec<f64>,
        /// The objective at that point.
        objective: f64,
        /// Whether that point satisfies every bound.
        feasible: bool,
    },
    /// The escalation ladder ran out.
    Numerical {
        /// The basis reached, when there is one.
        basis: Basis,
        /// What went wrong, in words meant for a trace.
        note: String,
    },
}

impl Outcome {
    /// The word this outcome appears as in a trace.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Optimal { .. } => "optimal",
            Self::Infeasible { .. } => "infeasible",
            Self::Unbounded { .. } => "unbounded",
            Self::EffortLimit { .. } => "effort-limit",
            Self::Numerical { .. } => "numerical",
        }
    }
}

/// What the search spent.
///
/// Counted, never timed. Effort is a deterministic quantity here: two runs of
/// the same solve on two machines spend exactly the same amount, and a budget
/// measured in seconds would make the search's own behaviour depend on the
/// machine it ran on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effort {
    /// Pivots and bound flips together.
    pub iterations: u64,
    /// Bound flips, which change no basis.
    pub flips: u64,
    /// Steps of length zero.
    pub degenerate: u64,
    /// Times the basis was factored again.
    pub refactorizations: u64,
    /// Times the pricing rule was escalated.
    pub escalations: u64,
}

/// Degenerate iterations in a row before the ladder escalates.
pub const STALL_ITERATIONS: u64 = 200;

/// Escalations before the search falls back to the rule that cannot cycle.
pub const ESCALATIONS_BEFORE_BLAND: u32 = 2;

/// How to run the search.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The pricing rule to start with.
    pub rule: Rule,
    /// The most iterations to spend.
    pub iteration_limit: u64,
    /// The tolerance set in force.
    pub tol: Tol,
    /// The seed for the perturbation draws.
    ///
    /// Part of the options rather than of the machine: a perturbation is a
    /// deterministic function of this number, so a run that escalated can be
    /// replayed exactly, on any machine, from what the report already carries.
    pub seed: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            rule: Rule::Devex,
            iteration_limit: 1_000_000,
            tol: Tol::solver(),
            seed: 0,
        }
    }
}

/// Where the search is on the escalation ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rung {
    /// The ordinary rule, with no intervention.
    Normal,
    /// The basis was factored again and the reference framework restarted.
    Refactored,
    /// The bounds are perturbed to break ties.
    Perturbed,
    /// The rule that cannot cycle, at the cost of every heuristic.
    Bland,
}

impl Rung {
    /// The word this rung appears as in a trace.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Refactored => "refactored",
            Self::Perturbed => "perturbed",
            Self::Bland => "bland",
        }
    }
}

/// A deterministic outward shift of every finite bound.
///
/// Degeneracy is ties, and ties are what a simplex cycles on. Widening every
/// finite bound by a distinct tiny amount removes the ties without removing a
/// single feasible point: the perturbed region contains the real one, so
/// anything feasible stays feasible.
///
/// The shift is drawn from the seeded stream, so it is the same on every
/// machine, and it is *removed* before anything terminates. A bound nobody
/// wrote must never appear in an inference, which is what the trap named for
/// this rule checks.
#[derive(Clone, Debug)]
struct Perturbation {
    lower: Vec<f64>,
    upper: Vec<f64>,
}

impl Perturbation {
    /// Draws a perturbation for a problem.
    fn draw(problem: &Problem, generator: &mut Xoshiro256PlusPlus, tol: Tol) -> Self {
        let columns = problem.columns();
        let mut lower = vec![0.0; columns];
        let mut upper = vec![0.0; columns];
        for index in 0..columns {
            // A fixed column has nothing to widen. Moving its bounds apart
            // would turn an equality into a range, which is a different model
            // rather than the same one seen through a haze.
            if problem.is_fixed(index) {
                continue;
            }
            let low = problem.lower(index);
            if low.is_finite() {
                lower[index] = -shift(low, generator, tol);
            }
            let high = problem.upper(index);
            if high.is_finite() {
                upper[index] = shift(high, generator, tol);
            }
        }
        Self { lower, upper }
    }
}

/// One perturbation magnitude: ten fences, scaled by the bound's own size, and
/// thinned by a seeded draw so that no two bounds move by the same amount.
fn shift(bound: f64, generator: &mut Xoshiro256PlusPlus, tol: Tol) -> f64 {
    tol.feas * 10.0 * (1.0 + bound.abs()) * generator.next_unit_interval()
}

/// Solves a problem from a cold start.
#[must_use]
pub fn solve(problem: &Problem, options: &Options) -> (Outcome, Effort) {
    solve_from(problem, Basis::logical(problem), options)
}

/// Solves a problem from a given basis.
#[must_use]
pub fn solve_from(problem: &Problem, start: Basis, options: &Options) -> (Outcome, Effort) {
    let tol = options.tol;
    let mut basis = start;
    let mut effort = Effort::default();
    let mut pricing = Pricing::new(options.rule, problem.columns());
    let mut generator = Xoshiro256PlusPlus::from_seed(options.seed);
    let mut rung = Rung::Normal;
    let mut degenerate_run = 0u64;
    let mut escalations = 0u32;
    let mut perturbation: Option<Perturbation> = None;

    let mut inverse = match basis.factor(problem, tol) {
        Ok(inverse) => inverse,
        Err(error) => {
            return (
                Outcome::Numerical {
                    basis,
                    note: format!("the starting basis is unusable: {error}"),
                },
                effort,
            );
        }
    };

    loop {
        if effort.iterations >= options.iteration_limit {
            let point = basis.primal_point(problem, &inverse);
            let feasible = basis.is_primal_feasible(problem, &point, tol);
            let objective = problem.objective_at(&point);
            return (
                Outcome::EffortLimit {
                    basis,
                    point,
                    objective,
                    feasible,
                },
                effort,
            );
        }

        let point = basis.primal_point(problem, &inverse);
        let phase = Phase::of(problem, &basis, &point, perturbation.as_ref(), tol);

        let multipliers = {
            let mut costs: Vec<f64> = basis
                .basic()
                .iter()
                .map(|column| phase.costs[*column])
                .collect();
            inverse.btran(&mut costs);
            costs
        };

        let Some(entering) = pricing.choose(problem, &basis, &multipliers, &phase.costs, tol)
        else {
            // Nothing prices better under the objective in force. If the
            // bounds are still perturbed, that is a statement about a model
            // nobody wrote: drop the perturbation and carry on against the
            // real one. Nothing terminates while a made-up bound is in force.
            if perturbation.is_some() {
                perturbation = None;
                rung = Rung::Normal;
                degenerate_run = 0;
                pricing.switch(options.rule);
                continue;
            }
            if phase.infeasibility > tol.feas {
                return (
                    Outcome::Infeasible {
                        basis,
                        infeasibility: phase.infeasibility,
                        multipliers,
                    },
                    effort,
                );
            }
            let objective = problem.objective_at(&point);
            return (
                Outcome::Optimal {
                    basis,
                    point,
                    objective,
                    multipliers,
                },
                effort,
            );
        };

        let mut alpha = vec![0.0; problem.rows()];
        for &(row, value) in problem.column(entering.column) {
            alpha[row as usize] = value;
        }
        inverse.ftran(&mut alpha);

        let step = ratio::choose(
            &Attempt {
                problem,
                basis: &basis,
                point: &point,
                lower: &phase.lower,
                upper: &phase.upper,
                entering: entering.column,
                direction: entering.direction,
                alpha: &alpha,
            },
            tol,
        );

        match step {
            Step::Unbounded => {
                if phase.infeasibility > tol.feas {
                    // The first phase minimizes a sum of distances, which is
                    // bounded below by zero. Reaching here means the numbers
                    // stopped describing the problem.
                    return (
                        Outcome::Numerical {
                            basis,
                            note: "the first phase reported an unbounded direction".to_owned(),
                        },
                        effort,
                    );
                }
                let ray =
                    recession_ray(problem, &basis, entering.column, entering.direction, &alpha);
                let column = entering.column;
                let direction = if entering.direction > 0.0 { 1i8 } else { -1i8 };
                return (
                    Outcome::Unbounded {
                        basis,
                        point,
                        ray,
                        entering: column,
                        direction,
                    },
                    effort,
                );
            }
            Step::BoundFlip { .. } => {
                degenerate_run = 0;
                let to = match basis.status(entering.column) {
                    Status::AtLower => Status::AtUpper,
                    Status::AtUpper => Status::AtLower,
                    other => {
                        return (
                            Outcome::Numerical {
                                basis,
                                note: format!("a {} column was asked to flip", other.as_str()),
                            },
                            effort,
                        );
                    }
                };
                basis.flip(entering.column, to);
                effort.flips += 1;
            }
            Step::Blocked {
                row,
                leaving,
                theta,
                pivot,
            } => {
                if theta == 0.0 {
                    effort.degenerate += 1;
                }
                // Devex needs the leaving row of the basis-transformed matrix.
                // One transposed solve gives the row multipliers for it, and
                // one pass over the columns turns those into the row itself.
                let pivot_row = leaving_row(problem, &basis, &inverse, row);
                pricing.update(entering.column, leaving, &pivot_row, pivot);

                let value_after = point[leaving] - entering.direction * theta * alpha[row];
                let rests_at = resting_status(problem, leaving, value_after);
                basis.pivot(row, entering.column, rests_at);

                if inverse.update(row, &alpha, tol).is_err() {
                    match basis.factor(problem, tol) {
                        Ok(fresh) => {
                            inverse = fresh;
                            effort.refactorizations += 1;
                        }
                        Err(error) => {
                            return (
                                Outcome::Numerical {
                                    basis,
                                    note: format!("the basis became unusable: {error}"),
                                },
                                effort,
                            );
                        }
                    }
                }
            }
        }

        effort.iterations += 1;

        // The escalation ladder. A run of steps that move nowhere is the only
        // way a simplex fails to terminate, so the response is graded: try the
        // arithmetic again, then break the ties, then fall back to the rule
        // that provably cannot cycle.
        if degenerate_run >= STALL_ITERATIONS {
            degenerate_run = 0;
            escalations += 1;
            effort.escalations += 1;
            rung = match rung {
                Rung::Normal => {
                    // The cheapest explanation for a stall is that the numbers
                    // stopped being reliable, so try them again first.
                    if let Ok(fresh) = basis.factor(problem, tol) {
                        inverse = fresh;
                        effort.refactorizations += 1;
                    }
                    pricing.reset();
                    Rung::Refactored
                }
                Rung::Refactored if escalations <= ESCALATIONS_BEFORE_BLAND => {
                    perturbation = Some(Perturbation::draw(problem, &mut generator, tol));
                    Rung::Perturbed
                }
                _ => {
                    pricing.switch(Rule::Bland);
                    perturbation = None;
                    Rung::Bland
                }
            };
        }

        // The refactorization policy, measured rather than assumed: the
        // residual is computed against the original columns of the current
        // basis, not through the factors that would be answering.
        let columns: Vec<Vec<(u32, f64)>> = basis
            .basic()
            .iter()
            .map(|column| problem.column(*column).to_vec())
            .collect();
        let refreshed = basis.primal_point(problem, &inverse);
        let mut target = vec![0.0; problem.rows()];
        for (column, value) in refreshed.iter().enumerate() {
            if basis.status(column).is_basic() || *value == 0.0 {
                continue;
            }
            for &(row, coefficient) in problem.column(column) {
                target[row as usize] -= coefficient * value;
            }
        }
        let basic_values: Vec<f64> = basis
            .basic()
            .iter()
            .map(|column| refreshed[*column])
            .collect();
        let residual = Factorization::residual(&columns, &basic_values, &target);

        if let Some(reason) = inverse.should_refactor(residual, tol) {
            match basis.factor(problem, tol) {
                Ok(fresh) => {
                    inverse = fresh;
                    effort.refactorizations += 1;
                    if reason == Reason::Drift {
                        // Drift means the weights were estimated from numbers
                        // that had stopped being reliable.
                        pricing.reset();
                    }
                }
                Err(error) => {
                    return (
                        Outcome::Numerical {
                            basis,
                            note: format!("refactorization on {} failed: {error}", reason.as_str()),
                        },
                        effort,
                    );
                }
            }
        }
    }
}

/// The objective and bounds in force at one iteration.
struct Phase {
    costs: Vec<f64>,
    lower: Vec<f64>,
    upper: Vec<f64>,
    infeasibility: f64,
}

impl Phase {
    /// Builds the first-phase problem, or the second one when nothing is
    /// infeasible.
    ///
    /// The first phase is not a different algorithm. A basic column sitting
    /// below its lower bound has that bound moved to negative infinity and its
    /// old lower bound installed as an upper bound, with a cost that pushes it
    /// upward — so the column is pulled back to where it belongs by an
    /// ordinary bounded step, and reaching it is an ordinary blocking event.
    fn of(
        problem: &Problem,
        basis: &Basis,
        point: &[f64],
        perturbation: Option<&Perturbation>,
        tol: Tol,
    ) -> Self {
        let columns = problem.columns();
        let mut lower: Vec<f64> = (0..columns).map(|index| problem.lower(index)).collect();
        let mut upper: Vec<f64> = (0..columns).map(|index| problem.upper(index)).collect();
        if let Some(shifts) = perturbation {
            for index in 0..columns {
                lower[index] += shifts.lower[index];
                upper[index] += shifts.upper[index];
            }
        }
        let mut costs = vec![0.0; columns];
        let mut infeasibility = 0.0;

        for column in basis.basic() {
            let value = point[*column];
            let low = lower[*column];
            let high = upper[*column];
            if value < low - tol.feas {
                infeasibility += low - value;
                lower[*column] = f64::NEG_INFINITY;
                upper[*column] = low;
                costs[*column] = -1.0;
            } else if value > high + tol.feas {
                infeasibility += value - high;
                lower[*column] = high;
                upper[*column] = f64::INFINITY;
                costs[*column] = 1.0;
            }
        }

        if infeasibility <= tol.feas {
            for (index, slot) in costs.iter_mut().enumerate() {
                *slot = problem.cost(index);
            }
            return Self {
                costs,
                lower,
                upper,
                infeasibility: 0.0,
            };
        }
        Self {
            costs,
            lower,
            upper,
            infeasibility,
        }
    }
}

/// Where a column comes to rest, decided against the model's own bounds.
///
/// The first phase moves bounds around, so the bound a step blocked at is not
/// necessarily a bound the model has. The resting place is therefore chosen by
/// which of the model's bounds the column actually ended up at.
fn resting_status(problem: &Problem, column: usize, value: f64) -> Status {
    let low = problem.lower(column);
    let high = problem.upper(column);
    if low == high {
        return Status::Fixed;
    }
    match (low.is_finite(), high.is_finite()) {
        (true, true) => {
            if (value - low).abs() <= (value - high).abs() {
                Status::AtLower
            } else {
                Status::AtUpper
            }
        }
        (true, false) => Status::AtLower,
        (false, true) => Status::AtUpper,
        (false, false) => Status::Free,
    }
}

/// The leaving row of the basis-transformed matrix, over the nonbasic columns.
fn leaving_row(
    problem: &Problem,
    basis: &Basis,
    inverse: &Inverse,
    row: usize,
) -> Vec<(usize, f64)> {
    let mut unit = vec![0.0; problem.rows()];
    unit[row] = 1.0;
    inverse.btran(&mut unit);

    let mut out = Vec::new();
    for column in 0..problem.columns() {
        if basis.status(column).is_basic() {
            continue;
        }
        let mut total = 0.0;
        for &(at, value) in problem.column(column) {
            total += unit[at as usize] * value;
        }
        if total != 0.0 {
            out.push((column, total));
        }
    }
    out
}

/// The direction the objective improves along without limit.
///
/// The entering column moves, every basic column moves to keep the system
/// satisfied, and nothing else moves at all. This is a candidate ray: a
/// certificate needs it exact, over the original model, with a strictly
/// improving objective demonstrated rather than assumed.
fn recession_ray(
    problem: &Problem,
    basis: &Basis,
    entering: usize,
    direction: f64,
    alpha: &[f64],
) -> Vec<f64> {
    let mut ray = vec![0.0; problem.columns()];
    ray[entering] = direction;
    for (row, column) in basis.basic().iter().enumerate() {
        ray[*column] = -direction * alpha[row];
    }
    ray
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

    fn optimum(outcome: &Outcome) -> (f64, &[f64]) {
        match outcome {
            Outcome::Optimal {
                objective, point, ..
            } => (*objective, point),
            other => panic!("expected an optimum, got {}", other.as_str()),
        }
    }

    /// A comparison fence for these fixtures, a notch inside the search's own
    /// feasibility fence. Derived rather than written, because a bare
    /// scientific literal is indistinguishable from a tolerance to the scan
    /// that keeps tolerances out of call sites.
    const FENCE: f64 = Tol::solver().feas / 10.0;

    #[test]
    fn a_two_variable_maximization_lands_on_its_vertex() {
        // The same case the reference solver is held to: the optimum of
        // 3x + 2y over x + y <= 4 and x + 3y <= 6 is 12 at (4, 0), which the
        // minimizing model reports as -12.
        let problem = problem(concat!(
            "OBJSENSE MAX\nROWS\n N obj\n L c1\n L c2\n",
            "COLUMNS\n x obj 3 c1 1\n x c2 1\n y obj 2 c1 1\n y c2 3\n",
            "RHS\n r c1 4 c2 6\n",
            "ENDATA\n"
        ));
        let (outcome, effort) = solve(&problem, &Options::default());
        let (objective, point) = optimum(&outcome);
        assert!(
            (objective - -12.0).abs() < FENCE,
            "objective was {objective}"
        );
        assert!((point[0] - 4.0).abs() < FENCE, "x was {}", point[0]);
        assert!(point[1].abs() < FENCE, "y was {}", point[1]);
        assert!(effort.iterations > 0);
    }

    #[test]
    fn an_equality_system_needs_the_first_phase() {
        // The cold basis puts both logicals in, and an equality row's logical
        // is fixed at a value the cold point does not give it. Reaching
        // feasibility at all is the first phase's work.
        let problem = problem(concat!(
            "ROWS\n N obj\n E c1\n E c2\n",
            "COLUMNS\n x obj 1 c1 1\n x c2 1\n y obj 1 c1 1\n y c2 -1\n",
            "RHS\n r c1 3 c2 1\n",
            "ENDATA\n"
        ));
        let (outcome, _) = solve(&problem, &Options::default());
        let (objective, point) = optimum(&outcome);
        assert!((point[0] - 2.0).abs() < FENCE, "x was {}", point[0]);
        assert!((point[1] - 1.0).abs() < FENCE, "y was {}", point[1]);
        assert!((objective - 3.0).abs() < FENCE);
    }

    #[test]
    fn a_bounded_variable_stops_at_its_bound() {
        let problem = problem(concat!(
            "ROWS\n N obj\n L c1\n",
            "COLUMNS\n x obj -1 c1 1\n",
            "RHS\n r c1 100\n",
            "BOUNDS\n UP b x 7\n",
            "ENDATA\n"
        ));
        let (outcome, effort) = solve(&problem, &Options::default());
        let (objective, point) = optimum(&outcome);
        assert!(
            (objective - -7.0).abs() < FENCE,
            "objective was {objective}"
        );
        assert!((point[0] - 7.0).abs() < FENCE);
        // Reaching a bound needs no pivot at all, which is what bound flips
        // are for.
        assert!(effort.flips > 0 || effort.iterations > 0);
    }

    #[test]
    fn a_negative_lower_bound_and_a_free_column_are_both_handled() {
        let shifted = problem(concat!(
            "ROWS\n N obj\n L c1\n",
            "COLUMNS\n x obj 1 c1 1\n",
            "RHS\n r c1 100\n",
            "BOUNDS\n LO b x -5\n UP b x 5\n",
            "ENDATA\n"
        ));
        let (outcome, _) = solve(&shifted, &Options::default());
        let (objective, _) = optimum(&outcome);
        assert!(
            (objective - -5.0).abs() < FENCE,
            "objective was {objective}"
        );

        let free = problem(concat!(
            "ROWS\n N obj\n G c1\n",
            "COLUMNS\n x obj 1 c1 1\n",
            "RHS\n r c1 -3\n",
            "BOUNDS\n FR b x\n",
            "ENDATA\n"
        ));
        let (outcome, _) = solve(&free, &Options::default());
        let (objective, _) = optimum(&outcome);
        assert!(
            (objective - -3.0).abs() < FENCE,
            "objective was {objective}"
        );
    }

    #[test]
    fn contradictory_constraints_end_the_first_phase_short() {
        let problem = problem(concat!(
            "ROWS\n N obj\n G c1\n L c2\n",
            "COLUMNS\n x obj 1 c1 1\n x c2 1\n",
            "RHS\n r c1 2 c2 1\n",
            "ENDATA\n"
        ));
        let (outcome, _) = solve(&problem, &Options::default());
        match outcome {
            Outcome::Infeasible { infeasibility, .. } => {
                assert!(infeasibility > FENCE, "infeasibility was {infeasibility}");
            }
            other => panic!("expected infeasibility, got {}", other.as_str()),
        }
    }

    #[test]
    fn an_objective_with_nothing_to_stop_it_reports_a_ray() {
        let problem = problem(concat!(
            "ROWS\n N obj\n L c1\n",
            "COLUMNS\n x obj -1\n y obj 1 c1 1\n",
            "RHS\n r c1 4\n",
            "ENDATA\n"
        ));
        let (outcome, _) = solve(&problem, &Options::default());
        match outcome {
            Outcome::Unbounded { ray, point, .. } => {
                // The ray improves the objective and keeps the system solved.
                let rate: f64 = (0..problem.columns())
                    .map(|index| problem.cost(index) * ray[index])
                    .sum();
                assert!(
                    rate < -FENCE,
                    "the ray does not improve the objective: {rate}"
                );
                let residual = problem.residual_at(&ray);
                assert!(
                    residual.iter().all(|value| value.abs() <= FENCE),
                    "the ray leaves the system: {residual:?}"
                );
                assert!(
                    problem
                        .residual_at(&point)
                        .iter()
                        .all(|value| value.abs() <= FENCE)
                );
            }
            other => panic!("expected an unbounded direction, got {}", other.as_str()),
        }
    }

    #[test]
    fn the_effort_budget_stops_the_search_and_keeps_what_it_had() {
        let problem = problem(concat!(
            "OBJSENSE MAX\nROWS\n N obj\n L c1\n L c2\n",
            "COLUMNS\n x obj 3 c1 1\n x c2 1\n y obj 2 c1 1\n y c2 3\n",
            "RHS\n r c1 4 c2 6\n",
            "ENDATA\n"
        ));
        let options = Options {
            iteration_limit: 1,
            ..Options::default()
        };
        let (outcome, effort) = solve(&problem, &options);
        match outcome {
            Outcome::EffortLimit { point, .. } => {
                assert_eq!(effort.iterations, 1);
                assert_eq!(point.len(), problem.columns());
            }
            // A one-iteration budget may also be enough on so small a model,
            // and finishing early is not a failure.
            Outcome::Optimal { .. } => {}
            other => panic!("expected an effort stop, got {}", other.as_str()),
        }
    }

    #[test]
    fn every_pricing_rule_reaches_the_same_optimum() {
        // The rules differ in which column they take and therefore in how many
        // iterations they spend. They may not differ in where they end up.
        let problem = problem(concat!(
            "OBJSENSE MAX\nROWS\n N obj\n L c1\n L c2\n L c3\n",
            "COLUMNS\n",
            " x obj 5 c1 2\n x c2 1\n x c3 1\n",
            " y obj 4 c1 1\n y c2 2\n y c3 1\n",
            " z obj 3 c1 1\n z c2 1\n z c3 2\n",
            "RHS\n r c1 10 c2 8\n r c3 6\n",
            "ENDATA\n"
        ));
        let mut objectives = Vec::new();
        for rule in [Rule::Devex, Rule::Dantzig, Rule::Bland] {
            let options = Options {
                rule,
                ..Options::default()
            };
            let (outcome, _) = solve(&problem, &options);
            let (objective, point) = optimum(&outcome);
            assert!(
                problem
                    .residual_at(point)
                    .iter()
                    .all(|value| value.abs() <= FENCE),
                "under {}",
                rule.as_str()
            );
            objectives.push(objective);
        }
        for pair in objectives.windows(2) {
            assert!(
                (pair[0] - pair[1]).abs() < FENCE,
                "the rules disagreed: {objectives:?}"
            );
        }
    }

    #[test]
    fn the_same_problem_solved_twice_spends_the_same_effort() {
        let problem = problem(concat!(
            "OBJSENSE MAX\nROWS\n N obj\n L c1\n L c2\n L c3\n",
            "COLUMNS\n",
            " x obj 5 c1 2\n x c2 1\n x c3 1\n",
            " y obj 4 c1 1\n y c2 2\n y c3 1\n",
            " z obj 3 c1 1\n z c2 1\n z c3 2\n",
            "RHS\n r c1 10 c2 8\n r c3 6\n",
            "ENDATA\n"
        ));
        let (first, first_effort) = solve(&problem, &Options::default());
        let (second, second_effort) = solve(&problem, &Options::default());
        assert_eq!(first_effort, second_effort);
        assert_eq!(optimum(&first).0, optimum(&second).0);
        assert_eq!(optimum(&first).1, optimum(&second).1);
    }

    #[test]
    fn scaling_does_not_change_where_the_search_ends_up() {
        let source = concat!(
            "ROWS\n N obj\n L c1\n L c2\n",
            "COLUMNS\n x obj -1 c1 100000\n x c2 0.001\n y obj -1 c1 3\n y c2 400000\n",
            "RHS\n r c1 1000000 c2 4000000\n",
            "BOUNDS\n UP b x 20\n UP b y 20\n",
            "ENDATA\n"
        );
        let working = Working::from_model(&model(source));
        let plain = Problem::unscaled(&working);
        let scaled = Problem::scaled(&working, Tol::solver());
        assert!(
            !scaled.scaling().is_identity(),
            "the fixture gave scaling nothing to do"
        );

        let (a, _) = solve(&plain, &Options::default());
        let (b, _) = solve(&scaled, &Options::default());
        let left = optimum(&a).0;
        let right = optimum(&b).0;
        assert!((left - right).abs() < FENCE, "{left} against {right}");

        // And the recovered point is in the same coordinates as the plain one.
        let recovered = scaled.recover_point(optimum(&b).1);
        let plain_point = optimum(&a).1;
        for (index, (got, want)) in recovered.iter().zip(plain_point).enumerate() {
            assert!(
                (got - want).abs() < FENCE,
                "column {index}: {got} against {want}"
            );
        }
    }

    #[test]
    fn a_problem_with_no_rows_is_answered_without_pivoting() {
        let empty = problem("ROWS\n N obj\nCOLUMNS\nENDATA\n");
        let (outcome, effort) = solve(&empty, &Options::default());
        assert!(matches!(outcome, Outcome::Optimal { .. }));
        assert_eq!(effort.iterations, 0);
    }
}
