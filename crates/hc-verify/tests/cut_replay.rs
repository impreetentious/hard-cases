//! The verifier's cut recomputation against the frozen vectors.
//!
//! The vectors are hand-worked derivations from `spec/v1/vectors/proof`. What
//! is checked here is that this crate reproduces them from the model and the
//! combination alone — no basis, no bound state, nothing the certificate could
//! have got wrong in its favour.

use hc_verify::vcut::{View, cover, gomory};
use hc_verify::vmodel::{Cell, Column, Constraint, Direction, Instance, Tag};
use hc_verify::vparse::{Layout, read};
use hc_verify::vrat::Frac;

fn instance(source: &str) -> Instance {
    read(source.as_bytes(), Layout::Free).expect("the source parses")
}

fn whole(value: i64) -> Frac {
    Frac::from_i64(value)
}

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
    " UI b x1 1\n UI b x2 1\n UI b x3 1\n UI b x4 1\n",
    "ENDATA\n",
);

#[test]
fn the_hand_worked_cover_lifting_is_reproduced() {
    let model = instance(MODEL_C);
    let view = View::of(&model);
    let cut = cover(&model, &view, 0, &[0, 1], &[2, 3]).expect("the lifting runs");

    // x1 + x2 + x3 <= 1, written the one way every inequality is written.
    assert_eq!(cut.right.to_canonical(), "-1");
    let rendered: Vec<(u32, String)> = cut
        .coefficients
        .iter()
        .map(|(column, value)| (*column, value.to_canonical()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (0, "-1".to_owned()),
            (1, "-1".to_owned()),
            (2, "-1".to_owned()),
        ]
    );
}

#[test]
fn a_cover_that_does_not_exceed_the_capacity_is_refused() {
    let model = instance(MODEL_C);
    let view = View::of(&model);
    // 7 + 3 = 10 against 12.
    let refusal = cover(&model, &view, 0, &[0, 3], &[]).expect_err("not a cover");
    assert_eq!(refusal.code.text(), "HCC-CUT-07");
}

#[test]
fn a_general_integer_in_the_source_row_is_refused() {
    let source = MODEL_C.replace(" UI b x4 1\n", " UI b x4 3\n");
    let model = instance(&source);
    let view = View::of(&model);
    let refusal = cover(&model, &view, 0, &[0, 1], &[]).expect_err("not binary");
    assert_eq!(refusal.code.text(), "HCC-CUT-08");
}

/// A model whose relaxation is fractional, so a Gomory cut has something to
/// say: `2x + 2y <= 3` with both integer over zero and three.
fn fractional() -> Instance {
    Instance {
        tag: Tag::MpsFree,
        digest: [0; 32],
        written: Direction::Min,
        constant: Frac::zero(),
        columns: vec![
            Column {
                low: Some(whole(0)),
                high: Some(whole(3)),
                whole: true,
                cost: whole(-1),
                name: "x".to_owned(),
            },
            Column {
                low: Some(whole(0)),
                high: Some(whole(3)),
                whole: true,
                cost: whole(-1),
                name: "y".to_owned(),
            },
        ],
        rows: vec![Constraint {
            low: None,
            high: Some(whole(3)),
            name: "r0".to_owned(),
        }],
        cells: vec![
            Cell {
                column: 0,
                row: 0,
                value: whole(2),
            },
            Cell {
                column: 1,
                row: 0,
                value: whole(2),
            },
        ],
        cancelled: Vec::new(),
        dropped: Vec::new(),
    }
}

#[test]
fn a_gomory_cut_is_recomputed_from_the_combination_alone() {
    let model = fractional();
    let view = View::of(&model);
    // Half the row equation: (1/2)(2x + 2y - s) = 0 gives x + y - s/2 = 0,
    // which isolates x with coefficient one.
    let half = Frac::ratio(
        &hc_verify::vint::Int::from_i64(1),
        &hc_verify::vint::Int::from_i64(2),
    )
    .expect("one half");
    let cut = gomory(&view, &[(0, half)], 0).expect("the derivation runs");

    // Every integer point of the model satisfying 2x + 2y <= 3 must satisfy
    // the cut. There are three of them: (0,0), (1,0) and (0,1).
    for (x, y) in [(0_i64, 0_i64), (1, 0), (0, 1)] {
        let point = [whole(x), whole(y)];
        let mut activity = Frac::zero();
        for (column, coefficient) in &cut.coefficients {
            activity = activity.plus(&coefficient.times(&point[*column as usize]));
        }
        assert!(
            activity.compare(&cut.right) != core::cmp::Ordering::Less,
            "the cut removed the feasible point ({x}, {y}): {cut:?}"
        );
    }
}

#[test]
fn a_combination_that_isolates_nothing_is_refused() {
    let model = fractional();
    let view = View::of(&model);
    let refusal = gomory(&view, &[(0, Frac::one())], 0).expect_err("not isolated");
    assert_eq!(refusal.code.text(), "HCC-CUT-01");
}

/// A combination may only name rows visible at the node the cut is derived at.
#[test]
fn trap_tableau_row_scope() {
    let model = fractional();
    let view = View::of(&model);
    let refusal = gomory(&view, &[(9, Frac::one())], 0).expect_err("no such row");
    assert_eq!(refusal.code.text(), "HCC-SCOPE-05");
}
