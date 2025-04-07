//! Finding cuts.
//!
//! Separation is allowed to be as heuristic as it likes. It looks at a
//! relaxation's solution, notices a variable sitting at a fraction, and tries
//! the derivation that would exclude that point. If the derivation refuses, or
//! the cut it produces does not exclude the point after all, the candidate is
//! dropped and nothing is recorded.
//!
//! What is not heuristic is the derivation itself, which is exact, and the
//! recomputation the certificate will be checked against, which is the same
//! function. A separation routine that guessed and a verifier that recomputed
//! would be two programs; here the routine derives what the verifier will
//! derive, and a cut that does not survive its own derivation never enters the
//! pool.

use hc_exact::Rational;
use hc_lp::Basis;
use hc_lp::exact::{Canonical, tableau_row};

use crate::cover::{is_cover, knapsack_from, lift};
use crate::gmi::{derive, over_variables};
use crate::pool::{Derivation, Entry, Family};

/// How much a cut must exclude the point before it is worth keeping.
///
/// A cut that does not cut is a valid inequality nobody needs, and recording
/// one would put a record in a certificate that supports nothing.
#[must_use]
pub fn violated(coefficients: &[(usize, Rational)], right: &Rational, point: &[Rational]) -> bool {
    let mut activity = Rational::zero();
    for (column, coefficient) in coefficients {
        let Some(value) = point.get(*column) else {
            return false;
        };
        activity = activity.add(&coefficient.mul(value));
    }
    activity.cmp_value(right).is_lt()
}

/// Gomory candidates from a basis, at one node.
///
/// One per basic integer variable sitting at a fraction: the tableau row that
/// isolates it, and the cut that row gives. Everything the certificate will
/// need is carried back, because the point of deriving here is that the same
/// derivation runs again on the other side.
#[must_use]
pub fn gomory(canonical: &Canonical, basis: &Basis, point: &[Rational], scope: u64) -> Vec<Entry> {
    let mut out = Vec::new();
    for (position, column) in basis.basic().iter().enumerate() {
        if *column >= canonical.structural() || !canonical.is_integral(*column) {
            continue;
        }
        if point[*column].is_integer() {
            continue;
        }
        let Ok(row) = tableau_row(canonical, basis, position) else {
            continue;
        };
        let combination: Vec<(usize, Rational)> = row
            .iter()
            .enumerate()
            .filter(|(_, value)| !value.is_zero())
            .map(|(at, value)| (at, value.clone()))
            .collect();
        if combination.is_empty() {
            continue;
        }
        let Ok(cut) = derive(canonical, &combination, *column) else {
            continue;
        };
        let expanded = over_variables(canonical, &cut);
        if !violated(&expanded.coefficients, &expanded.right, point) {
            continue;
        }
        out.push(Entry {
            scope,
            family: Family::Gomory,
            derivation: Derivation::Gomory {
                combination,
                isolated: *column,
            },
            coefficients: expanded.coefficients,
            right: expanded.right,
        });
    }
    out
}

/// Cover candidates from the knapsack rows of a model, at one node.
///
/// The cover is chosen greedily — heaviest items first until the capacity is
/// exceeded — and the greed decides nothing: the cover condition is checked
/// and every lifting coefficient is recomputed by the dynamic program.
#[must_use]
pub fn cover(canonical: &Canonical, point: &[Rational], scope: u64) -> Vec<Entry> {
    let mut out = Vec::new();
    for row in 0..canonical.rows() {
        let Ok(knapsack) = knapsack_from(canonical, row) else {
            continue;
        };
        // The variables the relaxation has most of, first. A cover made of
        // them is the one the current point is furthest from satisfying, which
        // is the cover worth deriving; a cover of variables the point has none
        // of is valid and useless.
        let mut ordered = knapsack.weights.clone();
        ordered.sort_by(|left, right| {
            let here = point.get(left.0).cloned().unwrap_or_else(Rational::zero);
            let there = point.get(right.0).cloned().unwrap_or_else(Rational::zero);
            there
                .cmp_value(&here)
                .then(right.1.cmp(&left.1))
                .then(left.0.cmp(&right.0))
        });

        let mut members: Vec<usize> = Vec::new();
        let mut total = 0_u64;
        for (column, weight) in &ordered {
            members.push(*column);
            total += weight;
            if total > knapsack.capacity {
                break;
            }
        }
        members.sort_unstable();
        if !is_cover(&knapsack, &members) {
            continue;
        }
        let order: Vec<usize> = knapsack
            .weights
            .iter()
            .map(|(column, _)| *column)
            .filter(|column| !members.contains(column))
            .collect();
        let Ok(cut) = lift(&knapsack, &members, &order) else {
            continue;
        };
        let (coefficients, right) = crate::cover::as_inequality(&cut);
        if !violated(&coefficients, &right, point) {
            continue;
        }
        out.push(Entry {
            scope,
            family: Family::Cover,
            derivation: Derivation::Cover {
                row,
                members,
                order,
            },
            coefficients,
            right,
        });
    }
    out
}
