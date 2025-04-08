//! Diving: following the relaxation down, one variable at a time.
//!
//! Fix the most fractional integer variable to the integer it is nearest,
//! re-solve, and repeat. Each step is a smaller problem than the last and the
//! dive ends when the relaxation comes out integral, becomes infeasible, or the
//! budget runs out.
//!
//! It is a search inside a search, and it is capped because of it. A dive that
//! ran to the bottom of every branch would be depth-first search with a
//! different name and the same cost.

use hc_exact::Rational;
use hc_lp::exact::Canonical;

use crate::Outcome;
use crate::round::{nearest, repair};

/// One step of a dive: a column and the value it is fixed to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    /// The column.
    pub column: usize,
    /// The value it takes.
    pub value: Rational,
}

/// The next variable to fix, and where.
///
/// The most fractional one, which is the variable the relaxation is least
/// decided about, with the lowest index breaking ties.
#[must_use]
pub fn next_fix(canonical: &Canonical, point: &[Rational]) -> Option<Fix> {
    let half = Rational::from_i64(2).recip().expect("two is not zero");
    let mut best: Option<(usize, Rational)> = None;
    for (column, value) in point
        .iter()
        .enumerate()
        .take(canonical.structural().min(point.len()))
    {
        if !canonical.is_integral(column) || value.is_integer() {
            continue;
        }
        let floor = Rational::from_int(value.floor());
        let fraction = value.sub(&floor);
        let distance = if fraction.cmp_value(&half).is_gt() {
            Rational::one().sub(&fraction)
        } else {
            fraction
        };
        if best
            .as_ref()
            .is_none_or(|(_, seen)| distance.cmp_value(seen).is_gt())
        {
            best = Some((column, distance));
        }
    }
    best.map(|(column, _)| Fix {
        column,
        value: nearest(&point[column]),
    })
}

/// Dives from a relaxation's point.
///
/// `resolve` is asked for the relaxation's point under a set of fixings, and
/// returns `None` when there is none — an infeasible dive, which is an ordinary
/// outcome and ends it. `screen` is the caller's exact acceptance gate.
///
/// `budget` bounds the number of re-solves.
pub fn dive(
    canonical: &Canonical,
    point: &[Rational],
    budget: usize,
    resolve: &mut dyn FnMut(&[Fix]) -> Option<Vec<Rational>>,
    screen: &dyn Fn(&[Rational]) -> Result<Rational, String>,
) -> Outcome {
    let mut fixings: Vec<Fix> = Vec::new();
    let mut current = point.to_vec();

    for _ in 0..budget {
        let Some(fix) = next_fix(canonical, &current) else {
            // The relaxation came out integral. Repair the activities and let
            // the gate decide.
            let mut candidate = current;
            repair(canonical, &mut candidate);
            return match screen(&candidate) {
                Ok(objective) => Outcome::Found {
                    point: candidate,
                    objective,
                },
                Err(reason) => Outcome::Refused { reason },
            };
        };
        fixings.push(fix);
        let Some(next) = resolve(&fixings) else {
            return Outcome::Abandoned {
                reason: format!("the dive became infeasible after {} fixings", fixings.len()),
            };
        };
        current = next;
    }

    Outcome::Abandoned {
        reason: format!("the dive spent its budget of {budget} re-solves"),
    }
}
