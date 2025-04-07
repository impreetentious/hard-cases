//! Replaying cut derivations, on the side that does not trust them.
//!
//! An invalid cut is the worst thing that can happen to this project. The
//! solver does not crash and does not slow down; it reports a smaller optimal
//! value than the truth, confidently, with a complete-looking tree behind it.
//! So a certificate's cut records carry the *derivation* and never the
//! conclusion: the combination that produced a Gomory cut, the cover and the
//! order a lifting ran in, and nothing else that is read as fact.
//!
//! Everything else this file computes for itself. The displacement each
//! variable is measured from comes from the node's own bounds by a fixed rule,
//! so a cut is valid inside its scope whatever the search believed about a
//! basis. The lifting coefficients come from a dynamic program run here, in
//! exact integers, and are compared against the record rather than taken from
//! it.
//!
//! The tableau view is definitional. For each row the verifier introduces an
//! activity variable standing for `a_i . x`, carrying that row's own bounds;
//! it is never something a certificate may assert a value for, and it exists
//! only so that a combination of row equations has something to be a
//! combination of.

use crate::vmodel::Instance;
use crate::vrat::Frac;
use crate::vreject::{Code, Rejected};

/// The bounds in force on every column at a node, activity columns included.
///
/// The first `columns` entries are the model's variables and the rest are the
/// activity variables, one per row. Branch assumptions on the path have already
/// been folded in, which is what makes a cut derived at a node a statement
/// about that node's region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// How many of the columns are the model's own variables.
    pub structural: usize,
    /// Every row of the relaxation: the model's own first, then the cuts
    /// visible at this node in the order they were introduced.
    ///
    /// A cut is a row of the relaxation, so it has an activity variable and
    /// can be part of a later combination exactly as a model row can. The
    /// order is the certificate's own record order, which is why both
    /// implementations agree about what row three is.
    pub rows: Vec<Row>,
    /// The lower bound of each column, where it has one.
    pub low: Vec<Option<Frac>>,
    /// The upper bound of each column, where it has one.
    pub high: Vec<Option<Frac>>,
    /// Whether each column is declared integer.
    pub whole: Vec<bool>,
}

/// One row of the relaxation, as the view holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The terms, over the model's variables.
    pub terms: Vec<(u32, Frac)>,
    /// The lower side, where it has one.
    pub low: Option<Frac>,
    /// The upper side, where it has one.
    pub high: Option<Frac>,
}

impl View {
    /// The definitional view of a model, before any node narrows it.
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        let structural = instance.columns.len();
        let mut low = Vec::new();
        let mut high = Vec::new();
        let mut whole = Vec::new();
        for column in &instance.columns {
            low.push(column.low.clone());
            high.push(column.high.clone());
            whole.push(column.whole);
        }

        let mut rows = Vec::with_capacity(instance.rows.len());
        for (index, row) in instance.rows.iter().enumerate() {
            let terms = instance
                .cells
                .iter()
                .filter(|cell| cell.row as usize == index)
                .map(|cell| (cell.column, cell.value.clone()))
                .collect();
            rows.push(Row {
                terms,
                low: row.low.clone(),
                high: row.high.clone(),
            });
            low.push(row.low.clone());
            high.push(row.high.clone());
            // An activity variable stands for a row's value. Declaring it
            // integer would be a claim about the row the model never made.
            whole.push(false);
        }

        Self {
            structural,
            rows,
            low,
            high,
            whole,
        }
    }

    /// Appends a cut as a row of the relaxation.
    ///
    /// A cut is written `a . x >= b`, so it has a lower side and no upper one.
    pub fn add_cut(&mut self, terms: Vec<(u32, Frac)>, right: Frac) {
        self.rows.push(Row {
            terms,
            low: Some(right.clone()),
            high: None,
        });
        self.low.push(Some(right));
        self.high.push(None);
        self.whole.push(false);
    }

    /// How many columns the view has.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.low.len()
    }

    /// Narrows a structural column's bounds, keeping the tighter of the two.
    pub fn narrow(&mut self, column: usize, low: Option<&Frac>, high: Option<&Frac>) {
        if column >= self.low.len() {
            return;
        }
        if let Some(value) = low {
            let tighter = match &self.low[column] {
                None => true,
                Some(seen) => value.compare(seen) == core::cmp::Ordering::Greater,
            };
            if tighter {
                self.low[column] = Some(value.clone());
            }
        }
        if let Some(value) = high {
            let tighter = match &self.high[column] {
                None => true,
                Some(seen) => value.compare(seen) == core::cmp::Ordering::Less,
            };
            if tighter {
                self.high[column] = Some(value.clone());
            }
        }
    }
}

/// The coefficient of a column in a combination of row equations.
///
/// A structural column picks up the row coefficients; the activity column of
/// row `i` picks up minus that row's multiplier, because the equation is
/// `a_i . x - s_i = 0`.
fn combined(view: &View, combination: &[(u64, Frac)]) -> Vec<Frac> {
    let mut alpha = vec![Frac::zero(); view.columns()];
    for (row, multiplier) in combination {
        if multiplier.is_zero() {
            continue;
        }
        let row = *row as usize;
        let Some(held) = view.rows.get(row) else {
            continue;
        };
        for (column, value) in &held.terms {
            let slot = &mut alpha[*column as usize];
            *slot = slot.plus(&value.times(multiplier));
        }
        let activity = view.structural + row;
        if activity < alpha.len() {
            alpha[activity] = alpha[activity].minus(multiplier);
        }
    }
    alpha
}

/// A cut, in the model's own variables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recomputed {
    /// The coefficients, ascending by column, with no zero among them.
    pub coefficients: Vec<(u32, Frac)>,
    /// The right-hand side, in the canonical `a . x >= b` sense.
    pub right: Frac,
}

/// Recomputes a Gomory cut from its combination.
///
/// # Errors
///
/// Returns [`Rejected`] naming the earliest rule that refused.
pub fn gomory(
    view: &View,
    combination: &[(u64, Frac)],
    isolated: u64,
) -> Result<Recomputed, Rejected> {
    for (row, _) in combination {
        if *row as usize >= view.rows.len() {
            return Err(Rejected::new(
                Code::SCOPE_TABLEAU_ROW,
                format!("the combination names row {row}, which is not visible here"),
            ));
        }
    }
    let alpha = combined(view, combination);

    let isolated = usize::try_from(isolated).map_err(|_| {
        Rejected::new(
            Code::CUT_NOT_ISOLATED,
            "the isolated column is not a column",
        )
    })?;
    if isolated >= view.structural || !view.whole[isolated] {
        return Err(Rejected::new(
            Code::CUT_NOT_ISOLATED,
            format!("column {isolated} is not an integer variable of the model"),
        ));
    }
    if alpha[isolated] != Frac::one() {
        return Err(Rejected::new(
            Code::CUT_NOT_ISOLATED,
            format!(
                "the combination gives column {isolated} the coefficient {}",
                alpha[isolated]
            ),
        ));
    }

    // Each remaining column enters as a displacement from one of its bounds,
    // chosen from the model rather than from any basis.
    let mut terms: Vec<(usize, bool, Frac, Frac, bool)> = Vec::new();
    let mut constant = Frac::zero();
    for (column, coefficient) in alpha.iter().enumerate() {
        if column == isolated || coefficient.is_zero() {
            continue;
        }
        let (from_lower, endpoint) = match (&view.low[column], &view.high[column]) {
            (Some(low), _) => (true, low.clone()),
            (None, Some(high)) => (false, high.clone()),
            (None, None) => {
                return Err(Rejected::new(
                    Code::CUT_FREE_NONBASIC,
                    format!("column {column} has no finite bound to displace from"),
                ));
            }
        };
        if view.whole[column] && !endpoint.is_whole() {
            return Err(Rejected::new(
                Code::CUT_FRACTIONAL_ENDPOINT,
                format!("column {column} is integer and its endpoint is {endpoint}"),
            ));
        }
        constant = constant.minus(&coefficient.times(&endpoint));
        let displaced = if from_lower {
            coefficient.clone()
        } else {
            coefficient.negated()
        };
        terms.push((column, from_lower, endpoint, displaced, view.whole[column]));
    }

    let f = constant.fractional();
    if f.is_zero() {
        return Err(Rejected::new(
            Code::CUT_FRACTION_RANGE,
            format!("the constant {constant} is an integer, so the cut is vacuous"),
        ));
    }
    let one_minus_f = Frac::one().minus(&f);

    // The formula, in displacement space.
    let mut coefficients: Vec<(usize, Frac)> = Vec::new();
    let mut right = f.clone();
    for (column, from_lower, endpoint, a, whole) in &terms {
        let value = if *whole {
            let f_j = a.fractional();
            if f_j.compare(&f) != core::cmp::Ordering::Greater {
                f_j
            } else {
                f.times(&Frac::one().minus(&f_j))
                    .divided(&one_minus_f)
                    .expect("f is strictly below one")
            }
        } else if a.is_minus() {
            f.negated()
                .times(a)
                .divided(&one_minus_f)
                .expect("f is strictly below one")
        } else {
            a.clone()
        };
        if value.is_zero() {
            continue;
        }
        // And back to the model's coordinates.
        if *from_lower {
            coefficients.push((*column, value.clone()));
            right = right.plus(&value.times(endpoint));
        } else {
            coefficients.push((*column, value.negated()));
            right = right.minus(&value.times(endpoint));
        }
    }

    // Activity columns stand for rows; expanding them leaves an inequality
    // over the model's own variables, which is what a proof can cite.
    let mut over = vec![Frac::zero(); view.structural];
    for (column, coefficient) in &coefficients {
        if *column < view.structural {
            over[*column] = over[*column].plus(coefficient);
            continue;
        }
        let Some(row) = view.rows.get(*column - view.structural) else {
            continue;
        };
        for (at, value) in &row.terms {
            let slot = &mut over[*at as usize];
            *slot = slot.plus(&coefficient.times(value));
        }
    }

    Ok(Recomputed {
        coefficients: over
            .into_iter()
            .enumerate()
            .filter(|(_, value)| !value.is_zero())
            .map(|(column, value)| (column as u32, value))
            .collect(),
        right,
    })
}

/// The most variables a lifting program will consider.
pub const DP_VARS: usize = 64;
/// The largest capacity a lifting program will consider.
pub const DP_CAPACITY: u64 = 4_096;
/// The most states a lifting program will build.
pub const DP_STATES: u64 = 1_000_000;

/// Recomputes a lifted cover cut.
///
/// # Errors
///
/// Returns [`Rejected`] naming the earliest rule that refused.
pub fn cover(
    instance: &Instance,
    view: &View,
    row: u64,
    members: &[u64],
    order: &[u64],
) -> Result<Recomputed, Rejected> {
    let row = usize::try_from(row)
        .ok()
        .filter(|at| *at < instance.rows.len());
    let Some(row) = row else {
        return Err(Rejected::new(
            Code::CUT_SOURCE_NOT_KNAPSACK,
            "the cover names a row that does not exist",
        ));
    };
    let capacity = whole(
        instance.rows[row]
            .high
            .as_ref()
            .ok_or_else(|| refuse_source("the row has no finite upper side"))?,
    )?;

    // Every column in the row must be a binary variable with a positive
    // integer weight. The derivation is stated for that shape and its dynamic
    // program enumerates zero-one states.
    let mut weights: Vec<(usize, u64)> = Vec::new();
    for cell in &instance.cells {
        if cell.row != row as u32 || cell.value.is_zero() {
            continue;
        }
        let column = cell.column as usize;
        if !binary(view, column) {
            return Err(Rejected::new(
                Code::CUT_SOURCE_NOT_BINARY,
                format!("column {column} in the source row is not binary"),
            ));
        }
        weights.push((column, whole(&cell.value)?));
    }
    weights.sort_unstable();

    let members: Vec<usize> = members
        .iter()
        .map(|value| usize::try_from(*value).unwrap_or(usize::MAX))
        .collect();
    let mut total = 0_u64;
    for column in &members {
        let Some((_, weight)) = weights.iter().find(|(at, _)| at == column) else {
            return Err(refuse_source("a cover member is not in the row"));
        };
        total = total.saturating_add(*weight);
    }
    if total <= capacity {
        return Err(Rejected::new(
            Code::CUT_COVER_NOT_STRICT,
            format!("the cover weighs {total} against a capacity of {capacity}"),
        ));
    }

    if weights.len() > DP_VARS
        || capacity > DP_CAPACITY
        || (weights.len() as u64).saturating_mul(capacity + 1) > DP_STATES
    {
        return Err(Rejected::new(
            Code::CUT_LIFT_CAPS,
            "the lifting program exceeds the caps that make it admissible",
        ));
    }

    let limit = members.len() as i64 - 1;
    let mut value: Vec<(usize, i64)> = members.iter().map(|column| (*column, 1)).collect();
    let mut lifted: Vec<(usize, i64)> = Vec::new();

    for raw in order {
        let column = usize::try_from(*raw).unwrap_or(usize::MAX);
        if members.contains(&column) || lifted.iter().any(|(at, _)| *at == column) {
            return Err(refuse_source("the lift order repeats a column"));
        }
        let Some((_, weight)) = weights.iter().find(|(at, _)| *at == column) else {
            return Err(refuse_source("a lifted column is not in the row"));
        };
        if *weight > capacity {
            return Err(Rejected::new(
                Code::CUT_LIFT_INFEASIBLE,
                format!("column {column} admits no feasible assignment and should be fixed"),
            ));
        }
        let best = maximum(&weights, &value, column, capacity - weight);
        let coefficient = limit - best;
        if coefficient < 0 {
            return Err(Rejected::new(
                Code::CUT_LIFT_SIGN,
                format!("column {column} lifts to {coefficient}"),
            ));
        }
        if coefficient > 0 {
            value.push((column, coefficient));
        }
        lifted.push((column, coefficient));
    }

    let mut coefficients: Vec<(u32, Frac)> = members
        .iter()
        .map(|column| (*column as u32, Frac::from_i64(-1)))
        .collect();
    for (column, coefficient) in lifted {
        if coefficient != 0 {
            coefficients.push((column as u32, Frac::from_i64(-coefficient)));
        }
    }
    coefficients.sort_by_key(|(column, _)| *column);
    Ok(Recomputed {
        coefficients,
        right: Frac::from_i64(-limit),
    })
}

/// Whether a column is a binary variable.
fn binary(view: &View, column: usize) -> bool {
    column < view.structural
        && view.whole[column]
        && view.low[column]
            .as_ref()
            .is_some_and(|value| *value == Frac::zero())
        && view.high[column]
            .as_ref()
            .is_some_and(|value| *value == Frac::one())
}

fn refuse_source(detail: &str) -> Rejected {
    Rejected::new(Code::CUT_SOURCE_NOT_KNAPSACK, detail)
}

/// A rational as a nonnegative machine integer.
fn whole(value: &Frac) -> Result<u64, Rejected> {
    if !value.is_whole() || value.is_minus() {
        return Err(refuse_source(
            "a weight or the capacity is not a positive integer",
        ));
    }
    value
        .top()
        .to_decimal()
        .parse()
        .map_err(|_| refuse_source("a weight does not fit in a machine word"))
}

/// The most the objective can reach inside a capacity, excluding one column.
fn maximum(weights: &[(usize, u64)], value: &[(usize, i64)], excluded: usize, room: u64) -> i64 {
    let capacity = usize::try_from(room).unwrap_or(0);
    let mut best = vec![0_i64; capacity + 1];
    for (column, weight) in weights {
        if *column == excluded {
            continue;
        }
        let Some((_, worth)) = value.iter().find(|(at, _)| at == column) else {
            continue;
        };
        let weight = usize::try_from(*weight).unwrap_or(usize::MAX);
        if weight > capacity {
            continue;
        }
        for room in (weight..=capacity).rev() {
            let candidate = best[room - weight] + worth;
            if candidate > best[room] {
                best[room] = candidate;
            }
        }
    }
    best[capacity]
}
