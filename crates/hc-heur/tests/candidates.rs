//! Heuristics propose; the gate disposes.
//!
//! What is checked here is the division of labour. A heuristic produces a
//! candidate and reports what happened to it, including when nothing happened.
//! It never decides whether a candidate is acceptable, and a candidate that is
//! not genuinely feasible comes back refused rather than accepted quietly.

use hc_exact::Rational;
use hc_heur::Outcome;
use hc_heur::dive::{Fix, dive, next_fix};
use hc_heur::pump::pump;
use hc_heur::round::{candidate, nearest, round};
use hc_io::mps::{Form, read};
use hc_lp::exact::Canonical;

/// `3x + 2y <= 7`, both integer over zero to three.
const MODEL: &str = concat!(
    "NAME H\n",
    "ROWS\n N obj\n L r0\n",
    "COLUMNS\n MARKER 'MARKER' 'INTORG'\n",
    " x obj -5 r0 3\n y obj -3 r0 2\n",
    " MARKER 'MARKER' 'INTEND'\n",
    "RHS\n rhs r0 7\n",
    "BOUNDS\n UI b x 3\n UI b y 3\n",
    "ENDATA\n",
);

fn shape() -> Canonical {
    Canonical::of(&read(MODEL.as_bytes(), Form::Free).expect("the source parses"))
}

fn whole(value: i64) -> Rational {
    Rational::from_i64(value)
}

fn ratio(top: i64, bottom: i64) -> Rational {
    Rational::new(
        hc_exact::BigInt::from_i64(top),
        hc_exact::BigInt::from_i64(bottom),
    )
    .expect("a nonzero denominator")
}

/// The exact gate: feasible against the model, integral where declared, and
/// the objective it attains.
fn gate(canonical: &Canonical) -> impl Fn(&[Rational]) -> Result<Rational, String> + '_ {
    move |point: &[Rational]| {
        if point.len() != canonical.columns() {
            return Err(String::from("the wrong length"));
        }
        for (row, residual) in canonical.residual_at(point).iter().enumerate() {
            if !residual.is_zero() {
                return Err(format!("row {row} does not hold"));
            }
        }
        if let Some(column) = canonical.violations(point).first() {
            return Err(format!("column {column} is outside its bounds"));
        }
        for (column, value) in point.iter().enumerate() {
            if canonical.is_integral(column) && !value.is_integer() {
                return Err(format!("column {column} is fractional"));
            }
        }
        Ok(canonical.objective_at(point))
    }
}

/// A point over the working shape from the two variables.
fn point(x: Rational, y: Rational) -> Vec<Rational> {
    let activity = x.mul(&whole(3)).add(&y.mul(&whole(2)));
    vec![x, y, activity]
}

#[test]
fn rounding_moves_to_the_nearest_integer_and_repairs_the_rows() {
    assert_eq!(nearest(&ratio(1, 3)).to_canonical_string(), "0");
    assert_eq!(nearest(&ratio(2, 3)).to_canonical_string(), "1");
    // A tie goes upward, which is arbitrary and fixed.
    assert_eq!(nearest(&ratio(1, 2)).to_canonical_string(), "1");

    let canonical = shape();
    // x = 7/3, y = 0: the relaxation's optimum. Rounding gives x = 2.
    let rounded = candidate(&canonical, &point(ratio(7, 3), whole(0)));
    assert_eq!(rounded[0].to_canonical_string(), "2");
    // And the row activity is recomputed rather than carried over.
    assert_eq!(rounded[2].to_canonical_string(), "6");
}

#[test]
fn a_rounded_candidate_that_is_feasible_is_reported_as_found() {
    let canonical = shape();
    let outcome = round(&canonical, &point(ratio(7, 3), whole(0)), &gate(&canonical));
    let Outcome::Found { objective, .. } = &outcome else {
        panic!("x = 2 is feasible: {outcome:?}");
    };
    assert_eq!(objective.to_canonical_string(), "-10");
}

#[test]
fn a_rounded_candidate_that_is_not_feasible_is_reported_as_refused() {
    let canonical = shape();
    // x = 2.6 rounds to 3, which needs 9 of the row's 7.
    let outcome = round(
        &canonical,
        &point(ratio(13, 5), whole(0)),
        &gate(&canonical),
    );
    assert!(
        matches!(outcome, Outcome::Refused { .. }),
        "a rounded point that breaks the row must be refused: {outcome:?}"
    );
    assert_eq!(outcome.word(), "refused");
}

#[test]
fn a_candidate_of_the_wrong_shape_is_abandoned_rather_than_padded() {
    let canonical = shape();
    let outcome = round(&canonical, &[whole(1)], &gate(&canonical));
    assert!(matches!(outcome, Outcome::Abandoned { .. }));
}

#[test]
fn a_dive_fixes_the_most_fractional_variable_first() {
    let canonical = shape();
    let fix = next_fix(&canonical, &point(ratio(7, 3), ratio(1, 2))).expect("two fractions");
    // y is exactly one half away and x is one third, so y goes first.
    assert_eq!(fix.column, 1);
    assert_eq!(fix.value.to_canonical_string(), "1");
    assert_eq!(next_fix(&canonical, &point(whole(2), whole(0))), None);
}

#[test]
fn a_dive_that_becomes_infeasible_says_so_rather_than_pretending() {
    let canonical = shape();
    let mut resolve = |_: &[Fix]| None;
    let outcome = dive(
        &canonical,
        &point(ratio(7, 3), whole(0)),
        4,
        &mut resolve,
        &gate(&canonical),
    );
    assert!(matches!(outcome, Outcome::Abandoned { .. }));
    assert_eq!(outcome.word(), "abandoned");
}

#[test]
fn a_dive_that_reaches_an_integral_point_offers_it_to_the_gate() {
    let canonical = shape();
    let mut resolve = |_: &[Fix]| Some(point(whole(2), whole(0)));
    let outcome = dive(
        &canonical,
        &point(ratio(7, 3), whole(0)),
        4,
        &mut resolve,
        &gate(&canonical),
    );
    assert!(outcome.is_found(), "{outcome:?}");
}

#[test]
fn the_pump_stops_when_the_two_points_meet() {
    let canonical = shape();
    // A relaxation that always answers with the integral point it was given.
    let mut resolve = |target: &[Rational]| Some(target.to_vec());
    let outcome = pump(
        &canonical,
        &point(ratio(7, 3), whole(0)),
        8,
        0,
        &mut resolve,
        &gate(&canonical),
    );
    assert!(outcome.is_found(), "{outcome:?}");
}

#[test]
fn the_pump_gives_up_rather_than_running_forever() {
    let canonical = shape();
    // A relaxation that always answers with the same fractional point, so the
    // two never meet.
    let mut resolve = |_: &[Rational]| Some(point(ratio(7, 3), ratio(1, 2)));
    let outcome = pump(
        &canonical,
        &point(ratio(7, 3), ratio(1, 2)),
        5,
        0,
        &mut resolve,
        &gate(&canonical),
    );
    assert!(matches!(outcome, Outcome::Abandoned { .. }), "{outcome:?}");
}

#[test]
fn the_pump_is_reproducible_including_its_restarts() {
    let canonical = shape();
    let run = || {
        let mut resolve = |_: &[Rational]| Some(point(ratio(7, 3), ratio(1, 2)));
        pump(
            &canonical,
            &point(ratio(7, 3), ratio(1, 2)),
            6,
            42,
            &mut resolve,
            &gate(&canonical),
        )
    };
    assert_eq!(run(), run());
}
