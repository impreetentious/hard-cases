//! The feasibility pump.
//!
//! Two points, pulled towards each other. One is feasible for the relaxation
//! and fractional; the other is integral and probably infeasible. Round the
//! first to get the second, then re-solve the relaxation with an objective that
//! prefers points near the second, and repeat. When the two meet, the meeting
//! point is feasible and integral.
//!
//! It cycles. When the same integral point comes back, a few of its components
//! are flipped — the ones that were furthest from their rounded values — and
//! the walk continues from somewhere else. The flips are drawn from the search's
//! own seeded stream, so a pump that found a solution once finds it again.

use hc_core::Xoshiro256PlusPlus;
use hc_exact::Rational;
use hc_lp::exact::Canonical;

use crate::Outcome;
use crate::round::{candidate, nearest, repair};

/// How many flips a restart makes.
pub const RESTART_FLIPS: usize = 8;

/// Runs the pump.
///
/// `resolve` is asked for the relaxation's point nearest a given integral
/// target, and returns `None` when it cannot answer. `screen` is the caller's
/// exact acceptance gate.
pub fn pump(
    canonical: &Canonical,
    point: &[Rational],
    budget: usize,
    seed: u64,
    resolve: &mut dyn FnMut(&[Rational]) -> Option<Vec<Rational>>,
    screen: &dyn Fn(&[Rational]) -> Result<Rational, String>,
) -> Outcome {
    if point.len() != canonical.columns() {
        return Outcome::Abandoned {
            reason: String::from("the relaxation's point is the wrong length"),
        };
    }
    let mut generator = Xoshiro256PlusPlus::from_seed(seed);
    let mut relaxed = point.to_vec();
    let mut seen: Vec<Vec<Rational>> = Vec::new();

    for _ in 0..budget {
        let mut integral = candidate(canonical, &relaxed);
        if integral == relaxed {
            // The two met. Whether that is a solution is the gate's question.
            return match screen(&integral) {
                Ok(objective) => Outcome::Found {
                    point: integral,
                    objective,
                },
                Err(reason) => Outcome::Refused { reason },
            };
        }

        if seen.contains(&integral) {
            // A cycle. Flip the components the relaxation was least sure about
            // and carry on from there.
            let mut candidates: Vec<(usize, Rational)> = (0..canonical.structural())
                .filter(|column| canonical.is_integral(*column))
                .map(|column| {
                    let gap = relaxed[column].sub(&integral[column]);
                    (column, gap.abs())
                })
                .collect();
            candidates.sort_by(|left, right| right.1.cmp_value(&left.1).then(left.0.cmp(&right.0)));
            for (column, _) in candidates.into_iter().take(RESTART_FLIPS) {
                let step = if generator.next_below(2) == 0 {
                    Rational::one()
                } else {
                    Rational::from_i64(-1)
                };
                integral[column] = integral[column].add(&step);
            }
            repair(canonical, &mut integral);
        }
        seen.push(integral.clone());

        let Some(next) = resolve(&integral) else {
            return Outcome::Abandoned {
                reason: String::from("the pump's relaxation stopped answering"),
            };
        };
        relaxed = next;
    }

    Outcome::Abandoned {
        reason: format!("the pump spent its budget of {budget} rounds"),
    }
}

/// The integral point nearest a relaxation's point, for a caller building the
/// pump's objective.
#[must_use]
pub fn target(canonical: &Canonical, point: &[Rational]) -> Vec<Rational> {
    let mut out = point.to_vec();
    for column in 0..canonical.structural().min(out.len()) {
        if canonical.is_integral(column) {
            out[column] = nearest(&out[column]);
        }
    }
    out
}
