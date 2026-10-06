//! The two maps that make a reduction a proof step.
//!
//! Forward: a feasible point of the model before a transform becomes a
//! feasible point of the model after it, with objective no greater. Backward:
//! a feasible point of the model after lifts to a feasible point of the model
//! before, with objective exactly equal.
//!
//! Both are computed from the model the transform was applied to, because that
//! is the model that holds the information a lift needs — the value a column
//! was fixed at, the row a substitution came from, the bounds a split is
//! clamped into. The certificate carries no map data at all, so there is
//! nothing in it a forger could edit here.
//!
//! A discrepancy between the two is a defect in the transform, never something
//! to compensate for in the lift. Adjusting a reverse map to cancel a forward
//! map that was already wrong hides the defect and leaves the invariant broken,
//! which is the failure mode this whole arrangement is arranged against.

use hc_core::Model;
use hc_exact::Rational;

use crate::draft::Draft;
use crate::rule::Rule;

/// Maps a point of the model before a transform to one of the model after.
///
/// `before` is the model the transform was applied to.
#[must_use]
pub fn forward(before: &Model, rule: &Rule, point: &[Rational]) -> Vec<Rational> {
    let mut out = point.to_vec();
    match rule {
        Rule::FixedColumn { column, value } | Rule::EmptyColumn { column, value } => {
            // The applicability condition fixes the column to this value, so
            // dropping the component loses nothing — except in T2, where the
            // point may have held something else and this replaces it with a
            // value that cannot be worse.
            if *column < out.len() {
                out[*column] = value.clone();
                out.remove(*column);
            }
        }
        Rule::FreeSingletonColumn { column, .. } => {
            if *column < out.len() {
                out.remove(*column);
            }
        }
        Rule::DuplicateColumn { keep, drop } => {
            if *keep < out.len() && *drop < out.len() {
                out[*keep] = out[*keep].add(&out[*drop]);
                out.remove(*drop);
            }
        }
        // The rest change rows or bounds and leave the variables alone.
        Rule::SingletonRow { .. }
        | Rule::RedundantRow { .. }
        | Rule::ForcingRow { .. }
        | Rule::TightenBound { .. } => {}
    }
    let _ = before;
    out
}

/// Lifts a point of the model after a transform to one of the model before.
///
/// `before` is the model the transform was applied to, which is where the
/// information a lift needs lives.
#[must_use]
pub fn reverse(before: &Model, rule: &Rule, point: &[Rational]) -> Vec<Rational> {
    let mut out = point.to_vec();
    match rule {
        Rule::FixedColumn { column, value } | Rule::EmptyColumn { column, value } => {
            let at = (*column).min(out.len());
            out.insert(at, value.clone());
        }
        Rule::FreeSingletonColumn { row, column } => {
            // Evaluate the substitution at the lifted point. Every other term
            // of the row is already in place because the lift walks the chain
            // backwards, so the columns to the left of this one are the ones
            // the model before has.
            let draft = Draft::of(before);
            let entries = draft.row_entries(*row);
            let coefficient = entries
                .iter()
                .find(|(at, _)| at == column)
                .map(|(_, value)| value.clone())
                .unwrap_or_else(Rational::one);
            let right = draft.rows[*row]
                .lower
                .clone()
                .or_else(|| draft.rows[*row].upper.clone())
                .unwrap_or_else(Rational::zero);

            // Insert a placeholder so the indices line up with the model
            // before, then compute the value in those coordinates.
            let at = (*column).min(out.len());
            out.insert(at, Rational::zero());
            let mut total = right;
            for (other, value) in &entries {
                if other == column {
                    continue;
                }
                if let Some(held) = out.get(*other) {
                    total = total.sub(&value.mul(held));
                }
            }
            let recovered = coefficient
                .recip()
                .map_or_else(Rational::zero, |inverse| total.mul(&inverse));
            out[at] = recovered;
        }
        Rule::DuplicateColumn { keep, drop } => {
            // The deterministic split: the retained column takes as much of
            // the sum as its own bounds allow after leaving the dropped one
            // its minimum, and the dropped column takes the rest. Every
            // quantity is integral when the columns are, so an integer point
            // lifts to an integer point by construction.
            let draft = Draft::of(before);
            let left = &draft.variables[*keep];
            let right = &draft.variables[*drop];
            let sum = out.get(*keep).cloned().unwrap_or_else(Rational::zero);

            let target = sum.sub(right.lower.as_ref().unwrap_or(&Rational::zero()));
            let clamped = clamp(&target, left.lower.as_ref(), left.upper.as_ref());
            let other = sum.sub(&clamped);
            if *keep < out.len() {
                out[*keep] = clamped;
            }
            let at = (*drop).min(out.len());
            out.insert(at, other);
        }
        Rule::SingletonRow { .. }
        | Rule::RedundantRow { .. }
        | Rule::ForcingRow { .. }
        | Rule::TightenBound { .. } => {}
    }
    out
}

/// A value pulled into an interval.
fn clamp(value: &Rational, low: Option<&Rational>, high: Option<&Rational>) -> Rational {
    let mut out = value.clone();
    if let Some(bound) = low
        && out.cmp_value(bound).is_lt()
    {
        out = bound.clone();
    }
    if let Some(bound) = high
        && out.cmp_value(bound).is_gt()
    {
        out = bound.clone();
    }
    out
}
