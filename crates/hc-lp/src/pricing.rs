//! Choosing which column enters the basis.
//!
//! The textbook rule takes the most negative reduced cost. It is easy to
//! explain, it is what the teaching mode shows, and on any real model it is
//! bad: a reduced cost is a rate of improvement per unit of movement, and the
//! column with the steepest rate is frequently the one that can move least.
//!
//! Devex fixes that by dividing each reduced cost by an estimate of how far
//! the column actually moves the basis — a reference weight, updated from the
//! same vectors the pivot already needs. The estimate is crude and cheap,
//! which is the right combination for a decision made once per iteration on
//! every column.
//!
//! Both rules end at the same tiebreak: the lowest column index. Every
//! decision in this solver ends there. Two runs on two machines must choose
//! the same column, and "whichever came first" is not a rule when the loop
//! order can change.

use hc_core::Tol;

use crate::basis::{Basis, Status};
use crate::problem::Problem;

/// Above this weight the reference framework is restarted.
///
/// Devex weights only grow. Left alone they eventually swamp the reduced costs
/// they are dividing, and the rule quietly degenerates into choosing whichever
/// column has been untouched longest.
pub const WEIGHT_CEILING: f64 = 1e12;

/// Which pricing rule to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Reference-weighted pricing.
    Devex,
    /// The textbook rule, kept for teaching and for comparison.
    Dantzig,
    /// The lowest eligible index, which cannot cycle.
    ///
    /// This is the last rung of the escalation ladder rather than a choice
    /// anyone makes deliberately: it is the slowest rule available and the
    /// only one with a termination proof that needs no perturbation.
    Bland,
}

impl Rule {
    /// The word this rule appears as in a trace.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Devex => "devex",
            Self::Dantzig => "dantzig",
            Self::Bland => "bland",
        }
    }
}

/// The column chosen to enter, and which way it will move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entering {
    /// The column index.
    pub column: usize,
    /// Its reduced cost.
    pub reduced_cost: f64,
    /// Plus one to increase the column, minus one to decrease it.
    pub direction: f64,
}

/// The pricing state.
#[derive(Clone, Debug)]
pub struct Pricing {
    rule: Rule,
    weights: Vec<f64>,
    resets: usize,
}

impl Pricing {
    /// A fresh pricing state over a column count.
    #[must_use]
    pub fn new(rule: Rule, columns: usize) -> Self {
        Self {
            rule,
            weights: vec![1.0; columns],
            resets: 0,
        }
    }

    /// The rule in use.
    #[must_use]
    pub fn rule(&self) -> Rule {
        self.rule
    }

    /// Switches rule, restarting the reference framework.
    ///
    /// Used by the escalation ladder. The weights are restarted rather than
    /// carried, because they estimate movement under a rule that is no longer
    /// the one being applied.
    pub fn switch(&mut self, rule: Rule) {
        self.rule = rule;
        self.reset();
    }

    /// How many times the reference framework has been restarted.
    #[must_use]
    pub fn resets(&self) -> usize {
        self.resets
    }

    /// The weight of a column, which is one until the column takes part in a
    /// pivot.
    #[must_use]
    pub fn weight(&self, column: usize) -> f64 {
        self.weights[column]
    }

    /// Restarts the reference framework.
    pub fn reset(&mut self) {
        self.weights.fill(1.0);
        self.resets += 1;
    }

    /// Chooses the entering column, or reports that none improves.
    ///
    /// `costs` is the objective in use, which is not always the model's: the
    /// first phase prices against an infeasibility objective instead.
    #[must_use]
    pub fn choose(
        &self,
        problem: &Problem,
        basis: &Basis,
        multipliers: &[f64],
        costs: &[f64],
        tol: Tol,
    ) -> Option<Entering> {
        let mut best: Option<(f64, Entering)> = None;

        for column in 0..problem.columns() {
            let status = basis.status(column);
            if status.is_basic() || status == Status::Fixed {
                continue;
            }
            let reduced = reduced_cost(problem, multipliers, costs, column);

            // A column at its lower bound improves by rising when its reduced
            // cost is negative; one at its upper bound improves by falling
            // when it is positive. A free column improves in whichever
            // direction its sign allows.
            let direction = if reduced < -tol.opt && status.may_rise() {
                1.0
            } else if reduced > tol.opt && status.may_fall() {
                -1.0
            } else {
                continue;
            };

            if self.rule == Rule::Bland {
                // The lowest eligible index, and nothing else considered.
                return Some(Entering {
                    column,
                    reduced_cost: reduced,
                    direction,
                });
            }

            let score = match self.rule {
                Rule::Devex => reduced * reduced / self.weights[column],
                _ => reduced.abs(),
            };
            let candidate = Entering {
                column,
                reduced_cost: reduced,
                direction,
            };
            match best {
                // The comparison is strict, so an equal score leaves the
                // earlier — and therefore lower-indexed — column in place.
                Some((top, _)) if score <= top => {}
                _ => best = Some((score, candidate)),
            }
        }

        best.map(|(_, candidate)| candidate)
    }

    /// Updates the reference weights after a pivot.
    ///
    /// `pivot_row` holds the entries of the leaving row of `B^-1 A` for the
    /// nonbasic columns, which the iteration computes anyway in order to run
    /// the ratio test and to build the update. `alpha` is that row's entry in
    /// the entering column, which is the pivot.
    pub fn update(
        &mut self,
        entering: usize,
        leaving: usize,
        pivot_row: &[(usize, f64)],
        alpha: f64,
    ) {
        if self.rule != Rule::Devex || alpha == 0.0 {
            return;
        }
        let entering_weight = self.weights[entering];
        let mut largest = 0.0f64;

        for &(column, value) in pivot_row {
            if column == entering {
                continue;
            }
            let ratio = value / alpha;
            let candidate = ratio * ratio * entering_weight;
            if candidate > self.weights[column] {
                self.weights[column] = candidate;
            }
            largest = largest.max(self.weights[column]);
        }

        // The column that left takes the entering column's weight scaled by
        // the pivot, and never drops below one.
        let leaving_weight = (entering_weight / (alpha * alpha)).max(1.0);
        self.weights[leaving] = leaving_weight;
        self.weights[entering] = 1.0;
        largest = largest.max(leaving_weight);

        if largest > WEIGHT_CEILING {
            self.reset();
        }
    }
}

/// The reduced cost of a column under a given objective.
#[must_use]
pub fn reduced_cost(problem: &Problem, multipliers: &[f64], costs: &[f64], column: usize) -> f64 {
    let mut reduced = costs[column];
    for &(row, coefficient) in problem.column(column) {
        reduced -= multipliers[row as usize] * coefficient;
    }
    reduced
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
        let model = model(source);
        Problem::unscaled(&Working::from_model(&model))
    }

    /// Two structural columns, both improving, with different steepness.
    fn sample() -> Problem {
        problem(concat!(
            "ROWS\n N obj\n L r1\n L r2\n",
            "COLUMNS\n x obj -1 r1 1\n x r2 1\n y obj -3 r1 1\n y r2 1\n",
            "RHS\n rhs r1 6 r2 4\n",
            "BOUNDS\n UP b x 10\n UP b y 10\n",
            "ENDATA\n"
        ))
    }

    fn costs(problem: &Problem) -> Vec<f64> {
        (0..problem.columns())
            .map(|index| problem.cost(index))
            .collect()
    }

    #[test]
    fn nothing_is_chosen_when_nothing_improves() {
        let problem = sample();
        let basis = Basis::logical(&problem);
        let inverse = basis.factor(&problem, Tol::solver()).expect("nonsingular");
        let multipliers = basis.multipliers(&problem, &inverse);
        // Under a zero objective every reduced cost is zero.
        let zero = vec![0.0; problem.columns()];
        let pricing = Pricing::new(Rule::Devex, problem.columns());
        assert_eq!(
            pricing.choose(&problem, &basis, &multipliers, &zero, Tol::solver()),
            None
        );
    }

    #[test]
    fn the_textbook_rule_takes_the_steepest_reduced_cost() {
        let problem = sample();
        let basis = Basis::logical(&problem);
        let inverse = basis.factor(&problem, Tol::solver()).expect("nonsingular");
        let multipliers = basis.multipliers(&problem, &inverse);
        let pricing = Pricing::new(Rule::Dantzig, problem.columns());
        let chosen = pricing
            .choose(
                &problem,
                &basis,
                &multipliers,
                &costs(&problem),
                Tol::solver(),
            )
            .expect("something improves");
        assert_eq!(chosen.column, 1, "the steeper column is the second one");
        assert_eq!(chosen.direction, 1.0);
        assert!((chosen.reduced_cost - -3.0).abs() <= Tol::solver().opt);
    }

    #[test]
    fn the_lowest_index_rule_takes_the_first_eligible_column() {
        let problem = sample();
        let basis = Basis::logical(&problem);
        let inverse = basis.factor(&problem, Tol::solver()).expect("nonsingular");
        let multipliers = basis.multipliers(&problem, &inverse);
        let pricing = Pricing::new(Rule::Bland, problem.columns());
        let chosen = pricing
            .choose(
                &problem,
                &basis,
                &multipliers,
                &costs(&problem),
                Tol::solver(),
            )
            .expect("something improves");
        assert_eq!(
            chosen.column, 0,
            "the lowest eligible index, whatever its steepness"
        );
    }

    #[test]
    fn weights_change_which_column_the_reference_rule_takes() {
        let problem = sample();
        let basis = Basis::logical(&problem);
        let inverse = basis.factor(&problem, Tol::solver()).expect("nonsingular");
        let multipliers = basis.multipliers(&problem, &inverse);

        // With every weight at one, the reference rule agrees with the
        // textbook one.
        let mut pricing = Pricing::new(Rule::Devex, problem.columns());
        let chosen = pricing
            .choose(
                &problem,
                &basis,
                &multipliers,
                &costs(&problem),
                Tol::solver(),
            )
            .expect("something improves");
        assert_eq!(chosen.column, 1);

        // Give the steeper column a weight saying it barely moves, and the
        // rule prefers the other one — which is the entire point of it.
        pricing.update(1, 2, &[(0, 100.0)], 1.0);
        assert!(pricing.weight(0) > 1.0);
        pricing.update(0, 3, &[(1, 1000.0)], 1.0);
        let chosen = pricing
            .choose(
                &problem,
                &basis,
                &multipliers,
                &costs(&problem),
                Tol::solver(),
            )
            .expect("something improves");
        assert_eq!(
            chosen.column, 0,
            "the heavier column should have been passed over"
        );
    }

    #[test]
    fn a_tie_leaves_the_lower_index_in_place() {
        let tied = problem(concat!(
            "ROWS\n N obj\n L r\n",
            "COLUMNS\n a obj -2 r 1\n b obj -2 r 1\n",
            "RHS\n rhs r 5\n",
            "BOUNDS\n UP b a 5\n UP b b 5\n",
            "ENDATA\n"
        ));
        let basis = Basis::logical(&tied);
        let inverse = basis.factor(&tied, Tol::solver()).expect("nonsingular");
        let multipliers = basis.multipliers(&tied, &inverse);
        for rule in [Rule::Devex, Rule::Dantzig, Rule::Bland] {
            let pricing = Pricing::new(rule, tied.columns());
            let chosen = pricing
                .choose(&tied, &basis, &multipliers, &costs(&tied), Tol::solver())
                .expect("something improves");
            assert_eq!(chosen.column, 0, "under {}", rule.as_str());
        }
    }

    #[test]
    fn a_column_at_its_upper_bound_improves_by_falling() {
        let problem = problem(concat!(
            "ROWS\n N obj\n L r\n",
            "COLUMNS\n a obj 5 r 1\n",
            "RHS\n rhs r 100\n",
            "BOUNDS\n LO b a -9 \n UP b a 1\n",
            "ENDATA\n"
        ));
        let basis = Basis::logical(&problem);
        assert_eq!(basis.status(0), Status::AtUpper);
        let inverse = basis.factor(&problem, Tol::solver()).expect("nonsingular");
        let multipliers = basis.multipliers(&problem, &inverse);
        let pricing = Pricing::new(Rule::Devex, problem.columns());
        let chosen = pricing
            .choose(
                &problem,
                &basis,
                &multipliers,
                &costs(&problem),
                Tol::solver(),
            )
            .expect("something improves");
        assert_eq!(chosen.column, 0);
        assert_eq!(
            chosen.direction, -1.0,
            "falling is the improving direction here"
        );
    }

    #[test]
    fn a_fixed_column_is_never_chosen() {
        let problem = problem(concat!(
            "ROWS\n N obj\n L r\n",
            "COLUMNS\n a obj -5 r 1\n b obj -9 r 1\n",
            "RHS\n rhs r 100\n",
            "BOUNDS\n UP b a 5\n FX b b 2\n",
            "ENDATA\n"
        ));
        let basis = Basis::logical(&problem);
        assert_eq!(basis.status(1), Status::Fixed);
        let inverse = basis.factor(&problem, Tol::solver()).expect("nonsingular");
        let multipliers = basis.multipliers(&problem, &inverse);
        let pricing = Pricing::new(Rule::Devex, problem.columns());
        let chosen = pricing
            .choose(
                &problem,
                &basis,
                &multipliers,
                &costs(&problem),
                Tol::solver(),
            )
            .expect("something improves");
        assert_eq!(
            chosen.column, 0,
            "the fixed column has the better cost and cannot move"
        );
    }

    #[test]
    fn the_reference_framework_restarts_when_the_weights_run_away() {
        let mut pricing = Pricing::new(Rule::Devex, 4);
        assert_eq!(pricing.resets(), 0);
        // A minute pivot inflates the leaving column's weight enormously.
        // The magnitude is named rather than written, because a bare
        // scientific literal is indistinguishable from a tolerance to the scan
        // that keeps tolerances out of call sites.
        let minute = Tol::solver().opt;
        pricing.update(0, 1, &[(2, 1.0)], minute);
        assert_eq!(pricing.resets(), 1);
        assert_eq!(
            pricing.weight(1),
            1.0,
            "a restart puts every weight back to one"
        );
    }

    #[test]
    fn switching_rules_restarts_the_weights() {
        let mut pricing = Pricing::new(Rule::Devex, 4);
        pricing.update(0, 1, &[(2, 4.0)], 1.0);
        assert!(pricing.weight(2) > 1.0);
        pricing.switch(Rule::Bland);
        assert_eq!(pricing.rule(), Rule::Bland);
        assert_eq!(pricing.weight(2), 1.0);
        assert_eq!(Rule::Bland.as_str(), "bland");
    }

    #[test]
    fn the_reference_rule_ignores_updates_it_did_not_ask_for() {
        let mut pricing = Pricing::new(Rule::Dantzig, 4);
        pricing.update(0, 1, &[(2, 4.0)], 1.0);
        assert_eq!(pricing.weight(2), 1.0, "the textbook rule keeps no weights");
    }
}
