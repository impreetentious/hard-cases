//! A dense textbook simplex, written to be obviously correct and nothing else.
//!
//! This is the oracle. It exists so that the production solver has something
//! to disagree with, and every design choice here is made against that
//! purpose. The tableau is dense and is recomputed rather than updated. The
//! pricing rule is Bland's, which is the slowest rule available and the only
//! one that terminates without an anti-cycling argument. There is no
//! factorization, no scaling, no warm start, and no attempt at efficiency
//! anywhere.
//!
//! It shares no code with the production linear algebra, and a gate refuses
//! the build if an import ever appears. An oracle that imports the thing it
//! checks agrees with it exactly where agreement is worthless.
//!
//! The arithmetic is floating point and the result is therefore a *hint*. It
//! is used to notice disagreements, never to establish anything: a claim comes
//! from an exact certificate an independent checker accepted, and this file
//! produces neither.

use hc_core::{Model, RowId, VarId};
use hc_exact::Rational;

/// Reduced-cost fence for declaring the tableau optimal.
const PRICE_FENCE: f64 = 1e-9;
/// Smallest pivot magnitude the ratio test will admit.
const PIVOT_FENCE: f64 = 1e-9;
/// Residual fence for declaring the first phase feasible.
const FEASIBLE_FENCE: f64 = 1e-7;

/// What the reference solver concluded.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// A finite optimum, with the point that attains it.
    Optimal {
        /// The objective value, in the minimizing sense, including the
        /// model's objective constant.
        objective: f64,
        /// The point, in the original variable order.
        point: Vec<f64>,
    },
    /// No feasible point.
    Infeasible,
    /// Feasible points of arbitrarily small objective value.
    Unbounded,
    /// The iteration budget ran out.
    ///
    /// Bland's rule terminates, so this means the instance is larger than an
    /// oracle should be pointed at rather than that anything went wrong.
    Exhausted,
}

/// A linear program in the one shape the tableau understands.
///
/// Minimize `cost . y` subject to `matrix y = rhs` and `y >= 0`.
#[derive(Clone, Debug, Default)]
pub struct Standard {
    /// One row per equality.
    pub matrix: Vec<Vec<f64>>,
    /// The right-hand side.
    pub rhs: Vec<f64>,
    /// The objective coefficients.
    pub cost: Vec<f64>,
    /// A constant added to the objective, from the shifts below.
    pub constant: f64,
}

impl Standard {
    /// The number of columns.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.cost.len()
    }

    /// The number of equalities.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.matrix.len()
    }
}

/// How one model variable was expressed in the standard form.
#[derive(Clone, Copy, Debug)]
enum Recovery {
    /// `x = shift + y[at]`.
    Shifted { at: usize, shift: f64 },
    /// `x = shift - y[at]`, used when only the upper bound is finite.
    Mirrored { at: usize, shift: f64 },
    /// `x = y[plus] - y[minus]`, used when neither bound is finite.
    Split { plus: usize, minus: usize },
}

/// Solves the relaxation of a model, ignoring integrality.
///
/// Integrality is ignored deliberately and completely: this is the linear
/// oracle, and the integer oracle is exhaustive enumeration elsewhere.
#[must_use]
pub fn solve(model: &Model) -> Outcome {
    let (standard, recovery) = to_standard(model);
    let budget = 200 * (standard.rows() + standard.columns()).max(16);
    match simplex(&standard, budget) {
        Solved::Optimal { value, point } => {
            let mut original = vec![0.0; model.variables().len()];
            for (index, how) in recovery.iter().enumerate() {
                original[index] = match *how {
                    Recovery::Shifted { at, shift } => shift + point[at],
                    Recovery::Mirrored { at, shift } => shift - point[at],
                    Recovery::Split { plus, minus } => point[plus] - point[minus],
                };
            }
            Outcome::Optimal {
                objective: value + standard.constant,
                point: original,
            }
        }
        Solved::Infeasible => Outcome::Infeasible,
        Solved::Unbounded => Outcome::Unbounded,
        Solved::Exhausted => Outcome::Exhausted,
    }
}

/// Rewrites a model into the one shape the tableau understands.
///
/// Every variable becomes one or two nonnegative columns, every finite row
/// side becomes an equality with a nonnegative slack, and every finite
/// variable upper bound becomes a row of its own. That last choice is what
/// makes this a textbook implementation rather than a bounded-variable one:
/// carrying bounds implicitly is the first optimization a real simplex makes,
/// and not making it is the point.
fn to_standard(model: &Model) -> (Standard, Vec<Recovery>) {
    // The model's own objective constant, before any shift from a bounded
    // variable. Forgetting it moves every reported objective by a fixed amount
    // and changes no argmax, which is exactly the kind of error an oracle is
    // supposed to catch rather than commit.
    let mut standard = Standard {
        constant: model.objective_constant().to_binary64(),
        ..Standard::default()
    };
    let mut recovery = Vec::with_capacity(model.variables().len());
    let mut bound_rows: Vec<(usize, f64)> = Vec::new();

    for variable in model.variables() {
        let lower = variable.lower.as_ref().map(Rational::to_binary64);
        let upper = variable.upper.as_ref().map(Rational::to_binary64);
        let cost = variable.objective.to_binary64();
        match (lower, upper) {
            (Some(low), Some(high)) => {
                let at = standard.cost.len();
                standard.cost.push(cost);
                standard.constant += cost * low;
                recovery.push(Recovery::Shifted { at, shift: low });
                bound_rows.push((at, high - low));
            }
            (Some(low), None) => {
                let at = standard.cost.len();
                standard.cost.push(cost);
                standard.constant += cost * low;
                recovery.push(Recovery::Shifted { at, shift: low });
            }
            (None, Some(high)) => {
                let at = standard.cost.len();
                standard.cost.push(-cost);
                standard.constant += cost * high;
                recovery.push(Recovery::Mirrored { at, shift: high });
            }
            (None, None) => {
                let plus = standard.cost.len();
                standard.cost.push(cost);
                let minus = standard.cost.len();
                standard.cost.push(-cost);
                recovery.push(Recovery::Split { plus, minus });
            }
        }
    }

    // The column count is now fixed for the model's own variables. Slacks are
    // appended as rows are built, so each row is widened at the end.
    let by_row = model.matrix().by_row(model.rows().len());
    let mut rows: Vec<(Vec<f64>, f64)> = Vec::new();

    for (index, row) in model.rows().iter().enumerate() {
        let mut coefficients = vec![0.0; standard.cost.len()];
        let mut shift = 0.0;
        for (variable, value) in by_row.row(RowId(index as u32)) {
            let coefficient = value.to_binary64();
            match recovery[variable.index()] {
                Recovery::Shifted { at, shift: base } => {
                    coefficients[at] += coefficient;
                    shift += coefficient * base;
                }
                Recovery::Mirrored { at, shift: base } => {
                    coefficients[at] -= coefficient;
                    shift += coefficient * base;
                }
                Recovery::Split { plus, minus } => {
                    coefficients[plus] += coefficient;
                    coefficients[minus] -= coefficient;
                }
            }
        }

        let low = row.lower.as_ref().map(Rational::to_binary64);
        let high = row.upper.as_ref().map(Rational::to_binary64);
        match (low, high) {
            (Some(left), Some(right)) if left == right => {
                rows.push((coefficients, left - shift));
            }
            (left, right) => {
                if let Some(value) = left {
                    rows.push((coefficients.clone(), value - shift));
                }
                if let Some(value) = right {
                    rows.push((coefficients, value - shift));
                }
                // The slack signs are attached below, once the widths are
                // known; an equality row gets none.
            }
        }
    }

    // Rebuild with slacks. A row from a lower side needs a surplus subtracted;
    // a row from an upper side needs a slack added. The pass above lost which
    // is which, so it is recomputed here from the same information rather than
    // carried in a parallel structure that could drift.
    let mut signs: Vec<Option<f64>> = Vec::new();
    for row in model.rows() {
        let low = row.lower.as_ref().map(Rational::to_binary64);
        let high = row.upper.as_ref().map(Rational::to_binary64);
        match (low, high) {
            (Some(left), Some(right)) if left == right => signs.push(None),
            (left, right) => {
                if left.is_some() {
                    signs.push(Some(-1.0));
                }
                if right.is_some() {
                    signs.push(Some(1.0));
                }
            }
        }
    }
    debug_assert_eq!(signs.len(), rows.len());

    let structural = standard.cost.len();
    let slack_count = signs.iter().filter(|sign| sign.is_some()).count() + bound_rows.len();
    let width = structural + slack_count;
    let mut next_slack = structural;

    for (row, sign) in rows.into_iter().zip(signs) {
        let (mut coefficients, rhs) = row;
        coefficients.resize(width, 0.0);
        if let Some(direction) = sign {
            coefficients[next_slack] = direction;
            next_slack += 1;
        }
        standard.matrix.push(coefficients);
        standard.rhs.push(rhs);
    }

    for (column, span) in bound_rows {
        let mut coefficients = vec![0.0; width];
        coefficients[column] = 1.0;
        coefficients[next_slack] = 1.0;
        next_slack += 1;
        standard.matrix.push(coefficients);
        standard.rhs.push(span);
    }

    standard.cost.resize(width, 0.0);
    (standard, recovery)
}

/// What the tableau concluded, in its own coordinates.
enum Solved {
    Optimal { value: f64, point: Vec<f64> },
    Infeasible,
    Unbounded,
    Exhausted,
}

/// Two-phase simplex over a dense tableau, priced by Bland's rule.
fn simplex(problem: &Standard, budget: usize) -> Solved {
    let rows = problem.rows();
    let columns = problem.columns();
    if rows == 0 {
        // With no constraints the answer is decided entirely by the signs of
        // the objective coefficients on nonnegative variables.
        if problem.cost.iter().any(|value| *value < -PRICE_FENCE) {
            return Solved::Unbounded;
        }
        return Solved::Optimal {
            value: 0.0,
            point: vec![0.0; columns],
        };
    }

    // Make every right-hand side nonnegative so the artificial basis is
    // feasible to begin with.
    let mut tableau: Vec<Vec<f64>> = Vec::with_capacity(rows);
    for (row, &rhs) in problem.matrix.iter().zip(&problem.rhs) {
        let flip = rhs < 0.0;
        let mut widened: Vec<f64> = row.iter().map(|v| if flip { -v } else { *v }).collect();
        widened.resize(columns + rows + 1, 0.0);
        widened[columns + rows] = if flip { -rhs } else { rhs };
        tableau.push(widened);
    }
    for (index, row) in tableau.iter_mut().enumerate() {
        row[columns + index] = 1.0;
    }

    let mut basis: Vec<usize> = (0..rows).map(|index| columns + index).collect();
    let width = columns + rows;

    // First phase: drive the artificials to zero.
    let mut phase_one = vec![0.0; width];
    for slot in phase_one.iter_mut().skip(columns) {
        *slot = 1.0;
    }
    let mut spent = 0usize;
    match run(
        &mut tableau,
        &mut basis,
        &phase_one,
        width,
        budget,
        &mut spent,
    ) {
        Step::Optimal => {}
        Step::Unbounded => return Solved::Infeasible,
        Step::Exhausted => return Solved::Exhausted,
    }

    let residual: f64 = basis
        .iter()
        .enumerate()
        .filter(|(_, column)| **column >= columns)
        .map(|(row, _)| tableau[row][width].abs())
        .sum();
    if residual > FEASIBLE_FENCE {
        return Solved::Infeasible;
    }

    // Drive any artificial still in the basis out of it. A row whose
    // structural part is entirely zero is redundant and is dropped rather than
    // pivoted, which is the only place this implementation removes anything.
    let mut row = 0usize;
    while row < tableau.len() {
        if basis[row] < columns {
            row += 1;
            continue;
        }
        let pivot = (0..columns).find(|&column| tableau[row][column].abs() > PIVOT_FENCE);
        match pivot {
            Some(column) => {
                eliminate(&mut tableau, row, column);
                basis[row] = column;
                row += 1;
            }
            None => {
                tableau.remove(row);
                basis.remove(row);
            }
        }
    }

    // Second phase, on the real objective, with the artificials pinned out by
    // giving them no cost and never letting them re-enter.
    let mut phase_two = problem.cost.clone();
    phase_two.resize(width, 0.0);
    match run(
        &mut tableau,
        &mut basis,
        &phase_two,
        columns,
        budget,
        &mut spent,
    ) {
        Step::Optimal => {}
        Step::Unbounded => return Solved::Unbounded,
        Step::Exhausted => return Solved::Exhausted,
    }

    let mut point = vec![0.0; columns];
    for (row, &column) in basis.iter().enumerate() {
        if column < columns {
            point[column] = tableau[row][width];
        }
    }
    let value = point.iter().zip(&problem.cost).map(|(x, c)| x * c).sum();
    Solved::Optimal { value, point }
}

/// How one run of the pivot loop ended.
enum Step {
    Optimal,
    Unbounded,
    Exhausted,
}

/// Pivots until optimal, unbounded, or out of budget.
///
/// `pricing_width` limits which columns may enter, which is what keeps an
/// artificial from re-entering during the second phase.
fn run(
    tableau: &mut [Vec<f64>],
    basis: &mut [usize],
    cost: &[f64],
    pricing_width: usize,
    budget: usize,
    spent: &mut usize,
) -> Step {
    let width = tableau.first().map_or(0, |row| row.len() - 1);
    loop {
        if *spent >= budget {
            return Step::Exhausted;
        }
        *spent += 1;

        // Reduced costs, recomputed from scratch. An update would be faster
        // and would have a drift story; this has neither.
        let mut entering = None;
        for column in 0..pricing_width {
            let mut reduced = cost[column];
            for (row, &basic) in basis.iter().enumerate() {
                if cost[basic] != 0.0 {
                    reduced -= cost[basic] * tableau[row][column];
                }
            }
            if reduced < -PRICE_FENCE {
                // Bland's rule: the lowest eligible index, always. It is the
                // slowest choice and the one that cannot cycle.
                entering = Some(column);
                break;
            }
        }
        let Some(column) = entering else {
            return Step::Optimal;
        };

        let mut leaving: Option<(usize, f64)> = None;
        for row in 0..tableau.len() {
            let coefficient = tableau[row][column];
            if coefficient <= PIVOT_FENCE {
                continue;
            }
            let ratio = tableau[row][width] / coefficient;
            let better = match leaving {
                None => true,
                Some((current, best)) => {
                    ratio < best - PIVOT_FENCE
                        || ((ratio - best).abs() <= PIVOT_FENCE && basis[row] < basis[current])
                }
            };
            if better {
                leaving = Some((row, ratio));
            }
        }
        let Some((row, _)) = leaving else {
            return Step::Unbounded;
        };

        eliminate(tableau, row, column);
        basis[row] = column;
    }
}

/// Makes `column` a unit column with its one in `row`.
fn eliminate(tableau: &mut [Vec<f64>], row: usize, column: usize) {
    let pivot = tableau[row][column];
    for value in &mut tableau[row] {
        *value /= pivot;
    }
    for other in 0..tableau.len() {
        if other == row {
            continue;
        }
        let factor = tableau[other][column];
        if factor == 0.0 {
            continue;
        }
        for index in 0..tableau[other].len() {
            let subtract = factor * tableau[row][index];
            tableau[other][index] -= subtract;
        }
    }
}

/// Whether a point satisfies every row and bound of a model, within a fence.
///
/// The oracle's own answer is checked before it is believed. A reference
/// implementation that reports an infeasible point as optimal would send every
/// disagreement it produces to the wrong place.
#[must_use]
pub fn is_feasible(model: &Model, point: &[f64], fence: f64) -> bool {
    if point.len() != model.variables().len() {
        return false;
    }
    for (index, variable) in model.variables().iter().enumerate() {
        let value = point[index];
        if let Some(low) = &variable.lower
            && value < low.to_binary64() - fence
        {
            return false;
        }
        if let Some(high) = &variable.upper
            && value > high.to_binary64() + fence
        {
            return false;
        }
    }
    let by_row = model.matrix().by_row(model.rows().len());
    for (index, row) in model.rows().iter().enumerate() {
        let mut activity = 0.0;
        for (variable, value) in by_row.row(RowId(index as u32)) {
            activity += value.to_binary64() * point[variable.index()];
        }
        if let Some(low) = &row.lower
            && activity < low.to_binary64() - fence
        {
            return false;
        }
        if let Some(high) = &row.upper
            && activity > high.to_binary64() + fence
        {
            return false;
        }
    }
    true
}

/// The objective a point attains, including the model's constant.
#[must_use]
pub fn objective_at(model: &Model, point: &[f64]) -> f64 {
    let mut total = model.objective_constant().to_binary64();
    for (index, variable) in model.variables().iter().enumerate() {
        total += variable.objective.to_binary64() * point[index];
    }
    total
}

/// The column of a variable, for tests that need to name one.
#[must_use]
pub fn variable_named(model: &Model, name: &str) -> Option<VarId> {
    model
        .variables()
        .iter()
        .position(|variable| variable.name == name)
        .map(|index| VarId(index as u32))
}
