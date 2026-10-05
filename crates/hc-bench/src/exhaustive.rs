//! The oracle that cannot be clever.
//!
//! For an integer program small enough, the answer can be found by looking at
//! every point. No basis, no bound, no branching — a loop over the product of
//! the integer boxes, evaluating each assignment exactly against the model's
//! own rationals and keeping the best feasible one.
//!
//! That is worth having precisely because it shares nothing with the search
//! except the model. It has no notion of a relaxation, so it cannot repeat a
//! mistake about relaxations; it has no notion of pruning, so it cannot repeat
//! a mistake about pruning. When it and the search disagree, one of them is
//! wrong, and finding out which is a day well spent.
//!
//! Its cost is exponential and the cap is enforced before the walk begins. An
//! instance too large is reported as too large rather than attempted, because
//! a harness that silently skipped would be a harness that stops testing as
//! soon as the instances get interesting.

use hc_core::Model;
use hc_exact::Rational;

/// What looking at every point established.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Truth {
    /// The best feasible assignment, and its exact objective.
    Optimal {
        /// The assignment, over the model's variables.
        point: Vec<Rational>,
        /// Its exact objective, the constant included.
        value: Rational,
    },
    /// No assignment in the box is feasible.
    Infeasible,
    /// The model is outside what enumeration can answer.
    ///
    /// Either the product of the integer boxes exceeds the cap, or some
    /// variable is continuous or unbounded, which enumeration cannot reach.
    Beyond {
        /// Why.
        reason: &'static str,
    },
}

/// Enumerates every integer assignment and reports the best feasible one.
///
/// `cap` bounds the product of the box sizes. It is checked before anything is
/// allocated or walked.
#[must_use]
pub fn best(model: &Model, cap: u64) -> Truth {
    let variables = model.variables();
    let mut boxes: Vec<(i64, i64)> = Vec::with_capacity(variables.len());
    let mut product = 1_u64;

    for variable in variables {
        if !variable.integral {
            return Truth::Beyond {
                reason: "a variable is continuous",
            };
        }
        let (Some(low), Some(high)) = (variable.lower.as_ref(), variable.upper.as_ref()) else {
            return Truth::Beyond {
                reason: "an integer variable is unbounded",
            };
        };
        // The effective integer bounds. A fractional bound narrows to the
        // integers inside it, which is what the model means by an integer
        // variable with a fractional bound.
        let first = ceiling(low);
        let last = floor(high);
        let (Some(first), Some(last)) = (first, last) else {
            return Truth::Beyond {
                reason: "a bound does not fit in a machine word",
            };
        };
        if first > last {
            return Truth::Infeasible;
        }
        let width = (last - first + 1) as u64;
        product = match product.checked_mul(width) {
            Some(value) if value <= cap => value,
            _ => {
                return Truth::Beyond {
                    reason: "the box product exceeds the cap",
                };
            }
        };
        boxes.push((first, last));
    }

    let mut point: Vec<i64> = boxes.iter().map(|(first, _)| *first).collect();
    let mut best: Option<(Vec<Rational>, Rational)> = None;

    loop {
        let exact: Vec<Rational> = point
            .iter()
            .map(|value| Rational::from_i64(*value))
            .collect();
        if feasible(model, &exact) {
            let value = model.objective_at(&exact);
            let better = match &best {
                None => true,
                Some((_, seen)) => value.cmp_value(seen).is_lt(),
            };
            if better {
                best = Some((exact, value));
            }
        }
        if !advance(&mut point, &boxes) {
            break;
        }
    }

    match best {
        None => Truth::Infeasible,
        Some((point, value)) => Truth::Optimal { point, value },
    }
}

/// Visits every feasible integer assignment in the model's box.
///
/// The same walk [`best`] makes, reporting each feasible point instead of
/// keeping only the best one. A cut is a claim that some region holds no
/// feasible integer point it excludes, and the only way to test that claim
/// without believing anything the solver derived is to look at all of them.
///
/// `cap` bounds the product of the box sizes and is checked before the walk
/// begins. Returns the reason when the model is outside what enumeration can
/// reach, exactly as [`best`] does.
///
/// # Errors
///
/// Returns the reason the model cannot be enumerated.
pub fn for_each_feasible(
    model: &Model,
    cap: u64,
    mut visit: impl FnMut(&[Rational]),
) -> Result<(), &'static str> {
    let Some(boxes) = boxes_of(model, cap)? else {
        // An empty box holds no points, which is a complete and correct answer
        // to "visit every feasible one".
        return Ok(());
    };

    let mut point: Vec<i64> = boxes.iter().map(|(first, _)| *first).collect();
    loop {
        let exact: Vec<Rational> = point
            .iter()
            .map(|value| Rational::from_i64(*value))
            .collect();
        if feasible(model, &exact) {
            visit(&exact);
        }
        if !advance(&mut point, &boxes) {
            return Ok(());
        }
    }
}

/// The integer box each variable ranges over, or why there is none.
///
/// `Ok(None)` means some variable's box is empty, so the model has no integer
/// points at all.
fn boxes_of(model: &Model, cap: u64) -> Result<Option<Vec<(i64, i64)>>, &'static str> {
    let variables = model.variables();
    let mut boxes: Vec<(i64, i64)> = Vec::with_capacity(variables.len());
    let mut product = 1_u64;

    for variable in variables {
        if !variable.integral {
            return Err("a variable is continuous");
        }
        let (Some(low), Some(high)) = (variable.lower.as_ref(), variable.upper.as_ref()) else {
            return Err("an integer variable is unbounded");
        };
        let (Some(first), Some(last)) = (ceiling(low), floor(high)) else {
            return Err("a bound does not fit in a machine word");
        };
        if first > last {
            return Ok(None);
        }
        let width = (last - first + 1) as u64;
        product = match product.checked_mul(width) {
            Some(value) if value <= cap => value,
            _ => return Err("the box product exceeds the cap"),
        };
        boxes.push((first, last));
    }
    Ok(Some(boxes))
}

/// Whether an assignment satisfies every row of the model, exactly.
///
/// Bounds are not checked because the walk never leaves them.
fn feasible(model: &Model, point: &[Rational]) -> bool {
    for (index, row) in model.rows().iter().enumerate() {
        let activity = model.row_activity(hc_core::RowId(index as u32), point);
        if let Some(low) = &row.lower
            && activity.cmp_value(low).is_lt()
        {
            return false;
        }
        if let Some(high) = &row.upper
            && activity.cmp_value(high).is_gt()
        {
            return false;
        }
    }
    true
}

/// Steps the odometer, returning false when it has wrapped all the way round.
fn advance(point: &mut [i64], boxes: &[(i64, i64)]) -> bool {
    for (value, (first, last)) in point.iter_mut().zip(boxes) {
        if *value < *last {
            *value += 1;
            return true;
        }
        *value = *first;
    }
    false
}

/// The least integer at least as large as a rational, as a machine word.
fn ceiling(value: &Rational) -> Option<i64> {
    word(&value.ceil())
}

/// The greatest integer no larger than a rational, as a machine word.
fn floor(value: &Rational) -> Option<i64> {
    word(&value.floor())
}

/// A big integer as a machine word, when it fits.
fn word(value: &hc_exact::BigInt) -> Option<i64> {
    value.to_string().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_io::mps::{Form, read};

    fn model(source: &str) -> Model {
        read(source.as_bytes(), Form::Free).expect("the source parses")
    }

    const KNAPSACK: &str = concat!(
        "NAME K\n",
        "ROWS\n",
        " N obj\n",
        " L c1\n",
        "COLUMNS\n",
        " MARKER 'MARKER' 'INTORG'\n",
        " x obj -7 c1 5\n",
        " y obj -9 c1 7\n",
        " z obj -5 c1 4\n",
        " MARKER 'MARKER' 'INTEND'\n",
        "RHS\n",
        " rhs c1 13\n",
        "BOUNDS\n",
        " UI b x 3\n",
        " UI b y 3\n",
        " UI b z 3\n",
        "ENDATA\n",
    );

    #[test]
    fn the_oracle_finds_the_hand_worked_optimum() {
        // Capacity 13 against weights 5, 7 and 4 with values 7, 9 and 5. One x
        // and two z fill it exactly for 17, and nothing else reaches that.
        let Truth::Optimal { point, value } = best(&model(KNAPSACK), 1 << 12) else {
            panic!("the knapsack has an optimum");
        };
        assert_eq!(value.to_canonical_string(), "-17");
        assert_eq!(point[0].to_canonical_string(), "1");
        assert_eq!(point[1].to_canonical_string(), "0");
        assert_eq!(point[2].to_canonical_string(), "2");
    }

    #[test]
    fn an_empty_box_is_infeasible_before_anything_is_walked() {
        let source = concat!(
            "NAME E\n",
            "ROWS\n",
            " N obj\n",
            " G c1\n",
            "COLUMNS\n",
            " MARKER 'MARKER' 'INTORG'\n",
            " x obj 1 c1 1\n",
            " MARKER 'MARKER' 'INTEND'\n",
            "RHS\n",
            " rhs c1 5\n",
            "BOUNDS\n",
            " UI b x 2\n",
            "ENDATA\n",
        );
        assert_eq!(best(&model(source), 1 << 12), Truth::Infeasible);
    }

    #[test]
    fn a_box_larger_than_the_cap_is_reported_rather_than_attempted() {
        assert!(matches!(
            best(&model(KNAPSACK), 4),
            Truth::Beyond {
                reason: "the box product exceeds the cap"
            }
        ));
    }

    #[test]
    fn a_continuous_variable_puts_the_model_outside_enumeration() {
        let source = concat!(
            "NAME C\n",
            "ROWS\n",
            " N obj\n",
            " L c1\n",
            "COLUMNS\n",
            " x obj -1 c1 1\n",
            "RHS\n",
            " rhs c1 4\n",
            "BOUNDS\n",
            " UP b x 9\n",
            "ENDATA\n",
        );
        assert!(matches!(
            best(&model(source), 1 << 12),
            Truth::Beyond {
                reason: "a variable is continuous"
            }
        ));
    }

    #[test]
    fn an_unbounded_integer_variable_puts_it_outside_too() {
        let source = concat!(
            "NAME U\n",
            "ROWS\n",
            " N obj\n",
            " L c1\n",
            "COLUMNS\n",
            " MARKER 'MARKER' 'INTORG'\n",
            " x obj 1 c1 1\n",
            " MARKER 'MARKER' 'INTEND'\n",
            "RHS\n",
            " rhs c1 4\n",
            "ENDATA\n",
        );
        assert!(matches!(
            best(&model(source), 1 << 12),
            Truth::Beyond {
                reason: "an integer variable is unbounded"
            }
        ));
    }
}
