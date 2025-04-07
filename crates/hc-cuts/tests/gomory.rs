//! The Gomory formula, on the frozen vector and against brute force.
//!
//! Two kinds of check, and both are needed. The frozen vector is a hand-worked
//! equation with one term in each of the formula's four quadrants, and it pins
//! the arithmetic against a derivation somebody did with a pencil. The
//! brute-force campaign takes every integer point of a tiny model and asserts
//! that no derived cut removes one, which is the property the formula exists to
//! have and the one a transcription error would break.
//!
//! An invalid cut is the worst failure this project has: the solver does not
//! crash, it reports a smaller optimal value than the truth with a
//! complete-looking tree behind it.

use hc_core::{Model, Xoshiro256PlusPlus};
use hc_cuts::gmi::{
    Derivation, Displacement, From, Refused, derive, formula, over_variables, reconstruct,
};
use hc_exact::{BigInt, Rational};
use hc_io::mps::{Form, read};
use hc_lp::exact::Canonical;

fn ratio(top: i64, bottom: i64) -> Rational {
    Rational::new(BigInt::from_i64(top), BigInt::from_i64(bottom)).expect("a nonzero denominator")
}

fn whole(value: i64) -> Rational {
    Rational::from_i64(value)
}

/// The hand-worked equation from the frozen vectors, built directly:
///
/// `z + (3/4) w1 + (5/4) w3 + (-1/2) w2 + (2/3) w4 = 7/3`
///
/// with `w1` and `w3` integer displacements and `w2` and `w4` continuous. Each
/// term lands in a different quadrant of the formula.
fn four_quadrants() -> Derivation {
    let term = |column: usize, top: i64, bottom: i64, integral: bool| Displacement {
        column,
        from: From::Lower,
        endpoint: Rational::zero(),
        coefficient: ratio(top, bottom),
        integral,
    };
    Derivation {
        isolated: 0,
        displacements: vec![
            term(1, 3, 4, true),
            term(2, -1, 2, false),
            term(3, 5, 4, true),
            term(4, 2, 3, false),
        ],
        constant: ratio(7, 3),
    }
}

#[test]
fn trap_gomory_formula_quadrants() {
    let cut = formula(&four_quadrants());

    // f = 7/3 - 2 = 1/3.
    //   w1: integer, a = 3/4, f_1 = 3/4 > f, so f(1-f_1)/(1-f) = 1/8.
    //   w3: integer, a = 5/4, f_3 = 1/4 <= f, so f_3 itself = 1/4.
    //   w2: continuous and negative, -f a / (1-f) = 1/4.
    //   w4: continuous and nonnegative, a itself = 2/3.
    let by_column: Vec<(usize, String)> = cut
        .coefficients
        .iter()
        .map(|(column, value)| (*column, value.to_canonical_string()))
        .collect();
    assert_eq!(
        by_column,
        vec![
            (1, "1/8".to_owned()),
            (2, "1/4".to_owned()),
            (3, "1/4".to_owned()),
            (4, "2/3".to_owned()),
        ]
    );
    // Every displacement is measured from zero here, so the right-hand side is
    // f itself.
    assert_eq!(cut.right.to_canonical_string(), "1/3");
}

#[test]
fn an_integral_constant_yields_no_cut() {
    let mut derivation = four_quadrants();
    derivation.constant = whole(2);
    // The formula would produce `0 >= 0`, which is sound and useless, so the
    // reconstruction refuses before it gets there.
    let source = concat!(
        "NAME G\nROWS\n N obj\n L r0\n",
        "COLUMNS\n MARKER 'MARKER' 'INTORG'\n x obj -1 r0 2\n MARKER 'MARKER' 'INTEND'\n",
        "RHS\n rhs r0 4\nBOUNDS\n UI b x 9\nENDATA\n",
    );
    let model = read(source.as_bytes(), Form::Free).expect("the source parses");
    let canonical = Canonical::of(&model);
    // The zero combination isolates nothing and gives an integral constant.
    assert!(matches!(
        reconstruct(&canonical, &[], 0),
        Err(Refused::NotIsolated { .. })
    ));
}

/// Every integer point inside a model's box, up to a cap.
fn box_points(model: &Model, cap: usize) -> Vec<Vec<Rational>> {
    let mut boxes: Vec<(i64, i64)> = Vec::new();
    let mut product = 1usize;
    for variable in model.variables() {
        let (low, high) = variable.effective_bounds();
        let (Some(low), Some(high)) = (low, high) else {
            return Vec::new();
        };
        let (Ok(first), Ok(last)) = (
            low.ceil().to_string().parse::<i64>(),
            high.floor().to_string().parse::<i64>(),
        ) else {
            return Vec::new();
        };
        if first > last {
            return Vec::new();
        }
        product = product.saturating_mul((last - first + 1) as usize);
        if product > cap {
            return Vec::new();
        }
        boxes.push((first, last));
    }
    let mut point: Vec<i64> = boxes.iter().map(|(first, _)| *first).collect();
    let mut out = Vec::new();
    loop {
        out.push(point.iter().map(|v| Rational::from_i64(*v)).collect());
        let mut carried = false;
        for (value, (first, last)) in point.iter_mut().zip(&boxes) {
            if *value < *last {
                *value += 1;
                carried = true;
                break;
            }
            *value = *first;
        }
        if !carried {
            break;
        }
    }
    out
}

/// Whether a point satisfies every row of a model.
fn feasible(model: &Model, point: &[Rational]) -> bool {
    for index in 0..model.rows().len() {
        let activity = model.row_activity(hc_core::RowId(index as u32), point);
        let row = &model.rows()[index];
        if row
            .lower
            .as_ref()
            .is_some_and(|bound| activity.cmp_value(bound).is_lt())
        {
            return false;
        }
        if row
            .upper
            .as_ref()
            .is_some_and(|bound| activity.cmp_value(bound).is_gt())
        {
            return false;
        }
    }
    true
}

fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME C\nROWS\n N obj\n");
    for row in 0..rows {
        let sense = if generator.next_below(4) == 0 {
            'G'
        } else {
            'L'
        };
        let _ = writeln!(source, " {sense} r{row}");
    }
    source.push_str("COLUMNS\n MARKER 'MARKER' 'INTORG'\n");
    for column in 0..columns {
        let _ = writeln!(
            source,
            " c{column} obj {}",
            generator.next_below(9) as i64 - 4
        );
        for row in 0..rows {
            let value = generator.next_below(7) as i64 - 3;
            if value != 0 {
                let _ = writeln!(source, " c{column} r{row} {value}");
            }
        }
    }
    source.push_str(" MARKER 'MARKER' 'INTEND'\nRHS\n");
    for row in 0..rows {
        let _ = writeln!(
            source,
            " rhs r{row} {}",
            generator.next_below(12) as i64 - 3
        );
    }
    source.push_str("BOUNDS\n");
    for column in 0..columns {
        let _ = writeln!(source, " UI b c{column} {}", 1 + generator.next_below(3));
    }
    source.push_str("ENDATA\n");
    source
}

#[test]
fn no_derived_cut_removes_an_integer_feasible_point() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x0c07_0000_0000_0001_u64);
    let mut derived = 0;
    let mut checked = 0;

    for round in 0..600 {
        let source = generate(&mut generator, 1 + (round % 2), 1 + (round % 3));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");
        let canonical = Canonical::of(&model);
        let points = box_points(&model, 256);
        if points.is_empty() {
            continue;
        }

        // Every single-row combination with a handful of multipliers, and every
        // integer column as the isolated one. Most will refuse; the ones that
        // do not are cuts, and every one of them must hold at every feasible
        // integer point.
        for row in 0..canonical.rows() {
            for (top, bottom) in [(1, 1), (1, 2), (1, 3), (2, 3), (-1, 2), (3, 4)] {
                let combination = vec![(row, ratio(top, bottom))];
                for isolated in 0..canonical.structural() {
                    let Ok(cut) = derive(&canonical, &combination, isolated) else {
                        continue;
                    };
                    let expanded = over_variables(&canonical, &cut);
                    derived += 1;
                    for point in &points {
                        if !feasible(&model, point) {
                            continue;
                        }
                        let mut activity = Rational::zero();
                        for (column, coefficient) in &expanded.coefficients {
                            activity = activity.add(&coefficient.mul(&point[*column]));
                        }
                        checked += 1;
                        assert!(
                            activity.cmp_value(&expanded.right).is_ge(),
                            "a cut cut off a feasible integer point\n{source}\ncut {expanded:?}\npoint {point:?}"
                        );
                    }
                }
            }
        }
    }

    assert!(derived > 100, "only {derived} cuts were derived");
    assert!(checked > 1_000, "only {checked} point checks were made");
}
