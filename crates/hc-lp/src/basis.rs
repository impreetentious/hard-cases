//! The basis, and where every column sits.
//!
//! A bounded-variable simplex tracks more than which columns are basic. A
//! nonbasic column sits at its lower bound, at its upper bound, at zero
//! because it has neither, or nowhere because its bounds coincide — and the
//! four cases price differently, move differently, and terminate differently.
//! Keeping them as one boolean and deciding later from the numbers is how a
//! bounded simplex acquires bugs that only appear on models with two-sided
//! rows.
//!
//! The status vector is the search's whole memory. Given it and a
//! factorization, every other quantity — the primal point, the row
//! multipliers, the reduced costs — is recomputed rather than carried, so
//! there is nothing to fall out of step with anything else.

use hc_core::Tol;

use crate::inverse::Inverse;
use crate::lu::Singular;
use crate::problem::Problem;

/// Where a column sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    /// In the basis; its value comes from the solve.
    Basic,
    /// Nonbasic at its lower bound.
    AtLower,
    /// Nonbasic at its upper bound.
    AtUpper,
    /// Nonbasic with no finite bound in either direction, held at zero.
    ///
    /// A free nonbasic column is unusual and worth naming: it may enter the
    /// basis in either direction, and a ratio test that assumed one direction
    /// would silently refuse half the moves available.
    Free,
    /// Nonbasic with coinciding bounds, which can never move.
    Fixed,
}

impl Status {
    /// Whether the column is in the basis.
    #[must_use]
    pub const fn is_basic(self) -> bool {
        matches!(self, Self::Basic)
    }

    /// Whether the column may increase from where it sits.
    #[must_use]
    pub const fn may_rise(self) -> bool {
        matches!(self, Self::AtLower | Self::Free)
    }

    /// Whether the column may decrease from where it sits.
    #[must_use]
    pub const fn may_fall(self) -> bool {
        matches!(self, Self::AtUpper | Self::Free)
    }

    /// The word this status appears as in a trace.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::AtLower => "at-lower",
            Self::AtUpper => "at-upper",
            Self::Free => "free",
            Self::Fixed => "fixed",
        }
    }
}

/// A basis: which columns are in it, in which row position, and where the rest
/// sit.
#[derive(Clone, Debug, PartialEq)]
pub struct Basis {
    status: Vec<Status>,
    /// The basic column at each row position.
    basic: Vec<usize>,
    /// For a basic column, its row position.
    position: Vec<Option<usize>>,
}

impl Basis {
    /// The all-logical basis: every row's logical column is basic, and every
    /// structural column sits at a bound.
    ///
    /// This is always a valid basis — the logical columns form an identity
    /// block up to sign — and it is where a cold start begins. It is usually
    /// primal infeasible, which is the first phase's problem.
    #[must_use]
    pub fn logical(problem: &Problem) -> Self {
        let mut status = Vec::with_capacity(problem.columns());
        for index in 0..problem.structural() {
            status.push(resting_place(problem, index));
        }
        let basic: Vec<usize> = (problem.structural()..problem.columns()).collect();
        for _ in &basic {
            status.push(Status::Basic);
        }
        let mut position = vec![None; problem.columns()];
        for (row, column) in basic.iter().enumerate() {
            position[*column] = Some(row);
        }
        Self {
            status,
            basic,
            position,
        }
    }

    /// Where a column sits.
    #[must_use]
    pub fn status(&self, column: usize) -> Status {
        self.status[column]
    }

    /// The basic column at each row position.
    #[must_use]
    pub fn basic(&self) -> &[usize] {
        &self.basic
    }

    /// The row position a column occupies, when it is basic.
    #[must_use]
    pub fn position_of(&self, column: usize) -> Option<usize> {
        self.position[column]
    }

    /// The number of columns.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.status.len()
    }

    /// Moves a nonbasic column to a different resting place.
    ///
    /// This is a bound flip: the column stays out of the basis and jumps from
    /// one bound to the other. It is the cheapest move a bounded simplex has
    /// and the one a textbook implementation does not have at all.
    ///
    /// # Panics
    ///
    /// Panics when the column is basic or the new status is.
    pub fn flip(&mut self, column: usize, to: Status) {
        assert!(
            !self.status[column].is_basic(),
            "a basic column cannot be flipped"
        );
        assert!(!to.is_basic(), "a flip cannot make a column basic");
        self.status[column] = to;
    }

    /// Replaces the basic column at a row position.
    ///
    /// # Panics
    ///
    /// Panics when the entering column is already basic, or when the leaving
    /// status is basic.
    pub fn pivot(&mut self, row: usize, entering: usize, leaving_status: Status) {
        assert!(
            !self.status[entering].is_basic(),
            "the entering column is already basic"
        );
        assert!(
            !leaving_status.is_basic(),
            "the leaving column must stop being basic"
        );
        let leaving = self.basic[row];
        self.status[leaving] = leaving_status;
        self.position[leaving] = None;
        self.basic[row] = entering;
        self.status[entering] = Status::Basic;
        self.position[entering] = Some(row);
    }

    /// The value a nonbasic column holds.
    ///
    /// # Panics
    ///
    /// Panics when the column is basic; a basic column's value comes from the
    /// solve and asking here would return a plausible wrong number.
    #[must_use]
    pub fn resting_value(&self, problem: &Problem, column: usize) -> f64 {
        match self.status[column] {
            Status::Basic => panic!("a basic column has no resting value"),
            Status::AtLower | Status::Fixed => problem.lower(column),
            Status::AtUpper => problem.upper(column),
            Status::Free => 0.0,
        }
    }

    /// Factors the basis.
    ///
    /// # Errors
    ///
    /// Returns [`Singular`] when the chosen columns are not independent.
    pub fn factor(&self, problem: &Problem, tol: Tol) -> Result<Inverse, Singular> {
        let columns: Vec<Vec<(u32, f64)>> = self
            .basic
            .iter()
            .map(|column| problem.column(*column).to_vec())
            .collect();
        Inverse::factor(&columns, problem.rows(), tol)
    }

    /// The primal point this basis and these resting places imply.
    ///
    /// The nonbasic columns are placed first, and the basic ones are whatever
    /// makes the system hold. Nothing is carried between iterations: this is
    /// recomputed, so it cannot drift away from the statuses that define it.
    ///
    /// # Panics
    ///
    /// Panics when the factorization is not the basis size.
    #[must_use]
    pub fn primal_point(&self, problem: &Problem, inverse: &Inverse) -> Vec<f64> {
        let mut point = vec![0.0; problem.columns()];
        let mut target = vec![0.0; problem.rows()];
        for (column, slot) in point.iter_mut().enumerate() {
            if self.status[column].is_basic() {
                continue;
            }
            let value = self.resting_value(problem, column);
            *slot = value;
            if value == 0.0 {
                continue;
            }
            for &(row, coefficient) in problem.column(column) {
                target[row as usize] -= coefficient * value;
            }
        }
        inverse.ftran(&mut target);
        for (row, column) in self.basic.iter().enumerate() {
            point[*column] = target[row];
        }
        point
    }

    /// The row multipliers this basis implies.
    ///
    /// These are the numbers a dual bound would be built from, and they are
    /// exactly why nothing here is a claim: they come from a floating-point
    /// solve, and an inference has to be reassembled in exact arithmetic
    /// before anything may be concluded from them.
    #[must_use]
    pub fn multipliers(&self, problem: &Problem, inverse: &Inverse) -> Vec<f64> {
        let mut costs: Vec<f64> = self
            .basic
            .iter()
            .map(|column| problem.cost(*column))
            .collect();
        inverse.btran(&mut costs);
        costs
    }

    /// The reduced cost of one column, given the row multipliers.
    #[must_use]
    pub fn reduced_cost(&self, problem: &Problem, multipliers: &[f64], column: usize) -> f64 {
        let mut reduced = problem.cost(column);
        for &(row, coefficient) in problem.column(column) {
            reduced -= multipliers[row as usize] * coefficient;
        }
        reduced
    }

    /// Whether every basic column sits inside its bounds, within a fence.
    #[must_use]
    pub fn is_primal_feasible(&self, problem: &Problem, point: &[f64], tol: Tol) -> bool {
        self.basic.iter().all(|column| {
            point[*column] >= problem.lower(*column) - tol.feas
                && point[*column] <= problem.upper(*column) + tol.feas
        })
    }
}

/// Where a nonbasic column rests when nothing else has been decided.
///
/// The finite bound nearer zero, or zero when neither is finite. A cold start
/// that placed every column at its lower bound would put a column bounded only
/// above at negative infinity, which is not a place.
fn resting_place(problem: &Problem, column: usize) -> Status {
    let low = problem.lower(column);
    let high = problem.upper(column);
    if low == high {
        return Status::Fixed;
    }
    match (low.is_finite(), high.is_finite()) {
        (true, true) => {
            if low.abs() <= high.abs() {
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
        let working = Working::from_model(&model);
        Problem::unscaled(&working)
    }

    fn sample() -> Problem {
        problem(concat!(
            "ROWS\n N obj\n L r1\n G r2\n",
            "COLUMNS\n x obj 2 r1 1\n x r2 1\n y obj -1 r1 3\n y r2 1\n",
            "RHS\n rhs r1 6 r2 1\n",
            "BOUNDS\n UP b x 4\n",
            "ENDATA\n"
        ))
    }

    #[test]
    fn the_cold_start_puts_every_logical_in_the_basis() {
        let problem = sample();
        let basis = Basis::logical(&problem);
        assert_eq!(basis.basic(), &[2, 3]);
        assert_eq!(basis.status(2), Status::Basic);
        assert_eq!(basis.status(3), Status::Basic);
        assert_eq!(basis.position_of(2), Some(0));
        assert_eq!(basis.position_of(3), Some(1));
        assert_eq!(basis.position_of(0), None);
        assert_eq!(basis.status(0), Status::AtLower);
        assert_eq!(basis.status(1), Status::AtLower);
    }

    #[test]
    fn a_column_rests_at_the_finite_bound_nearer_zero() {
        let mixed = problem(concat!(
            "ROWS\n N obj\n L r\n",
            "COLUMNS\n a obj 1 r 1\n b obj 1 r 1\n c obj 1 r 1\n d obj 1 r 1\n",
            "RHS\n rhs r 10\n",
            "BOUNDS\n",
            " LO b a -1\n UP b a 9\n",
            " MI b b\n UP b b -2\n",
            " FR b c\n",
            " FX b d 3\n",
            "ENDATA\n"
        ));
        let basis = Basis::logical(&mixed);
        // Bounded on both sides, the lower one nearer zero.
        assert_eq!(basis.status(0), Status::AtLower);
        // Bounded only above.
        assert_eq!(basis.status(1), Status::AtUpper);
        // Bounded on neither side.
        assert_eq!(basis.status(2), Status::Free);
        // Bounds coincide.
        assert_eq!(basis.status(3), Status::Fixed);
        assert_eq!(basis.resting_value(&mixed, 0), -1.0);
        assert_eq!(basis.resting_value(&mixed, 1), -2.0);
        assert_eq!(basis.resting_value(&mixed, 2), 0.0);
        assert_eq!(basis.resting_value(&mixed, 3), 3.0);
    }

    #[test]
    fn a_column_bounded_above_nearer_zero_rests_there() {
        let leaning = problem(concat!(
            "ROWS\n N obj\n L r\n",
            "COLUMNS\n a obj 1 r 1\n",
            "RHS\n rhs r 10\n",
            "BOUNDS\n LO b a -9\n UP b a 1\n",
            "ENDATA\n"
        ));
        let basis = Basis::logical(&leaning);
        assert_eq!(basis.status(0), Status::AtUpper);
        assert_eq!(basis.resting_value(&leaning, 0), 1.0);
    }

    #[test]
    fn the_statuses_say_which_directions_are_available() {
        assert!(Status::AtLower.may_rise() && !Status::AtLower.may_fall());
        assert!(Status::AtUpper.may_fall() && !Status::AtUpper.may_rise());
        assert!(Status::Free.may_rise() && Status::Free.may_fall());
        assert!(!Status::Fixed.may_rise() && !Status::Fixed.may_fall());
        assert!(Status::Basic.is_basic());
        assert_eq!(Status::AtLower.as_str(), "at-lower");
        assert_eq!(Status::Free.as_str(), "free");
    }

    #[test]
    fn the_cold_basis_factors_and_gives_the_point_its_statuses_imply() {
        let problem = sample();
        let basis = Basis::logical(&problem);
        let inverse = basis
            .factor(&problem, Tol::solver())
            .expect("the logical basis is a basis");
        let point = basis.primal_point(&problem, &inverse);
        // Both structural columns rest at zero, so both row activities are
        // zero and the logicals take that value.
        assert_eq!(point, vec![0.0, 0.0, 0.0, 0.0]);
        assert_eq!(problem.residual_at(&point), vec![0.0, 0.0]);
        // The second row demands an activity of at least one, so this basis is
        // not primal feasible — which is what the first phase exists for.
        assert!(!basis.is_primal_feasible(&problem, &point, Tol::solver()));
    }

    #[test]
    fn a_pivot_moves_one_column_in_and_one_out() {
        let problem = sample();
        let mut basis = Basis::logical(&problem);
        basis.pivot(1, 0, Status::AtUpper);
        assert_eq!(basis.basic(), &[2, 0]);
        assert_eq!(basis.status(0), Status::Basic);
        assert_eq!(basis.position_of(0), Some(1));
        assert_eq!(basis.status(3), Status::AtUpper);
        assert_eq!(basis.position_of(3), None);
    }

    #[test]
    fn a_flip_keeps_a_column_out_of_the_basis() {
        let problem = sample();
        let mut basis = Basis::logical(&problem);
        assert_eq!(basis.status(0), Status::AtLower);
        basis.flip(0, Status::AtUpper);
        assert_eq!(basis.status(0), Status::AtUpper);
        assert_eq!(basis.resting_value(&problem, 0), 4.0);
        assert_eq!(basis.position_of(0), None);
    }

    #[test]
    #[should_panic(expected = "already basic")]
    fn pivoting_a_basic_column_in_is_a_defect() {
        let problem = sample();
        let mut basis = Basis::logical(&problem);
        basis.pivot(0, 2, Status::AtLower);
    }

    #[test]
    #[should_panic(expected = "no resting value")]
    fn asking_a_basic_column_where_it_rests_is_a_defect() {
        let problem = sample();
        let basis = Basis::logical(&problem);
        let _ = basis.resting_value(&problem, 2);
    }

    #[test]
    fn the_point_after_a_pivot_satisfies_the_system() {
        let problem = sample();
        let mut basis = Basis::logical(&problem);
        basis.pivot(1, 0, Status::AtLower);
        let inverse = basis.factor(&problem, Tol::solver()).expect("nonsingular");
        let point = basis.primal_point(&problem, &inverse);
        let residual = problem.residual_at(&point);
        assert!(
            residual
                .iter()
                .all(|value| value.abs() <= Tol::solver().feas),
            "{residual:?}"
        );
        // The logical of the second row now rests at its lower bound of one,
        // so the structural column that replaced it must carry that activity.
        assert_eq!(point[3], 1.0);
        assert!((point[0] - 1.0).abs() <= Tol::solver().feas);
    }

    #[test]
    fn reduced_costs_are_zero_on_the_basic_columns() {
        // This is the identity that makes pricing meaningful, and it holds by
        // construction of the multipliers rather than by luck.
        let problem = sample();
        let mut basis = Basis::logical(&problem);
        basis.pivot(1, 0, Status::AtLower);
        let inverse = basis.factor(&problem, Tol::solver()).expect("nonsingular");
        let multipliers = basis.multipliers(&problem, &inverse);
        for column in basis.basic() {
            let reduced = basis.reduced_cost(&problem, &multipliers, *column);
            assert!(
                reduced.abs() <= Tol::solver().feas,
                "column {column} priced {reduced}"
            );
        }
    }

    #[test]
    fn a_problem_with_no_rows_has_an_empty_basis() {
        let empty = problem("ROWS\n N obj\nCOLUMNS\nENDATA\n");
        let basis = Basis::logical(&empty);
        assert!(basis.basic().is_empty());
        assert_eq!(basis.columns(), 0);
        let inverse = basis.factor(&empty, Tol::solver()).expect("nonsingular");
        assert!(basis.primal_point(&empty, &inverse).is_empty());
        assert!(basis.is_primal_feasible(&empty, &[], Tol::solver()));
    }
}
