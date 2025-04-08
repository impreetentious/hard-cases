//! Rounding, with the row repair that makes it worth trying.
//!
//! The relaxation's point has some integer variables sitting at fractions.
//! Moving each to the nearest integer gives a point that is integral by
//! construction and feasible by luck; the repair pass improves the luck by
//! recomputing every row's activity from the rounded values rather than
//! leaving the old ones in place.
//!
//! That recomputation is exact and it is not a fix-up: in the working shape a
//! row's activity is a variable whose value is determined by the others, so
//! after moving the structural values the activity values are simply wrong
//! until they are recomputed. What remains after that is a genuine question —
//! whether the rounded point satisfies the rows and the bounds — and the
//! screening answers it.

use hc_exact::Rational;
use hc_lp::exact::Canonical;

use crate::Outcome;

/// Rounds a point and repairs the row activities.
///
/// Returns the candidate, which has been made integral and consistent and has
/// been established as nothing.
#[must_use]
pub fn candidate(canonical: &Canonical, point: &[Rational]) -> Vec<Rational> {
    let mut out = point.to_vec();
    for column in 0..canonical.structural().min(out.len()) {
        if !canonical.is_integral(column) || out[column].is_integer() {
            continue;
        }
        out[column] = nearest(&out[column]);
    }
    repair(canonical, &mut out);
    out
}

/// The nearest integer, with a tie going upward.
///
/// The direction of the tie-break is arbitrary and fixed, which is all it needs
/// to be: it decides which of two candidates is offered and nothing about
/// whether either is accepted.
#[must_use]
pub fn nearest(value: &Rational) -> Rational {
    let floor = Rational::from_int(value.floor());
    let fraction = value.sub(&floor);
    let half = Rational::from_i64(2).recip().expect("two is not zero");
    if fraction.cmp_value(&half).is_lt() {
        floor
    } else {
        floor.add(&Rational::one())
    }
}

/// Recomputes every row activity from the structural values.
pub fn repair(canonical: &Canonical, point: &mut [Rational]) {
    let structural = canonical.structural();
    if point.len() < canonical.columns() {
        return;
    }
    let mut activities = vec![Rational::zero(); canonical.rows()];
    for (column, value) in point.iter().enumerate().take(structural) {
        if value.is_zero() {
            continue;
        }
        for (row, coefficient) in canonical.column(column) {
            let slot = &mut activities[*row as usize];
            *slot = slot.add(&coefficient.mul(value));
        }
    }
    for (row, activity) in activities.into_iter().enumerate() {
        point[structural + row] = activity;
    }
}

/// Rounds, repairs, and reports what happened.
///
/// `screen` is the caller's exact acceptance gate. It is a parameter rather
/// than something this crate reaches for, because the gate belongs to the
/// search and a heuristic that could accept its own candidates would be a
/// heuristic that decides what is true.
pub fn round(
    canonical: &Canonical,
    point: &[Rational],
    screen: &dyn Fn(&[Rational]) -> Result<Rational, String>,
) -> Outcome {
    if point.len() != canonical.columns() {
        return Outcome::Abandoned {
            reason: String::from("the relaxation's point is the wrong length"),
        };
    }
    let candidate = candidate(canonical, point);
    match screen(&candidate) {
        Ok(objective) => Outcome::Found {
            point: candidate,
            objective,
        },
        Err(reason) => Outcome::Refused { reason },
    }
}
