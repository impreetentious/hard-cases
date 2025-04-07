//! One fixture set per transform: a case that applies, cases just outside
//! applicability, and the rounding directions that are easiest to get
//! backwards.
//!
//! The campaign beside this one checks the invariants over generated chains,
//! which is where the interactions show up. This checks the rules themselves,
//! on models small enough to work out with a pencil, because a rule that is
//! wrong in a way the invariants happen not to catch is a rule that is wrong.
//!
//! Every case is named for what it is about, and every refusal asserts the
//! reason as well as the fact. A rule that declined for the wrong cause would
//! pass a test that only checked it declined.

use hc_core::Model;
use hc_exact::Rational;
use hc_io::mps::{Form, read};
use hc_presolve::rule::Inapplicable;
use hc_presolve::{Rule, apply};

fn model(source: &str) -> Model {
    read(source.as_bytes(), Form::Free)
        .unwrap_or_else(|error| panic!("the fixture does not parse: {error}\n{source}"))
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

/// A model with the parts a fixture wants, assembled from pieces.
fn build(rows: &str, columns: &str, rhs: &str, bounds: &str) -> Model {
    model(&format!(
        "NAME F\nROWS\n N obj\n{rows}COLUMNS\n{columns}RHS\n{rhs}BOUNDS\n{bounds}ENDATA\n"
    ))
}

// ---------------------------------------------------------------------------
// T1 FixedColumn
// ---------------------------------------------------------------------------

#[test]
fn t1_substitutes_a_fixed_column_into_the_rows_and_the_constant() {
    // x is fixed at 2 and appears in r0 with coefficient 3, so r0's upper side
    // moves from 10 to 4 and the objective constant picks up 5 times 2.
    let before = build(
        " L r0\n",
        " x obj 5 r0 3\n y obj 1 r0 1\n",
        " rhs r0 10\n",
        " FX b x 2\n",
    );
    let after = apply(
        &before,
        &Rule::FixedColumn {
            column: 0,
            value: whole(2),
        },
    )
    .expect("a fixed column substitutes");

    assert_eq!(after.variables().len(), 1);
    assert_eq!(after.variables()[0].name, "y");
    assert_eq!(after.objective_constant().to_canonical_string(), "10");
    assert_eq!(
        after.rows()[0]
            .upper
            .as_ref()
            .map(Rational::to_canonical_string),
        Some("4".to_owned())
    );
}

#[test]
fn t1_declines_when_the_bounds_have_not_closed() {
    let before = build(" L r0\n", " x obj 1 r0 1\n", " rhs r0 4\n", " UP b x 3\n");
    assert_eq!(
        apply(
            &before,
            &Rule::FixedColumn {
                column: 0,
                value: whole(1)
            }
        ),
        Err(Inapplicable::Condition)
    );
}

#[test]
fn t1_declines_a_fractional_value_on_an_integer_column() {
    let before = build(
        " L r0\n",
        " MARKER 'MARKER' 'INTORG'\n x obj 1 r0 1\n MARKER 'MARKER' 'INTEND'\n",
        " rhs r0 4\n",
        " FX b x 1.5\n",
    );
    assert_eq!(
        apply(
            &before,
            &Rule::FixedColumn {
                column: 0,
                value: ratio(3, 2)
            }
        ),
        Err(Inapplicable::Condition),
        "an integer column fixed at a fraction has no feasible point and is not a T1"
    );
}

// ---------------------------------------------------------------------------
// T2 EmptyColumn
// ---------------------------------------------------------------------------

#[test]
fn t2_rests_a_costless_empty_column_at_zero() {
    let before = build(
        " L r0\n",
        " x obj 0 r0 0\n y obj 1 r0 1\n",
        " rhs r0 4\n",
        " LO b x -5\n UP b x 5\n",
    );
    let after = apply(
        &before,
        &Rule::EmptyColumn {
            column: 0,
            value: Rational::zero(),
        },
    )
    .expect("a costless empty column rests at zero");
    assert_eq!(after.variables().len(), 1);
    assert_eq!(after.objective_constant().to_canonical_string(), "0");
}

#[test]
fn t2_rests_a_costed_empty_column_at_its_improving_endpoint() {
    // A positive objective coefficient wants the column as small as possible.
    let before = build(
        " L r0\n",
        " x obj 3 r0 0\n y obj 1 r0 1\n",
        " rhs r0 4\n",
        " LO b x -2\n UP b x 7\n",
    );
    let after = apply(
        &before,
        &Rule::EmptyColumn {
            column: 0,
            value: whole(-2),
        },
    )
    .expect("a costed empty column rests at its improving endpoint");
    assert_eq!(after.objective_constant().to_canonical_string(), "-6");

    // A negative coefficient wants it as large as possible.
    let before = build(
        " L r0\n",
        " x obj -3 r0 0\n y obj 1 r0 1\n",
        " rhs r0 4\n",
        " LO b x -2\n UP b x 7\n",
    );
    let after = apply(
        &before,
        &Rule::EmptyColumn {
            column: 0,
            value: whole(7),
        },
    )
    .expect("a costed empty column rests at its improving endpoint");
    assert_eq!(after.objective_constant().to_canonical_string(), "-21");
}

#[test]
fn t2_declines_when_the_improving_endpoint_is_infinite() {
    // The improving direction is downward and there is no lower bound, so
    // presolve does not get to decide the model is unbounded.
    let before = build(
        " L r0\n",
        " x obj 3 r0 0\n y obj 1 r0 1\n",
        " rhs r0 4\n",
        " FR b x\n",
    );
    assert_eq!(
        apply(
            &before,
            &Rule::EmptyColumn {
                column: 0,
                value: Rational::zero()
            }
        ),
        Err(Inapplicable::Infinite)
    );
}

// ---------------------------------------------------------------------------
// T3 SingletonRow
// ---------------------------------------------------------------------------

#[test]
fn t3_turns_a_one_column_row_into_a_bound() {
    // 3x <= 10 gives x <= 10/3, which intersects with the existing upper of 9.
    let before = build(" L r0\n", " x obj 1 r0 3\n", " rhs r0 10\n", " UP b x 9\n");
    let after = apply(&before, &Rule::SingletonRow { row: 0, column: 0 })
        .expect("a singleton row is a bound");
    assert!(after.rows().is_empty());
    assert_eq!(
        after.variables()[0]
            .upper
            .as_ref()
            .map(Rational::to_canonical_string),
        Some("10/3".to_owned())
    );
}

#[test]
fn t3_swaps_the_sides_when_the_coefficient_is_negative() {
    // -2x >= 6 gives x <= -3: the side swaps, which is the part of this rule
    // most easily got backwards.
    let before = build(" G r0\n", " x obj 1 r0 -2\n", " rhs r0 6\n", " FR b x\n");
    let after = apply(&before, &Rule::SingletonRow { row: 0, column: 0 })
        .expect("a singleton row is a bound");
    assert_eq!(
        after.variables()[0]
            .upper
            .as_ref()
            .map(Rational::to_canonical_string),
        Some("-3".to_owned())
    );
    assert_eq!(after.variables()[0].lower, None);
}

#[test]
fn t3_rounds_an_integer_bound_inward_on_both_sides() {
    // 2x <= 7 gives x <= 3.5, which rounds down to 3 for an integer column.
    let before = build(
        " L r0\n",
        " MARKER 'MARKER' 'INTORG'\n x obj 1 r0 2\n MARKER 'MARKER' 'INTEND'\n",
        " rhs r0 7\n",
        " UI b x 9\n",
    );
    let after = apply(&before, &Rule::SingletonRow { row: 0, column: 0 })
        .expect("a singleton row is a bound");
    assert_eq!(
        after.variables()[0]
            .upper
            .as_ref()
            .map(Rational::to_canonical_string),
        Some("3".to_owned())
    );

    // 2x >= 7 gives x >= 3.5, which rounds up to 4.
    let before = build(
        " G r0\n",
        " MARKER 'MARKER' 'INTORG'\n x obj 1 r0 2\n MARKER 'MARKER' 'INTEND'\n",
        " rhs r0 7\n",
        " UI b x 9\n",
    );
    let after = apply(&before, &Rule::SingletonRow { row: 0, column: 0 })
        .expect("a singleton row is a bound");
    assert_eq!(
        after.variables()[0]
            .lower
            .as_ref()
            .map(Rational::to_canonical_string),
        Some("4".to_owned())
    );
}

#[test]
fn t3_reports_an_empty_intersection_as_a_contradiction() {
    // x >= 5 against an upper bound of 2.
    let before = build(" G r0\n", " x obj 1 r0 1\n", " rhs r0 5\n", " UP b x 2\n");
    assert_eq!(
        apply(&before, &Rule::SingletonRow { row: 0, column: 0 }),
        Err(Inapplicable::Contradiction)
    );
}

// ---------------------------------------------------------------------------
// T4 FreeSingletonColumn
// ---------------------------------------------------------------------------

#[test]
fn t4_substitutes_a_free_column_out_of_its_one_equality() {
    // z is free and appears only in r0: 2z + y = 6, so z = (6 - y)/2. The
    // objective 4z + y becomes 4(6-y)/2 + y = 12 - 2y + y = 12 - y.
    let before = build(
        " E r0\n",
        " z obj 4 r0 2\n y obj 1 r0 1\n",
        " rhs r0 6\n",
        " FR b z\n",
    );
    let after = apply(&before, &Rule::FreeSingletonColumn { row: 0, column: 0 })
        .expect("a free singleton column substitutes");
    assert!(after.rows().is_empty());
    assert_eq!(after.variables().len(), 1);
    assert_eq!(after.objective_constant().to_canonical_string(), "12");
    assert_eq!(after.variables()[0].objective.to_canonical_string(), "-1");
}

#[test]
fn t4_declines_a_ranged_row_an_integer_column_and_a_bounded_one() {
    let ranged = build(
        " L r0\n",
        " z obj 4 r0 2\n y obj 1 r0 1\n",
        " rhs r0 6\n",
        " FR b z\n",
    );
    assert_eq!(
        apply(&ranged, &Rule::FreeSingletonColumn { row: 0, column: 0 }),
        Err(Inapplicable::Condition),
        "a substitution into an inequality is a restriction, not an equivalence"
    );

    let bounded = build(
        " E r0\n",
        " z obj 4 r0 2\n y obj 1 r0 1\n",
        " rhs r0 6\n",
        " LO b z 0\n",
    );
    assert_eq!(
        apply(&bounded, &Rule::FreeSingletonColumn { row: 0, column: 0 }),
        Err(Inapplicable::Condition)
    );

    let integral = build(
        " E r0\n",
        " MARKER 'MARKER' 'INTORG'\n z obj 4 r0 2\n MARKER 'MARKER' 'INTEND'\n y obj 1 r0 1\n",
        " rhs r0 6\n",
        " FR b z\n",
    );
    assert_eq!(
        apply(&integral, &Rule::FreeSingletonColumn { row: 0, column: 0 }),
        Err(Inapplicable::Condition)
    );
}

// ---------------------------------------------------------------------------
// T5 RedundantRow
// ---------------------------------------------------------------------------

#[test]
fn t5_removes_a_row_the_bounds_already_imply() {
    // x and y both in [0, 2], so x + y is at most 4, and the row allows 10.
    let before = build(
        " L r0\n",
        " x obj 1 r0 1\n y obj 1 r0 1\n",
        " rhs r0 10\n",
        " UP b x 2\n UP b y 2\n",
    );
    let after = apply(&before, &Rule::RedundantRow { row: 0 }).expect("the bounds imply the row");
    assert!(after.rows().is_empty());
    assert_eq!(after.variables().len(), 2);
}

#[test]
fn t5_declines_just_outside_and_with_an_infinite_contributor() {
    let tight = build(
        " L r0\n",
        " x obj 1 r0 1\n y obj 1 r0 1\n",
        " rhs r0 3\n",
        " UP b x 2\n UP b y 2\n",
    );
    assert_eq!(
        apply(&tight, &Rule::RedundantRow { row: 0 }),
        Err(Inapplicable::Condition),
        "a maximum activity of 4 against an upper side of 3 is not redundant"
    );

    let open = build(
        " L r0\n",
        " x obj 1 r0 1\n y obj 1 r0 1\n",
        " rhs r0 10\n",
        " UP b x 2\n",
    );
    assert_eq!(
        apply(&open, &Rule::RedundantRow { row: 0 }),
        Err(Inapplicable::Infinite)
    );
}

// ---------------------------------------------------------------------------
// T6 ForcingRow
// ---------------------------------------------------------------------------

#[test]
fn t6_pins_every_term_when_a_side_sits_at_its_extreme_activity() {
    // x and y in [0, 2]; the maximum of x + y is 4 and the row demands at
    // least 4, so both are pinned at 2.
    let before = build(
        " G r0\n",
        " x obj 1 r0 1\n y obj 1 r0 1\n",
        " rhs r0 4\n",
        " UP b x 2\n UP b y 2\n",
    );
    let after = apply(
        &before,
        &Rule::ForcingRow {
            row: 0,
            upper: false,
        },
    )
    .expect("a lower side at the maximum activity forces every term");
    for variable in after.variables() {
        assert_eq!(
            variable.lower.as_ref().map(Rational::to_canonical_string),
            Some("2".to_owned())
        );
        assert_eq!(
            variable.upper.as_ref().map(Rational::to_canonical_string),
            Some("2".to_owned())
        );
    }
}

#[test]
fn t6_pins_the_other_way_on_an_upper_side_at_the_minimum() {
    let before = build(
        " L r0\n",
        " x obj 1 r0 1\n y obj 1 r0 1\n",
        " rhs r0 0\n",
        " UP b x 2\n UP b y 2\n",
    );
    let after = apply(
        &before,
        &Rule::ForcingRow {
            row: 0,
            upper: true,
        },
    )
    .expect("an upper side at the minimum activity forces every term");
    for variable in after.variables() {
        assert_eq!(
            variable.upper.as_ref().map(Rational::to_canonical_string),
            Some("0".to_owned())
        );
    }
}

#[test]
fn t6_declines_with_slack_and_with_an_infinite_endpoint() {
    let slack = build(
        " G r0\n",
        " x obj 1 r0 1\n y obj 1 r0 1\n",
        " rhs r0 3\n",
        " UP b x 2\n UP b y 2\n",
    );
    assert_eq!(
        apply(
            &slack,
            &Rule::ForcingRow {
                row: 0,
                upper: false
            }
        ),
        Err(Inapplicable::Condition)
    );

    let open = build(
        " G r0\n",
        " x obj 1 r0 1\n y obj 1 r0 1\n",
        " rhs r0 4\n",
        " UP b x 2\n",
    );
    assert_eq!(
        apply(
            &open,
            &Rule::ForcingRow {
                row: 0,
                upper: false
            }
        ),
        Err(Inapplicable::Infinite)
    );
}

// ---------------------------------------------------------------------------
// T7 TightenBound
// ---------------------------------------------------------------------------

#[test]
fn t7_derives_a_tighter_bound_from_a_row_and_the_other_terms() {
    // 2x + y <= 7 with y at least 1 gives 2x <= 6, so x <= 3, tighter than 9.
    let before = build(
        " L r0\n",
        " x obj 1 r0 2\n y obj 1 r0 1\n",
        " rhs r0 7\n",
        " UP b x 9\n LO b y 1\n UP b y 5\n",
    );
    let after = apply(
        &before,
        &Rule::TightenBound {
            row: 0,
            column: 0,
            upper: true,
            value: whole(3),
        },
    )
    .expect("the row implies a tighter bound");
    assert_eq!(
        after.variables()[0]
            .upper
            .as_ref()
            .map(Rational::to_canonical_string),
        Some("3".to_owned())
    );
}

#[test]
fn t7_flips_the_side_when_the_coefficient_is_negative() {
    // -2x + y <= 7 with y at most 5 gives -2x <= 7 - 1, so x >= -3 once y is
    // at its minimum of 1: the derived bound lands on the lower side.
    let before = build(
        " L r0\n",
        " x obj 1 r0 -2\n y obj 1 r0 1\n",
        " rhs r0 7\n",
        " LO b x -9\n UP b x 9\n LO b y 1\n UP b y 5\n",
    );
    let after = apply(
        &before,
        &Rule::TightenBound {
            row: 0,
            column: 0,
            upper: false,
            value: whole(-3),
        },
    )
    .expect("the derived bound lands on the lower side");
    assert_eq!(
        after.variables()[0]
            .lower
            .as_ref()
            .map(Rational::to_canonical_string),
        Some("-3".to_owned())
    );
}

#[test]
fn t7_declines_without_a_strict_improvement() {
    let before = build(
        " L r0\n",
        " x obj 1 r0 2\n y obj 1 r0 1\n",
        " rhs r0 7\n",
        " UP b x 3\n LO b y 1\n UP b y 5\n",
    );
    assert_eq!(
        apply(
            &before,
            &Rule::TightenBound {
                row: 0,
                column: 0,
                upper: true,
                value: whole(3)
            }
        ),
        Err(Inapplicable::NoImprovement),
        "a bound that is already this tight is not a tightening"
    );
}

#[test]
fn t7_rounds_a_derived_integer_bound_inward() {
    // 2x + y <= 8 with y at least 1 gives x <= 3.5, which rounds down to 3.
    let before = build(
        " L r0\n",
        " MARKER 'MARKER' 'INTORG'\n x obj 1 r0 2\n MARKER 'MARKER' 'INTEND'\n y obj 1 r0 1\n",
        " rhs r0 8\n",
        " UI b x 9\n LO b y 1\n UP b y 5\n",
    );
    let after = apply(
        &before,
        &Rule::TightenBound {
            row: 0,
            column: 0,
            upper: true,
            value: whole(3),
        },
    )
    .expect("the derived bound rounds inward");
    assert_eq!(
        after.variables()[0]
            .upper
            .as_ref()
            .map(Rational::to_canonical_string),
        Some("3".to_owned())
    );
}

// ---------------------------------------------------------------------------
// T8 DuplicateColumnAggregate
// ---------------------------------------------------------------------------

#[test]
fn t8_aggregates_two_identical_continuous_columns() {
    let before = build(
        " L r0\n",
        " a obj 2 r0 3\n b obj 2 r0 3\n",
        " rhs r0 12\n",
        " UP b a 4\n UP b b 6\n",
    );
    let after = apply(&before, &Rule::DuplicateColumn { keep: 0, drop: 1 })
        .expect("identical columns aggregate");
    assert_eq!(after.variables().len(), 1);
    assert_eq!(
        after.variables()[0]
            .upper
            .as_ref()
            .map(Rational::to_canonical_string),
        Some("10".to_owned())
    );
}

#[test]
fn t8_declines_a_differing_objective_and_an_infinite_bound() {
    let differing = build(
        " L r0\n",
        " a obj 2 r0 3\n b obj 5 r0 3\n",
        " rhs r0 12\n",
        " UP b a 4\n UP b b 6\n",
    );
    assert_eq!(
        apply(&differing, &Rule::DuplicateColumn { keep: 0, drop: 1 }),
        Err(Inapplicable::Condition)
    );

    let open = build(
        " L r0\n",
        " a obj 2 r0 3\n b obj 2 r0 3\n",
        " rhs r0 12\n",
        " UP b a 4\n",
    );
    assert_eq!(
        apply(&open, &Rule::DuplicateColumn { keep: 0, drop: 1 }),
        Err(Inapplicable::Infinite)
    );
}

#[test]
fn t8_declines_columns_that_differ_in_a_single_row() {
    let before = build(
        " L r0\n L r1\n",
        " a obj 2 r0 3\n a r1 1\n b obj 2 r0 3\n b r1 2\n",
        " rhs r0 12 r1 8\n",
        " UP b a 4\n UP b b 6\n",
    );
    assert_eq!(
        apply(&before, &Rule::DuplicateColumn { keep: 0, drop: 1 }),
        Err(Inapplicable::Condition),
        "proportional is not identical, and only identical aggregates"
    );
}
