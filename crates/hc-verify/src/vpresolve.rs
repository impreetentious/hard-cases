//! Replaying presolve, on the side that does not trust it.
//!
//! A reduction chain is where a proof most easily stops being a proof. The
//! solver shrinks the model, proves a bound about the smaller one, and every
//! claim downstream rests on steps nobody rechecked. So this file is the second
//! implementation of the eight transforms, written against the specification
//! rather than against the other one, and it recomputes each stage from the one
//! before rather than reading what the record says the result was.
//!
//! That distinction is the whole of it. A record carries parameters and the
//! hash of the model it claims to produce. The parameters that are indices
//! select what the rule is about; every other number in the record is compared
//! against what this code derives and is otherwise inert. A verifier that read
//! a stated coefficient would be checking the reduction against its own account
//! of itself.
//!
//! Applicability is checked, not assumed. A record naming a transform whose
//! condition does not hold on the model the verifier is holding is refused, and
//! refused before anything is rebuilt: the condition is what makes the step
//! true, and a step performed where its condition fails is a different model
//! wearing the same hash.

use crate::vmodel::{Cell, Column, Constraint, Instance};
use crate::vrat::Frac;
use crate::vreject::{Code, Rejected};

/// One reduction, as read from a record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reduction {
    /// A column whose bounds have closed on one value.
    Fixed {
        /// The column.
        column: usize,
        /// The value stated, which is compared against the bounds.
        stated: Frac,
    },
    /// A column no row mentions.
    Empty {
        /// The column.
        column: usize,
        /// The value stated, which is compared against the derived endpoint.
        stated: Frac,
    },
    /// A row with one coefficient.
    Singleton {
        /// The row.
        row: usize,
        /// The column.
        column: usize,
    },
    /// A free continuous column in exactly one equality.
    FreeColumn {
        /// The row.
        row: usize,
        /// The column.
        column: usize,
    },
    /// A row the bounds already imply.
    Redundant {
        /// The row.
        row: usize,
    },
    /// A row whose side sits at its extreme activity.
    Forcing {
        /// The row.
        row: usize,
        /// Whether the forcing side is the upper one.
        upper: bool,
    },
    /// A bound one row implies, tighter than the model's.
    Tighten {
        /// The row.
        row: usize,
        /// The column.
        column: usize,
        /// Whether the upper bound moves.
        upper: bool,
        /// The value stated, which is compared against the derivation.
        stated: Frac,
    },
    /// Two identical columns replaced by their sum.
    Aggregate {
        /// The column that stays.
        keep: usize,
        /// The column that goes.
        drop: usize,
    },
}

/// Reads a record's rule identifier and parameters.
///
/// # Errors
///
/// Returns [`Rejected`] when the identifier is outside the closed list or the
/// parameters are not the shape that rule takes.
pub fn read(rule: u64, parameters: &[Frac]) -> Result<Reduction, Rejected> {
    let index = |at: usize| -> Result<usize, Rejected> {
        let value = parameters.get(at).ok_or_else(|| {
            Rejected::new(
                Code::TRANSFORM_UNKNOWN_RULE,
                format!("rule {rule} is missing parameter {at}"),
            )
        })?;
        if !value.is_whole() || value.is_minus() {
            return Err(Rejected::new(
                Code::TRANSFORM_UNKNOWN_RULE,
                format!("parameter {at} of rule {rule} is {value}, which is not an index"),
            ));
        }
        value
            .top()
            .to_decimal()
            .parse::<usize>()
            .map_err(|_| Rejected::new(Code::TRANSFORM_UNKNOWN_RULE, "an index is too large"))
    };
    let flag = |at: usize| -> Result<bool, Rejected> { Ok(index(at)? == 1) };
    let value = |at: usize| -> Result<Frac, Rejected> {
        parameters.get(at).cloned().ok_or_else(|| {
            Rejected::new(
                Code::TRANSFORM_UNKNOWN_RULE,
                format!("rule {rule} is missing parameter {at}"),
            )
        })
    };

    let (reduction, arity) = match rule {
        1 => (
            Reduction::Fixed {
                column: index(0)?,
                stated: value(1)?,
            },
            2,
        ),
        2 => (
            Reduction::Empty {
                column: index(0)?,
                stated: value(1)?,
            },
            2,
        ),
        3 => (
            Reduction::Singleton {
                row: index(0)?,
                column: index(1)?,
            },
            2,
        ),
        4 => (
            Reduction::FreeColumn {
                row: index(0)?,
                column: index(1)?,
            },
            2,
        ),
        5 => (Reduction::Redundant { row: index(0)? }, 1),
        6 => (
            Reduction::Forcing {
                row: index(0)?,
                upper: flag(1)?,
            },
            2,
        ),
        7 => (
            Reduction::Tighten {
                row: index(0)?,
                column: index(1)?,
                upper: flag(2)?,
                stated: value(3)?,
            },
            4,
        ),
        8 => (
            Reduction::Aggregate {
                keep: index(0)?,
                drop: index(1)?,
            },
            2,
        ),
        other => {
            return Err(Rejected::new(
                Code::TRANSFORM_UNKNOWN_RULE,
                format!("rule {other} is outside the closed list"),
            ));
        }
    };
    if parameters.len() != arity {
        return Err(Rejected::new(
            Code::TRANSFORM_UNKNOWN_RULE,
            format!(
                "rule {rule} takes {arity} parameters and the record carries {}",
                parameters.len()
            ),
        ));
    }
    Ok(reduction)
}

/// A refusal from a transform, always the same rule.
fn refuse(detail: impl Into<String>) -> Rejected {
    Rejected::new(Code::TRANSFORM_NOT_APPLICABLE, detail)
}

/// The effective bounds of a column: rounded inward when it is integer.
#[must_use]
pub fn effective(column: &Column) -> (Option<Frac>, Option<Frac>) {
    if !column.whole {
        return (column.low.clone(), column.high.clone());
    }
    (
        column
            .low
            .as_ref()
            .map(|value| Frac::whole(value.ceiling())),
        column.high.as_ref().map(|value| Frac::whole(value.floor())),
    )
}

/// Every coefficient of one row, ascending by column.
#[must_use]
pub fn row_of(instance: &Instance, row: u32) -> Vec<(u32, Frac)> {
    let mut out: Vec<(u32, Frac)> = instance
        .cells
        .iter()
        .filter(|cell| cell.row == row)
        .map(|cell| (cell.column, cell.value.clone()))
        .collect();
    out.sort_by_key(|(column, _)| *column);
    out
}

/// Every coefficient of one column, ascending by row.
#[must_use]
pub fn column_of(instance: &Instance, column: u32) -> Vec<(u32, Frac)> {
    let mut out: Vec<(u32, Frac)> = instance
        .cells
        .iter()
        .filter(|cell| cell.column == column)
        .map(|cell| (cell.row, cell.value.clone()))
        .collect();
    out.sort_by_key(|(row, _)| *row);
    out
}

/// The exact extreme activities of a row over the current bounds.
#[must_use]
pub fn activity_range(instance: &Instance, row: u32) -> (Option<Frac>, Option<Frac>) {
    let mut minimum = Some(Frac::zero());
    let mut maximum = Some(Frac::zero());
    for (column, coefficient) in row_of(instance, row) {
        let (low, high) = effective(&instance.columns[column as usize]);
        let (towards_min, towards_max) = if coefficient.is_plus() {
            (low, high)
        } else {
            (high, low)
        };
        minimum = match (minimum, towards_min) {
            (Some(total), Some(bound)) => Some(total.plus(&coefficient.times(&bound))),
            _ => None,
        };
        maximum = match (maximum, towards_max) {
            (Some(total), Some(bound)) => Some(total.plus(&coefficient.times(&bound))),
            _ => None,
        };
    }
    (minimum, maximum)
}

/// The bound a row implies for a column, and which side it lands on.
#[must_use]
pub fn derived_bound(
    instance: &Instance,
    row: u32,
    column: u32,
    from_upper: bool,
) -> Option<(bool, Frac)> {
    let entries = row_of(instance, row);
    let coefficient = entries
        .iter()
        .find(|(at, _)| *at == column)
        .map(|(_, value)| value.clone())?;
    if coefficient.is_zero() {
        return None;
    }
    let side = if from_upper {
        instance.rows[row as usize].high.as_ref()?
    } else {
        instance.rows[row as usize].low.as_ref()?
    };

    let mut residual = Frac::zero();
    for (other, value) in &entries {
        if *other == column {
            continue;
        }
        let (low, high) = effective(&instance.columns[*other as usize]);
        let towards = if value.is_plus() == from_upper {
            low
        } else {
            high
        };
        residual = residual.plus(&value.times(&towards?));
    }

    let free = side.minus(&residual).divided(&coefficient)?;
    let lands_upper = if coefficient.is_plus() {
        from_upper
    } else {
        !from_upper
    };
    let rounded = if instance.columns[column as usize].whole {
        if lands_upper {
            Frac::whole(free.floor())
        } else {
            Frac::whole(free.ceiling())
        }
    } else {
        free
    };
    Some((lands_upper, rounded))
}

/// The value the empty-column rule chooses, when one exists.
#[must_use]
pub fn empty_value(column: &Column) -> Option<Frac> {
    let (low, high) = effective(column);
    let zero = Frac::zero();
    let inside = low
        .as_ref()
        .is_none_or(|bound| bound.compare(&zero) != core::cmp::Ordering::Greater)
        && high
            .as_ref()
            .is_none_or(|bound| bound.compare(&zero) != core::cmp::Ordering::Less);

    if column.cost.is_zero() {
        if inside {
            return Some(zero);
        }
        return low.or(high);
    }
    if column.cost.is_plus() { low } else { high }
}

/// Applies a reduction, producing the next model.
///
/// # Errors
///
/// Returns [`Rejected`] when the rule does not apply to this model, or when the
/// derivation closes the domain to nothing.
pub fn apply(instance: &Instance, reduction: &Reduction) -> Result<Instance, Rejected> {
    let mut next = instance.clone();
    // A derived model describes what it contains, not how a file was read.
    next.cancelled = Vec::new();
    next.dropped = Vec::new();

    match reduction {
        Reduction::Fixed { column, stated } => fixed(&mut next, *column, stated)?,
        Reduction::Empty { column, stated } => empty(&mut next, *column, stated)?,
        Reduction::Singleton { row, column } => singleton(&mut next, *row, *column)?,
        Reduction::FreeColumn { row, column } => free_column(&mut next, *row, *column)?,
        Reduction::Redundant { row } => redundant(&mut next, *row)?,
        Reduction::Forcing { row, upper } => forcing(&mut next, *row, *upper)?,
        Reduction::Tighten {
            row,
            column,
            upper,
            stated,
        } => tighten(&mut next, *row, *column, *upper, stated)?,
        Reduction::Aggregate { keep, drop } => aggregate(&mut next, *keep, *drop)?,
    }
    Ok(next)
}

/// Removes a column and renumbers the cells after it.
fn drop_column(instance: &mut Instance, column: u32) {
    instance.columns.remove(column as usize);
    instance.cells.retain(|cell| cell.column != column);
    for cell in &mut instance.cells {
        if cell.column > column {
            cell.column -= 1;
        }
    }
    reorder(instance);
}

/// Removes a row and renumbers the cells after it.
fn drop_row(instance: &mut Instance, row: u32) {
    instance.rows.remove(row as usize);
    instance.cells.retain(|cell| cell.row != row);
    for cell in &mut instance.cells {
        if cell.row > row {
            cell.row -= 1;
        }
    }
    reorder(instance);
}

/// Puts the cells back into canonical order and drops any that vanished.
fn reorder(instance: &mut Instance) {
    instance.cells.retain(|cell| !cell.value.is_zero());
    instance.cells.sort_by_key(|cell| (cell.column, cell.row));
}

/// T1.
fn fixed(instance: &mut Instance, column: usize, stated: &Frac) -> Result<(), Rejected> {
    let holder = instance
        .columns
        .get(column)
        .ok_or_else(|| refuse(format!("column {column} does not exist")))?;
    let (low, high) = effective(holder);
    let (Some(low), Some(high)) = (low, high) else {
        return Err(refuse(format!(
            "column {column} is not bounded on both sides"
        )));
    };
    if low != high {
        return Err(refuse(format!("column {column} is not fixed")));
    }
    if low != *stated {
        return Err(refuse(format!(
            "column {column} is fixed at {low} and the record says {stated}"
        )));
    }
    if holder.whole && !low.is_whole() {
        return Err(refuse(format!(
            "column {column} is integer and fixed at {low}"
        )));
    }

    let cost = holder.cost.clone();
    instance.constant = instance.constant.plus(&cost.times(&low));
    for (row, coefficient) in column_of(instance, column as u32) {
        let shift = coefficient.times(&low);
        let side = &mut instance.rows[row as usize];
        if let Some(bound) = &side.low {
            side.low = Some(bound.minus(&shift));
        }
        if let Some(bound) = &side.high {
            side.high = Some(bound.minus(&shift));
        }
    }
    drop_column(instance, column as u32);
    Ok(())
}

/// T2.
fn empty(instance: &mut Instance, column: usize, stated: &Frac) -> Result<(), Rejected> {
    let holder = instance
        .columns
        .get(column)
        .ok_or_else(|| refuse(format!("column {column} does not exist")))?;
    if !column_of(instance, column as u32).is_empty() {
        return Err(refuse(format!("column {column} appears in a row")));
    }
    let chosen = empty_value(holder)
        .ok_or_else(|| refuse(format!("column {column} has no finite improving endpoint")))?;
    if chosen != *stated {
        return Err(refuse(format!(
            "the endpoint for column {column} is {chosen} and the record says {stated}"
        )));
    }
    let cost = holder.cost.clone();
    instance.constant = instance.constant.plus(&cost.times(&chosen));
    drop_column(instance, column as u32);
    Ok(())
}

/// T3.
fn singleton(instance: &mut Instance, row: usize, column: usize) -> Result<(), Rejected> {
    if row >= instance.rows.len() || column >= instance.columns.len() {
        return Err(refuse("an index does not exist"));
    }
    let entries = row_of(instance, row as u32);
    if entries.len() != 1 || entries[0].0 != column as u32 {
        return Err(refuse(format!("row {row} is not a singleton on {column}")));
    }
    let coefficient = entries[0].1.clone();
    if coefficient.is_zero() {
        return Err(refuse(format!("row {row} has a zero coefficient")));
    }

    let whole = instance.columns[column].whole;
    let (low_side, high_side) = (
        instance.rows[row].low.clone(),
        instance.rows[row].high.clone(),
    );
    let divide = |value: &Frac| value.divided(&coefficient).expect("a nonzero coefficient");
    let (mut derived_low, mut derived_high) = if coefficient.is_plus() {
        (
            low_side.as_ref().map(divide),
            high_side.as_ref().map(divide),
        )
    } else {
        (
            high_side.as_ref().map(divide),
            low_side.as_ref().map(divide),
        )
    };
    if whole {
        derived_low = derived_low.map(|value| Frac::whole(value.ceiling()));
        derived_high = derived_high.map(|value| Frac::whole(value.floor()));
    }

    let holder = &mut instance.columns[column];
    if let Some(value) = derived_low {
        holder.low = Some(match &holder.low {
            Some(current) if current.compare(&value) != core::cmp::Ordering::Less => {
                current.clone()
            }
            _ => value,
        });
    }
    if let Some(value) = derived_high {
        holder.high = Some(match &holder.high {
            Some(current) if current.compare(&value) != core::cmp::Ordering::Greater => {
                current.clone()
            }
            _ => value,
        });
    }
    if let (Some(low), Some(high)) = (&holder.low, &holder.high) {
        if low.compare(high) == core::cmp::Ordering::Greater {
            return Err(Rejected::new(
                Code::TRANSFORM_CONTRADICTION,
                format!("column {column} has an empty domain after row {row}"),
            ));
        }
    }
    drop_row(instance, row as u32);
    Ok(())
}

/// T4.
fn free_column(instance: &mut Instance, row: usize, column: usize) -> Result<(), Rejected> {
    let holder = instance
        .columns
        .get(column)
        .ok_or_else(|| refuse("a column does not exist"))?;
    if holder.whole || holder.low.is_some() || holder.high.is_some() {
        return Err(refuse(format!(
            "column {column} is not free and continuous"
        )));
    }
    let appearances = column_of(instance, column as u32);
    if appearances.len() != 1 || appearances[0].0 != row as u32 {
        return Err(refuse(format!(
            "column {column} does not appear in exactly row {row}"
        )));
    }
    let coefficient = appearances[0].1.clone();
    if coefficient.is_zero() {
        return Err(refuse("the coefficient is zero"));
    }
    let side = instance
        .rows
        .get(row)
        .ok_or_else(|| refuse("a row does not exist"))?;
    let (Some(low), Some(high)) = (side.low.clone(), side.high.clone()) else {
        return Err(refuse(format!("row {row} is not an equality")));
    };
    if low != high {
        return Err(refuse(format!("row {row} is not an equality")));
    }

    let cost = holder.cost.clone();
    let scale = cost.divided(&coefficient).expect("a nonzero coefficient");
    instance.constant = instance.constant.plus(&scale.times(&low));
    for (other, value) in row_of(instance, row as u32) {
        if other as usize == column {
            continue;
        }
        let adjust = scale.times(&value);
        let slot = &mut instance.columns[other as usize];
        slot.cost = slot.cost.minus(&adjust);
    }
    drop_row(instance, row as u32);
    drop_column(instance, column as u32);
    Ok(())
}

/// T5.
fn redundant(instance: &mut Instance, row: usize) -> Result<(), Rejected> {
    let side = instance
        .rows
        .get(row)
        .ok_or_else(|| refuse("a row does not exist"))?
        .clone();
    let (minimum, maximum) = activity_range(instance, row as u32);
    if let Some(bound) = &side.low {
        let Some(minimum) = &minimum else {
            return Err(refuse(format!("row {row} has an unbounded minimum")));
        };
        if minimum.compare(bound) == core::cmp::Ordering::Less {
            return Err(refuse(format!(
                "row {row} is not implied on its lower side"
            )));
        }
    }
    if let Some(bound) = &side.high {
        let Some(maximum) = &maximum else {
            return Err(refuse(format!("row {row} has an unbounded maximum")));
        };
        if maximum.compare(bound) == core::cmp::Ordering::Greater {
            return Err(refuse(format!(
                "row {row} is not implied on its upper side"
            )));
        }
    }
    drop_row(instance, row as u32);
    Ok(())
}

/// T6.
fn forcing(instance: &mut Instance, row: usize, upper: bool) -> Result<(), Rejected> {
    let side = instance
        .rows
        .get(row)
        .ok_or_else(|| refuse("a row does not exist"))?
        .clone();
    let (minimum, maximum) = activity_range(instance, row as u32);
    let (bound, extreme) = if upper {
        (side.high.as_ref(), minimum.as_ref())
    } else {
        (side.low.as_ref(), maximum.as_ref())
    };
    let (Some(bound), Some(extreme)) = (bound, extreme) else {
        return Err(refuse(format!("row {row} has no finite extreme activity")));
    };
    if bound != extreme {
        return Err(refuse(format!(
            "row {row} sits at {bound} and its extreme activity is {extreme}"
        )));
    }
    let entries = row_of(instance, row as u32);
    if entries.is_empty() {
        return Err(refuse(format!("row {row} is empty")));
    }
    for (column, coefficient) in entries {
        let (low, high) = effective(&instance.columns[column as usize]);
        let towards_high = coefficient.is_plus() != upper;
        let endpoint = if towards_high { high } else { low };
        let Some(endpoint) = endpoint else {
            return Err(refuse(format!("column {column} has no finite endpoint")));
        };
        let holder = &mut instance.columns[column as usize];
        holder.low = Some(match &holder.low {
            Some(current) if current.compare(&endpoint) != core::cmp::Ordering::Less => {
                current.clone()
            }
            _ => endpoint.clone(),
        });
        holder.high = Some(match &holder.high {
            Some(current) if current.compare(&endpoint) != core::cmp::Ordering::Greater => {
                current.clone()
            }
            _ => endpoint,
        });
    }
    Ok(())
}

/// T7.
fn tighten(
    instance: &mut Instance,
    row: usize,
    column: usize,
    upper: bool,
    stated: &Frac,
) -> Result<(), Rejected> {
    if row >= instance.rows.len() || column >= instance.columns.len() {
        return Err(refuse("an index does not exist"));
    }
    // The verifier chooses the contributors itself. A record that chose them
    // would be choosing which residual it was checked against.
    let derived = [true, false]
        .into_iter()
        .filter_map(|from_upper| derived_bound(instance, row as u32, column as u32, from_upper))
        .find(|(lands, _)| *lands == upper)
        .ok_or_else(|| {
            refuse(format!(
                "row {row} implies no such bound for column {column}"
            ))
        })?
        .1;
    if derived != *stated {
        return Err(refuse(format!(
            "row {row} implies {derived} for column {column} and the record says {stated}"
        )));
    }

    let holder = &mut instance.columns[column];
    let improves = if upper {
        holder
            .high
            .as_ref()
            .is_none_or(|current| derived.compare(current) == core::cmp::Ordering::Less)
    } else {
        holder
            .low
            .as_ref()
            .is_none_or(|current| derived.compare(current) == core::cmp::Ordering::Greater)
    };
    if !improves {
        return Err(refuse(format!(
            "the bound {derived} does not tighten column {column}"
        )));
    }
    if upper {
        holder.high = Some(derived);
    } else {
        holder.low = Some(derived);
    }
    if let (Some(low), Some(high)) = (&holder.low, &holder.high) {
        if low.compare(high) == core::cmp::Ordering::Greater {
            return Err(Rejected::new(
                Code::TRANSFORM_CONTRADICTION,
                format!("column {column} has an empty domain after row {row}"),
            ));
        }
    }
    Ok(())
}

/// T8.
fn aggregate(instance: &mut Instance, keep: usize, drop: usize) -> Result<(), Rejected> {
    if keep == drop || keep >= instance.columns.len() || drop >= instance.columns.len() {
        return Err(refuse("the two columns are the same or do not exist"));
    }
    let left = instance.columns[keep].clone();
    let right = instance.columns[drop].clone();
    if left.cost != right.cost || left.whole != right.whole {
        return Err(refuse("the two columns differ in type or objective"));
    }
    let left_cells: Vec<(u32, Frac)> = column_of(instance, keep as u32);
    let right_cells: Vec<(u32, Frac)> = column_of(instance, drop as u32);
    if left_cells != right_cells {
        return Err(refuse("the two columns are not identical"));
    }
    let (Some(low_left), Some(high_left)) = (left.low, left.high) else {
        return Err(refuse("a bound is infinite"));
    };
    let (Some(low_right), Some(high_right)) = (right.low, right.high) else {
        return Err(refuse("a bound is infinite"));
    };

    instance.columns[keep].low = Some(low_left.plus(&low_right));
    instance.columns[keep].high = Some(high_left.plus(&high_right));
    drop_column(instance, drop as u32);
    Ok(())
}

/// Recomputes a derivation that is claimed to close a domain to nothing.
///
/// A contradiction record carries the rule and the indices its derivation is
/// about, and nothing else. There is no stated value because there is nothing
/// to state: the derivation either closes the domain on the model the verifier
/// holds or it does not, and a record asserting that presolve found the model
/// infeasible is not a witness.
///
/// # Errors
///
/// Returns [`Rejected`] when the rule is not one whose derivation can close a
/// domain, or when the derivation does not close it here.
pub fn contradicts(instance: &Instance, rule: u64, sources: &[u64]) -> Result<(), Rejected> {
    let index = |at: usize| -> Result<usize, Rejected> {
        sources
            .get(at)
            .and_then(|value| usize::try_from(*value).ok())
            .ok_or_else(|| {
                Rejected::new(
                    Code::TRANSFORM_CONTRADICTION,
                    format!("rule {rule} is missing source {at}"),
                )
            })
    };

    let (low, high) = match rule {
        // A singleton row: divide it out and intersect with the column's own
        // bounds.
        3 => {
            if sources.len() != 2 {
                return Err(Rejected::new(
                    Code::TRANSFORM_CONTRADICTION,
                    "a singleton-row contradiction names a row and a column",
                ));
            }
            let (row, column) = (index(0)?, index(1)?);
            let mut copy = instance.clone();
            match singleton(&mut copy, row, column) {
                Err(refusal) if refusal.code == Code::TRANSFORM_CONTRADICTION => {
                    return Ok(());
                }
                Err(refusal) => return Err(refusal),
                Ok(()) => {
                    return Err(Rejected::new(
                        Code::TRANSFORM_CONTRADICTION,
                        format!("row {row} does not close the domain of column {column}"),
                    ));
                }
            }
        }
        // A derived bound: recompute it and intersect.
        7 => {
            if sources.len() != 3 {
                return Err(Rejected::new(
                    Code::TRANSFORM_CONTRADICTION,
                    "a bound contradiction names a row, a column, and a side",
                ));
            }
            let (row, column, upper) = (index(0)?, index(1)?, index(2)? == 1);
            let derived = [true, false]
                .into_iter()
                .filter_map(|from_upper| {
                    derived_bound(instance, row as u32, column as u32, from_upper)
                })
                .find(|(lands, _)| *lands == upper)
                .ok_or_else(|| {
                    Rejected::new(
                        Code::TRANSFORM_CONTRADICTION,
                        format!("row {row} implies no such bound for column {column}"),
                    )
                })?
                .1;
            let holder = instance.columns.get(column).ok_or_else(|| {
                Rejected::new(
                    Code::TRANSFORM_CONTRADICTION,
                    format!("column {column} does not exist"),
                )
            })?;
            if upper {
                (holder.low.clone(), Some(derived))
            } else {
                (Some(derived), holder.high.clone())
            }
        }
        other => {
            return Err(Rejected::new(
                Code::TRANSFORM_CONTRADICTION,
                format!("rule {other} has no derivation that can close a domain"),
            ));
        }
    };

    match (low, high) {
        (Some(low), Some(high)) if low.compare(&high) == core::cmp::Ordering::Greater => Ok(()),
        _ => Err(Rejected::new(
            Code::TRANSFORM_CONTRADICTION,
            format!("the derivation of rule {rule} leaves a nonempty domain"),
        )),
    }
}

/// Maps a point of the model before a reduction to one of the model after.
///
/// The verifier needs only this direction. It holds both witnesses — the
/// original-space point and the reduced one — and checking that the first maps
/// onto the second is a stronger statement than lifting the second and hoping
/// it lands on the first: a lift has a deterministic answer the certificate
/// could be built around, while a forward map has to agree with a point the
/// certificate already committed to.
#[must_use]
pub fn forward(before: &Instance, reduction: &Reduction, point: &[Frac]) -> Vec<Frac> {
    let mut out = point.to_vec();
    match reduction {
        Reduction::Fixed { column, stated } | Reduction::Empty { column, stated } => {
            if *column < out.len() {
                out[*column] = stated.clone();
                out.remove(*column);
            }
        }
        Reduction::FreeColumn { column, .. } => {
            if *column < out.len() {
                out.remove(*column);
            }
        }
        Reduction::Aggregate { keep, drop } => {
            if *keep < out.len() && *drop < out.len() {
                out[*keep] = out[*keep].plus(&out[*drop]);
                out.remove(*drop);
            }
        }
        Reduction::Singleton { .. }
        | Reduction::Redundant { .. }
        | Reduction::Forcing { .. }
        | Reduction::Tighten { .. } => {}
    }
    let _ = before;
    out
}

/// Rebuilds a cell list from scratch, for callers assembling an instance.
#[must_use]
pub fn cells_from(entries: &[(u32, u32, Frac)]) -> Vec<Cell> {
    let mut out: Vec<Cell> = entries
        .iter()
        .filter(|(_, _, value)| !value.is_zero())
        .map(|(column, row, value)| Cell {
            column: *column,
            row: *row,
            value: value.clone(),
        })
        .collect();
    out.sort_by_key(|cell| (cell.column, cell.row));
    out
}

/// A row with the sides a caller states, for assembling test instances.
#[must_use]
pub fn constraint(low: Option<Frac>, high: Option<Frac>, name: &str) -> Constraint {
    Constraint {
        low,
        high,
        name: name.to_owned(),
    }
}
