//! The eight transforms.
//!
//! Each one is three things that have to agree: a condition under which it may
//! be applied, an action producing the next model, and a pair of maps carrying
//! points between the two. The pair is what makes a reduction a proof step
//! rather than a simplification. Forward, every feasible point of the model
//! before maps to a feasible point of the model after, with objective no
//! greater. Backward, every feasible point of the model after lifts to a
//! feasible point of the model before, with objective exactly equal. Together
//! those give equal optima, which is why a bound proved on the reduced model
//! is a bound on the original.
//!
//! Everything here is exact and everything here is recomputed. A record states
//! its parameters and the hash of the model it produces; it never states the
//! resulting coefficients as fact, because a verifier that read them would be
//! checking the reduction against its own account of itself.
//!
//! The applicability conditions are written to be checkable rather than to be
//! generous. Where a condition is not met the transform simply does not apply,
//! and the model keeps whatever it had. There is no repair path: a transform
//! that adjusted itself to become applicable would be a different transform.

use hc_core::{Model, Variable};
use hc_exact::Rational;

use crate::draft::{Draft, tighter_lower, tighter_upper};

/// One transform, with the parameters that identify it.
///
/// Indices are positions in the model the transform is applied *to*. After it
/// runs, the model has different indices, and the next record's parameters are
/// in those.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule {
    /// A column whose bounds have closed on a single value.
    FixedColumn {
        /// The column.
        column: usize,
        /// The value it is fixed at.
        value: Rational,
    },
    /// A column no row mentions.
    EmptyColumn {
        /// The column.
        column: usize,
        /// The value it is set to.
        value: Rational,
    },
    /// A row with exactly one coefficient, which is a bound in disguise.
    SingletonRow {
        /// The row.
        row: usize,
        /// The one column it mentions.
        column: usize,
    },
    /// A free continuous column appearing in exactly one equality.
    FreeSingletonColumn {
        /// The row.
        row: usize,
        /// The column.
        column: usize,
    },
    /// A row every point satisfying the bounds already satisfies.
    RedundantRow {
        /// The row.
        row: usize,
    },
    /// A row whose side is exactly its extreme activity, forcing every term.
    ForcingRow {
        /// The row.
        row: usize,
        /// Whether the forcing side is the upper one.
        upper: bool,
    },
    /// A bound one row implies is tighter than the one the model states.
    TightenBound {
        /// The row the bound is derived from.
        row: usize,
        /// The column whose bound moves.
        column: usize,
        /// Whether the upper bound moves.
        upper: bool,
        /// The new bound.
        value: Rational,
    },
    /// Two exactly identical columns, replaced by their sum.
    DuplicateColumn {
        /// The column that stays, holding the sum.
        keep: usize,
        /// The column that goes.
        drop: usize,
    },
}

/// Why a transform does not apply.
///
/// Carrying the reason rather than a boolean is what lets a fixture assert
/// that a rule declined for the right cause. A rule that declined for the
/// wrong reason would pass a test that only checked it declined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inapplicable {
    /// An index names something the model does not have.
    NoSuchIndex,
    /// The condition the rule requires does not hold.
    Condition,
    /// A bound the rule needs is infinite.
    Infinite,
    /// The rule would not tighten anything.
    NoImprovement,
    /// The derivation produces an empty domain.
    ///
    /// Not a failure of the rule. The model has no feasible point and the
    /// chain ends with a contradiction naming this derivation.
    Contradiction,
}

impl core::fmt::Display for Inapplicable {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::NoSuchIndex => "an index names something the model does not have",
            Self::Condition => "the rule's condition does not hold",
            Self::Infinite => "a bound the rule needs is infinite",
            Self::NoImprovement => "the rule would not tighten anything",
            Self::Contradiction => "the derivation produces an empty domain",
        })
    }
}

impl core::error::Error for Inapplicable {}

/// The rule identifier and parameters a record carries for a transform.
///
/// The encoding is in the specification rather than here; this is the one
/// place that writes it, so a record and the verifier's reading of it cannot
/// drift through two copies of the ordering.
#[must_use]
pub fn encode(rule: &Rule) -> (u64, Vec<Rational>) {
    let index = |value: usize| Rational::from_i64(value as i64);
    let flag = |value: bool| Rational::from_i64(i64::from(value));
    match rule {
        Rule::FixedColumn { column, value } => (1, vec![index(*column), value.clone()]),
        Rule::EmptyColumn { column, value } => (2, vec![index(*column), value.clone()]),
        Rule::SingletonRow { row, column } => (3, vec![index(*row), index(*column)]),
        Rule::FreeSingletonColumn { row, column } => (4, vec![index(*row), index(*column)]),
        Rule::RedundantRow { row } => (5, vec![index(*row)]),
        Rule::ForcingRow { row, upper } => (6, vec![index(*row), flag(*upper)]),
        Rule::TightenBound {
            row,
            column,
            upper,
            value,
        } => (
            7,
            vec![index(*row), index(*column), flag(*upper), value.clone()],
        ),
        Rule::DuplicateColumn { keep, drop } => (8, vec![index(*keep), index(*drop)]),
    }
}

/// Applies a transform, producing the next model.
///
/// This is the function both implementations run. The solver runs it to
/// perform the reduction; the verifier runs it to recompute what the record
/// claims the reduction produced.
///
/// # Errors
///
/// Returns [`Inapplicable`] naming why the rule does not apply here.
pub fn apply(model: &Model, rule: &Rule) -> Result<Model, Inapplicable> {
    let mut draft = Draft::of(model);
    match rule {
        Rule::FixedColumn { column, value } => fixed_column(&mut draft, *column, value)?,
        Rule::EmptyColumn { column, value } => empty_column(&mut draft, *column, value)?,
        Rule::SingletonRow { row, column } => singleton_row(&mut draft, *row, *column)?,
        Rule::FreeSingletonColumn { row, column } => {
            free_singleton_column(&mut draft, *row, *column)?;
        }
        Rule::RedundantRow { row } => redundant_row(&mut draft, *row)?,
        Rule::ForcingRow { row, upper } => forcing_row(&mut draft, *row, *upper)?,
        Rule::TightenBound {
            row,
            column,
            upper,
            value,
        } => tighten_bound(&mut draft, *row, *column, *upper, value)?,
        Rule::DuplicateColumn { keep, drop } => duplicate_column(&mut draft, *keep, *drop)?,
    }
    Ok(draft.finish())
}

/// T1. A column whose effective bounds have closed on one value.
fn fixed_column(draft: &mut Draft, column: usize, value: &Rational) -> Result<(), Inapplicable> {
    let variable = draft
        .variables
        .get(column)
        .ok_or(Inapplicable::NoSuchIndex)?;
    let (low, high) = variable.effective_bounds();
    let (Some(low), Some(high)) = (low, high) else {
        return Err(Inapplicable::Infinite);
    };
    if low != *value || high != *value {
        return Err(Inapplicable::Condition);
    }
    if variable.integral && !value.is_integer() {
        return Err(Inapplicable::Condition);
    }

    // Substitute the value into every row the column touches and into the
    // objective constant, then delete it.
    let cost = variable.objective.clone();
    draft.constant = draft.constant.add(&cost.mul(value));
    for (row, coefficient) in draft.column_entries(column) {
        let shift = coefficient.mul(value);
        let side = &mut draft.rows[row];
        if let Some(bound) = &side.lower {
            side.lower = Some(bound.sub(&shift));
        }
        if let Some(bound) = &side.upper {
            side.upper = Some(bound.sub(&shift));
        }
    }
    draft.drop_column(column);
    Ok(())
}

/// The value T2 chooses for an empty column, when there is one.
#[must_use]
pub fn empty_column_value(variable: &Variable) -> Option<Rational> {
    let (low, high) = variable.effective_bounds();
    let zero = Rational::zero();
    let inside = low
        .as_ref()
        .is_none_or(|bound| bound.cmp_value(&zero).is_le())
        && high
            .as_ref()
            .is_none_or(|bound| bound.cmp_value(&zero).is_ge());

    if variable.objective.is_zero() {
        // Nothing to gain by moving, so rest at zero when zero is available
        // and at whichever endpoint exists otherwise.
        if inside {
            return Some(zero);
        }
        return low.or(high);
    }
    if variable.objective.is_positive() {
        low
    } else {
        high
    }
}

/// T2. A column no row mentions, set to its best available endpoint.
fn empty_column(draft: &mut Draft, column: usize, value: &Rational) -> Result<(), Inapplicable> {
    let variable = draft
        .variables
        .get(column)
        .ok_or(Inapplicable::NoSuchIndex)?;
    if !draft.column_entries(column).is_empty() {
        return Err(Inapplicable::Condition);
    }
    // A missing improving endpoint makes the rule inapplicable. Presolve does
    // not get to decide the model is unbounded; the solve must establish both
    // a feasible original point and a ray.
    let chosen = empty_column_value(variable).ok_or(Inapplicable::Infinite)?;
    if chosen != *value {
        return Err(Inapplicable::Condition);
    }

    let cost = variable.objective.clone();
    draft.constant = draft.constant.add(&cost.mul(value));
    draft.drop_column(column);
    Ok(())
}

/// The bounds a singleton row implies for its one column.
///
/// Returns the derived lower and upper bounds, already rounded for an integer
/// column. Either may be absent when the corresponding row side is.
#[must_use]
pub fn singleton_bounds(
    coefficient: &Rational,
    lower: Option<&Rational>,
    upper: Option<&Rational>,
    integral: bool,
) -> (Option<Rational>, Option<Rational>) {
    let positive = coefficient.is_positive();
    let divide = |value: &Rational| value.mul(&coefficient.recip().expect("a nonzero coefficient"));

    // Dividing by a negative coefficient swaps the sides, which is the whole
    // of what makes this rule easy to get wrong.
    let (low, high) = if positive {
        (lower.map(divide), upper.map(divide))
    } else {
        (upper.map(divide), lower.map(divide))
    };

    if !integral {
        return (low, high);
    }
    (
        low.map(|value| Rational::from_int(value.ceil())),
        high.map(|value| Rational::from_int(value.floor())),
    )
}

/// T3. A row with one coefficient, which is a bound written as a row.
fn singleton_row(draft: &mut Draft, row: usize, column: usize) -> Result<(), Inapplicable> {
    if row >= draft.rows.len() || column >= draft.variables.len() {
        return Err(Inapplicable::NoSuchIndex);
    }
    let entries = draft.row_entries(row);
    if entries.len() != 1 || entries[0].0 != column {
        return Err(Inapplicable::Condition);
    }
    let coefficient = entries[0].1.clone();
    if coefficient.is_zero() {
        return Err(Inapplicable::Condition);
    }

    let integral = draft.variables[column].integral;
    let (low, high) = singleton_bounds(
        &coefficient,
        draft.rows[row].lower.as_ref(),
        draft.rows[row].upper.as_ref(),
        integral,
    );

    let variable = &mut draft.variables[column];
    if let Some(value) = &low {
        variable.lower = Some(tighter_lower(variable.lower.as_ref(), value));
    }
    if let Some(value) = &high {
        variable.upper = Some(tighter_upper(variable.upper.as_ref(), value));
    }
    if let (Some(l), Some(u)) = (&variable.lower, &variable.upper) {
        if l.cmp_value(u).is_gt() {
            return Err(Inapplicable::Contradiction);
        }
    }

    draft.drop_row(row);
    Ok(())
}

/// T4. A free continuous column appearing in exactly one equality.
fn free_singleton_column(draft: &mut Draft, row: usize, column: usize) -> Result<(), Inapplicable> {
    let variable = draft
        .variables
        .get(column)
        .ok_or(Inapplicable::NoSuchIndex)?;
    if variable.integral || variable.lower.is_some() || variable.upper.is_some() {
        return Err(Inapplicable::Condition);
    }
    let appearances = draft.column_entries(column);
    if appearances.len() != 1 || appearances[0].0 != row {
        return Err(Inapplicable::Condition);
    }
    let coefficient = appearances[0].1.clone();
    if coefficient.is_zero() {
        return Err(Inapplicable::Condition);
    }
    let side = draft.rows.get(row).ok_or(Inapplicable::NoSuchIndex)?;
    let (Some(low), Some(high)) = (side.lower.as_ref(), side.upper.as_ref()) else {
        return Err(Inapplicable::Condition);
    };
    if low != high {
        return Err(Inapplicable::Condition);
    }
    let right = low.clone();

    // x_j = (b_i - sum of the other terms) / a_ij, substituted into the
    // objective. Every other column in the row picks up a share of the
    // objective coefficient, and the constant picks up the rest.
    let cost = variable.objective.clone();
    let scale = cost.mul(&coefficient.recip().expect("a nonzero coefficient"));
    draft.constant = draft.constant.add(&scale.mul(&right));
    for (other, value) in draft.row_entries(row) {
        if other == column {
            continue;
        }
        let adjust = scale.mul(&value);
        let slot = &mut draft.variables[other];
        slot.objective = slot.objective.sub(&adjust);
    }

    draft.drop_row(row);
    draft.drop_column(column);
    Ok(())
}

/// The exact extreme activities of a row over the current bounds.
///
/// Absent where an infinite bound makes the extremum unbounded, which is
/// exactly the case in which nothing may be concluded from it.
#[must_use]
pub fn activity_range(draft: &Draft, row: usize) -> (Option<Rational>, Option<Rational>) {
    let mut minimum = Some(Rational::zero());
    let mut maximum = Some(Rational::zero());
    for (column, coefficient) in draft.row_entries(row) {
        let (low, high) = draft.variables[column].effective_bounds();
        let (towards_min, towards_max) = if coefficient.is_positive() {
            (low, high)
        } else {
            (high, low)
        };
        minimum = match (minimum, towards_min) {
            (Some(total), Some(bound)) => Some(total.add(&coefficient.mul(&bound))),
            _ => None,
        };
        maximum = match (maximum, towards_max) {
            (Some(total), Some(bound)) => Some(total.add(&coefficient.mul(&bound))),
            _ => None,
        };
    }
    (minimum, maximum)
}

/// T5. A row every point inside the bounds already satisfies.
fn redundant_row(draft: &mut Draft, row: usize) -> Result<(), Inapplicable> {
    let side = draft
        .rows
        .get(row)
        .ok_or(Inapplicable::NoSuchIndex)?
        .clone();
    let (minimum, maximum) = activity_range(draft, row);

    if let Some(bound) = &side.lower {
        let Some(minimum) = &minimum else {
            return Err(Inapplicable::Infinite);
        };
        if minimum.cmp_value(bound).is_lt() {
            return Err(Inapplicable::Condition);
        }
    }
    if let Some(bound) = &side.upper {
        let Some(maximum) = &maximum else {
            return Err(Inapplicable::Infinite);
        };
        if maximum.cmp_value(bound).is_gt() {
            return Err(Inapplicable::Condition);
        }
    }

    draft.drop_row(row);
    Ok(())
}

/// T6. A row whose side sits exactly at its extreme activity.
fn forcing_row(draft: &mut Draft, row: usize, upper: bool) -> Result<(), Inapplicable> {
    let side = draft
        .rows
        .get(row)
        .ok_or(Inapplicable::NoSuchIndex)?
        .clone();
    let (minimum, maximum) = activity_range(draft, row);

    // A lower side at the maximum activity forces every term to its
    // contributing extreme; an upper side at the minimum does the same the
    // other way.
    let (bound, extreme) = if upper {
        (side.upper.as_ref(), minimum.as_ref())
    } else {
        (side.lower.as_ref(), maximum.as_ref())
    };
    let (Some(bound), Some(extreme)) = (bound, extreme) else {
        return Err(Inapplicable::Infinite);
    };
    if bound != extreme {
        return Err(Inapplicable::Condition);
    }

    let entries = draft.row_entries(row);
    if entries.is_empty() {
        return Err(Inapplicable::Condition);
    }
    for (column, coefficient) in entries {
        let (low, high) = draft.variables[column].effective_bounds();
        // On a lower side at the maximum, a positive coefficient is pinned at
        // its upper endpoint. On an upper side at the minimum, that reverses.
        let towards_high = coefficient.is_positive() != upper;
        let endpoint = if towards_high { high } else { low };
        let Some(endpoint) = endpoint else {
            return Err(Inapplicable::Infinite);
        };
        let variable = &mut draft.variables[column];
        variable.lower = Some(tighter_lower(variable.lower.as_ref(), &endpoint));
        variable.upper = Some(tighter_upper(variable.upper.as_ref(), &endpoint));
    }
    Ok(())
}

/// The bound a row implies for a column, on whichever side it lands.
#[must_use]
pub fn derived_bound(
    draft: &Draft,
    row: usize,
    column: usize,
    from_upper_side: bool,
) -> Option<(bool, Rational)> {
    let entries = draft.row_entries(row);
    let coefficient = entries
        .iter()
        .find(|(at, _)| *at == column)
        .map(|(_, value)| value.clone())?;
    if coefficient.is_zero() {
        return None;
    }
    let side = if from_upper_side {
        draft.rows[row].upper.as_ref()?
    } else {
        draft.rows[row].lower.as_ref()?
    };

    let mut residual = Rational::zero();
    for (other, value) in &entries {
        if *other == column {
            continue;
        }
        let (low, high) = draft.variables[*other].effective_bounds();
        // Minimise the residual against an upper side, maximise against a
        // lower one.
        let towards = if value.is_positive() == from_upper_side {
            low
        } else {
            high
        };
        residual = residual.add(&value.mul(&towards?));
    }

    let free = side.sub(&residual).mul(&coefficient.recip()?);
    let bounds_upper = if coefficient.is_positive() {
        from_upper_side
    } else {
        !from_upper_side
    };
    let rounded = if draft.variables[column].integral {
        if bounds_upper {
            Rational::from_int(free.floor())
        } else {
            Rational::from_int(free.ceil())
        }
    } else {
        free
    };
    Some((bounds_upper, rounded))
}

/// T7. A bound one row implies, tighter than the one the model states.
fn tighten_bound(
    draft: &mut Draft,
    row: usize,
    column: usize,
    upper: bool,
    value: &Rational,
) -> Result<(), Inapplicable> {
    if row >= draft.rows.len() || column >= draft.variables.len() {
        return Err(Inapplicable::NoSuchIndex);
    }
    // The record says which side moves; the derivation says which side a given
    // row side lands on. Both row sides are tried and the one that lands where
    // the record says is the one checked.
    let derived = [true, false]
        .into_iter()
        .filter_map(|from_upper| derived_bound(draft, row, column, from_upper))
        .find(|(lands_upper, _)| *lands_upper == upper)
        .ok_or(Inapplicable::Infinite)?
        .1;
    if derived != *value {
        return Err(Inapplicable::Condition);
    }

    let variable = &mut draft.variables[column];
    let improves = if upper {
        variable
            .upper
            .as_ref()
            .is_none_or(|current| derived.cmp_value(current).is_lt())
    } else {
        variable
            .lower
            .as_ref()
            .is_none_or(|current| derived.cmp_value(current).is_gt())
    };
    if !improves {
        return Err(Inapplicable::NoImprovement);
    }
    if upper {
        variable.upper = Some(derived);
    } else {
        variable.lower = Some(derived);
    }
    if let (Some(l), Some(u)) = (&variable.lower, &variable.upper) {
        if l.cmp_value(u).is_gt() {
            return Err(Inapplicable::Contradiction);
        }
    }
    Ok(())
}

/// T8. Two exactly identical columns, replaced by their sum.
fn duplicate_column(draft: &mut Draft, keep: usize, drop: usize) -> Result<(), Inapplicable> {
    if keep == drop || keep >= draft.variables.len() || drop >= draft.variables.len() {
        return Err(Inapplicable::NoSuchIndex);
    }
    let left = draft.variables[keep].clone();
    let right = draft.variables[drop].clone();
    if left.objective != right.objective || left.integral != right.integral {
        return Err(Inapplicable::Condition);
    }
    if draft.column_entries(keep) != draft.column_entries(drop) {
        return Err(Inapplicable::Condition);
    }
    let (Some(low_left), Some(high_left)) = (left.lower.clone(), left.upper.clone()) else {
        return Err(Inapplicable::Infinite);
    };
    let (Some(low_right), Some(high_right)) = (right.lower.clone(), right.upper.clone()) else {
        return Err(Inapplicable::Infinite);
    };

    // The sum variable. Two binaries become a general integer over zero to
    // two; keeping it binary would drop the point where both were one.
    draft.variables[keep].lower = Some(low_left.add(&low_right));
    draft.variables[keep].upper = Some(high_left.add(&high_right));
    draft.drop_column(drop);
    Ok(())
}
