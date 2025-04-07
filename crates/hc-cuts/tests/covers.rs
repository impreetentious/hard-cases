//! Cover cuts, on the frozen vector and against brute force.
//!
//! The hand-worked case is the one in the frozen inference vectors: a knapsack
//! whose weights are 7, 6, 7 and 3 against a capacity of 12, with the cover
//! `{x1, x2}` and the lift order `x3, x4`. The lifting coefficients are worked
//! out there with a pencil, and they are what this must reproduce.
//!
//! The campaign is the same discipline as the Gomory one: derive cuts on tiny
//! models, enumerate every integer point, and require that no cut removes a
//! feasible one. A cover cut with an overstated lifting coefficient does not
//! look wrong; it looks like a stronger cut.

use hc_core::{Model, Xoshiro256PlusPlus};
use hc_cuts::cover::{Refused, is_cover, knapsack_from, lift};
use hc_exact::Rational;
use hc_io::mps::{Form, read};
use hc_lp::exact::Canonical;

/// `7 x1 + 6 x2 + 7 x3 + 3 x4 <= 12`, every variable binary.
const MODEL_C: &str = concat!(
    "NAME C\n",
    "ROWS\n",
    " N obj\n",
    " L r0\n",
    "COLUMNS\n",
    " MARKER 'MARKER' 'INTORG'\n",
    " x1 obj -1 r0 7\n",
    " x2 obj -1 r0 6\n",
    " x3 obj -1 r0 7\n",
    " x4 obj -1 r0 3\n",
    " MARKER 'MARKER' 'INTEND'\n",
    "RHS\n",
    " rhs r0 12\n",
    "BOUNDS\n",
    " UI b x1 1\n",
    " UI b x2 1\n",
    " UI b x3 1\n",
    " UI b x4 1\n",
    "ENDATA\n",
);

fn shape(source: &str) -> (Model, Canonical) {
    let model = read(source.as_bytes(), Form::Free).expect("the source parses");
    let canonical = Canonical::of(&model);
    (model, canonical)
}

#[test]
fn the_hand_worked_lifting_is_reproduced() {
    let (_, canonical) = shape(MODEL_C);
    let knapsack = knapsack_from(&canonical, 0).expect("the row is a binary knapsack");
    assert_eq!(knapsack.capacity, 12);
    assert_eq!(knapsack.weights, vec![(0, 7), (1, 6), (2, 7), (3, 3)]);

    // 7 + 6 = 13 > 12, so {x1, x2} is a cover and the base cut is
    // x1 + x2 <= 1.
    assert!(is_cover(&knapsack, &[0, 1]));
    let cut = lift(&knapsack, &[0, 1], &[2, 3]).expect("the lifting runs");
    assert_eq!(cut.limit, 1);

    // Fixing x3 to one leaves capacity 5, in which neither cover member fits,
    // so the maximum is zero and alpha_3 = 1 - 0 = 1. Fixing x4 leaves 9, in
    // which any single one fits and no pair does, so the maximum is one and
    // alpha_4 = 0.
    assert_eq!(cut.coefficients, vec![(0, 1), (1, 1), (2, 1)]);
}

#[test]
fn trap_cover_not_strict() {
    let (_, canonical) = shape(MODEL_C);
    let knapsack = knapsack_from(&canonical, 0).expect("the row is a binary knapsack");
    // 7 + 3 = 10, which does not exceed 12. The cut x1 + x4 <= 1 would remove
    // the feasible point where both are one.
    assert!(!is_cover(&knapsack, &[0, 3]));
    assert_eq!(lift(&knapsack, &[0, 3], &[]), Err(Refused::NotACover));
}

#[test]
fn a_row_that_is_not_a_binary_knapsack_is_refused() {
    // The same row with x4 a general integer over zero to three.
    let source = MODEL_C.replace(" UI b x4 1\n", " UI b x4 3\n");
    let (_, canonical) = shape(&source);
    assert_eq!(
        knapsack_from(&canonical, 0),
        Err(Refused::NotBinary { column: 3 })
    );

    // And a row with a fractional weight.
    let source = MODEL_C.replace(" x4 obj -1 r0 3\n", " x4 obj -1 r0 2.5\n");
    let (_, canonical) = shape(&source);
    assert_eq!(
        knapsack_from(&canonical, 0),
        Err(Refused::NotPositiveInteger)
    );
}

#[test]
fn a_variable_too_heavy_to_fit_should_have_been_fixed_rather_than_lifted() {
    let source = MODEL_C.replace(" x3 obj -1 r0 7\n", " x3 obj -1 r0 20\n");
    let (_, canonical) = shape(&source);
    let knapsack = knapsack_from(&canonical, 0).expect("the row is a binary knapsack");
    assert_eq!(
        lift(&knapsack, &[0, 1], &[2]),
        Err(Refused::NoFeasibleState { column: 2 })
    );
}

#[test]
fn the_lift_order_may_not_repeat_or_name_a_cover_member() {
    let (_, canonical) = shape(MODEL_C);
    let knapsack = knapsack_from(&canonical, 0).expect("the row is a binary knapsack");
    assert_eq!(
        lift(&knapsack, &[0, 1], &[2, 2]),
        Err(Refused::BadOrder { column: 2 })
    );
    assert_eq!(
        lift(&knapsack, &[0, 1], &[0]),
        Err(Refused::BadOrder { column: 0 })
    );
}

/// Every binary assignment of a model, as exact values.
fn assignments(count: usize) -> Vec<Vec<Rational>> {
    let mut out = Vec::new();
    for mask in 0..(1_u32 << count) {
        out.push(
            (0..count)
                .map(|bit| Rational::from_i64(i64::from((mask >> bit) & 1)))
                .collect(),
        );
    }
    out
}

fn generate(generator: &mut Xoshiro256PlusPlus, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME K\nROWS\n N obj\n L r0\n");
    source.push_str("COLUMNS\n MARKER 'MARKER' 'INTORG'\n");
    for column in 0..columns {
        let weight = 1 + generator.next_below(9);
        let _ = writeln!(source, " c{column} obj -1 r0 {weight}");
    }
    source.push_str(" MARKER 'MARKER' 'INTEND'\nRHS\n");
    let capacity = 2 + generator.next_below(20);
    let _ = writeln!(source, " rhs r0 {capacity}");
    source.push_str("BOUNDS\n");
    for column in 0..columns {
        let _ = writeln!(source, " UI b c{column} 1");
    }
    source.push_str("ENDATA\n");
    source
}

#[test]
fn trap_cover_lift_dp() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x0c07_0000_0000_0002);
    let mut derived = 0;

    for _ in 0..400 {
        let columns = 4 + (derived % 2);
        let source = generate(&mut generator, columns);
        let (model, canonical) = shape(&source);
        let Ok(knapsack) = knapsack_from(&canonical, 0) else {
            continue;
        };
        let points = assignments(columns);

        // Every subset as a candidate cover, and the remaining columns lifted
        // in index order. Most subsets are not covers; the ones that are give
        // cuts, and every cut must hold at every feasible assignment.
        for mask in 1..(1_u32 << columns) {
            let cover: Vec<usize> = (0..columns).filter(|bit| (mask >> bit) & 1 == 1).collect();
            if !is_cover(&knapsack, &cover) {
                continue;
            }
            let order: Vec<usize> = (0..columns).filter(|bit| (mask >> bit) & 1 == 0).collect();
            let Ok(cut) = lift(&knapsack, &cover, &order) else {
                continue;
            };
            derived += 1;

            for point in &points {
                let activity = model.row_activity(hc_core::RowId(0), point);
                if activity.cmp_value(
                    model.rows()[0]
                        .upper
                        .as_ref()
                        .expect("the row has a capacity"),
                ) == core::cmp::Ordering::Greater
                {
                    continue;
                }
                let mut total = 0_i64;
                for (column, coefficient) in &cut.coefficients {
                    if point[*column] == Rational::one() {
                        total += coefficient;
                    }
                }
                assert!(
                    total <= cut.limit,
                    "a lifted cover cut removed a feasible point\n{source}\ncut {cut:?}\npoint {point:?}"
                );
            }
        }
    }

    assert!(derived > 200, "only {derived} lifted cuts were derived");
}
