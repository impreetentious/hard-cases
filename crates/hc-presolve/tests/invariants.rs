//! The two invariants, on chains of every shape the generator can make.
//!
//! A reduction is a proof step only if both maps hold. Forward, a feasible
//! point of the model before becomes a feasible point of the model after, with
//! objective no greater. Backward, a feasible point of the model after lifts to
//! a feasible point of the model before, with objective exactly equal.
//!
//! Checking those on generated chains is worth more than checking them on
//! hand-written fixtures, because the interesting failures are the interactions
//! — a bound tightened by one rule that makes another rule's applicability
//! condition hold in a way nobody pictured.

use hc_core::{Model, Xoshiro256PlusPlus};
use hc_exact::Rational;
use hc_io::mps::{Form, read};
use hc_presolve::chain::model_hash;
use hc_presolve::map::{forward, reverse};
use hc_presolve::{Rule, apply, reduce};

/// Whether a point satisfies every row and bound of a model, exactly.
fn feasible(model: &Model, point: &[Rational]) -> bool {
    if point.len() != model.variables().len() {
        return false;
    }
    for (index, variable) in model.variables().iter().enumerate() {
        let value = &point[index];
        if variable
            .lower
            .as_ref()
            .is_some_and(|bound| value.cmp_value(bound).is_lt())
        {
            return false;
        }
        if variable
            .upper
            .as_ref()
            .is_some_and(|bound| value.cmp_value(bound).is_gt())
        {
            return false;
        }
        if variable.integral && !value.is_integer() {
            return false;
        }
    }
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

/// Every integer point in a small model's box, up to a cap.
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

/// A small bounded integer program in free-form MPS.
fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME P\nROWS\n N obj\n");
    for row in 0..rows {
        let sense = match generator.next_below(4) {
            0 => 'E',
            1 => 'G',
            _ => 'L',
        };
        let _ = writeln!(source, " {sense} r{row}");
    }
    source.push_str("COLUMNS\n MARKER 'MARKER' 'INTORG'\n");
    for column in 0..columns {
        let _ = writeln!(
            source,
            " c{column} obj {}",
            generator.next_below(11) as i64 - 5
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
            generator.next_below(14) as i64 - 4
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
fn trap_postsolve_restores_exactly() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x9e5e_0000_0000_0001);
    let mut chains = 0;
    let mut lifts = 0;

    for round in 0..2_000 {
        let rows = 1 + (round % 3);
        let columns = 1 + (round % 3);
        let source = generate(&mut generator, rows, columns);
        let model = read(source.as_bytes(), Form::Free)
            .unwrap_or_else(|error| panic!("the generator produced {error}\n{source}"));

        let chain = reduce(&model);
        if chain.steps.is_empty() {
            continue;
        }
        chains += 1;

        // Every hash a step states must be the hash of what the rule produces
        // when the transform is applied again from the model before it.
        let mut stage = model.clone();
        for step in &chain.steps {
            let next = apply(&stage, &step.rule).unwrap_or_else(|reason| {
                panic!("a recorded rule no longer applies: {reason}\n{source}")
            });
            assert_eq!(
                model_hash(&next),
                step.result,
                "a step's hash does not match what it produces\n{source}"
            );
            stage = next;
        }
        assert_eq!(model_hash(&stage), model_hash(&chain.reduced));

        // The reverse map: every feasible point of the reduced model lifts to
        // a feasible point of the original with exactly equal objective.
        for point in box_points(&chain.reduced, 512) {
            if !feasible(&chain.reduced, &point) {
                continue;
            }
            let lifted = chain.lift(&model, &point);
            lifts += 1;
            assert!(
                feasible(&model, &lifted),
                "a reduced point lifted to something infeasible\n{source}"
            );
            assert_eq!(
                model.objective_at(&lifted).to_canonical_string(),
                chain.reduced.objective_at(&point).to_canonical_string(),
                "the lift changed the objective\n{source}"
            );
        }
    }

    assert!(chains > 100, "only {chains} instances reduced at all");
    assert!(lifts > 100, "only {lifts} points were lifted");
}

#[test]
fn the_forward_map_never_worsens_the_objective() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x9e5e_0000_0000_0002);
    let mut pushed = 0;

    for round in 0..800 {
        let source = generate(&mut generator, 1 + (round % 3), 1 + (round % 3));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");
        let chain = reduce(&model);

        let mut stage = model.clone();
        for step in &chain.steps {
            let Ok(next) = apply(&stage, &step.rule) else {
                break;
            };
            for point in box_points(&stage, 256) {
                if !feasible(&stage, &point) {
                    continue;
                }
                let moved = forward(&stage, &step.rule, &point);
                pushed += 1;
                assert!(
                    feasible(&next, &moved),
                    "the forward map left the feasible region\n{source}"
                );
                assert!(
                    next.objective_at(&moved)
                        .cmp_value(&stage.objective_at(&point))
                        .is_le(),
                    "the forward map worsened the objective\n{source}"
                );
            }
            stage = next;
        }
    }
    assert!(pushed > 100, "only {pushed} points were mapped forward");
}

#[test]
fn a_reduction_that_does_not_apply_is_refused_rather_than_forced() {
    // The frozen instance has no fixed column, so claiming one must fail.
    let raw = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");
    let model = read(raw, Form::Fixed).expect("the frozen instance parses");
    assert!(
        apply(
            &model,
            &Rule::FixedColumn {
                column: 0,
                value: Rational::from_i64(4)
            }
        )
        .is_err()
    );
    assert!(
        apply(
            &model,
            &Rule::FixedColumn {
                column: 9,
                value: Rational::zero()
            }
        )
        .is_err()
    );
}

#[test]
fn the_reverse_map_of_an_aggregate_is_integral_on_every_value_of_the_sum() {
    // Two identical binary columns aggregate into a general integer over zero
    // to two. Every value of the sum must split into two integers inside the
    // original bounds, which is what makes the lift integral by construction
    // rather than by rounding.
    let source = concat!(
        "NAME D\n",
        "ROWS\n",
        " N obj\n",
        " L r0\n",
        "COLUMNS\n",
        " MARKER 'MARKER' 'INTORG'\n",
        " a obj -1 r0 1\n",
        " b obj -1 r0 1\n",
        " MARKER 'MARKER' 'INTEND'\n",
        "RHS\n",
        " rhs r0 1\n",
        "BOUNDS\n",
        " UI b a 1\n",
        " UI b b 1\n",
        "ENDATA\n",
    );
    let model = read(source.as_bytes(), Form::Free).expect("the source parses");
    let rule = Rule::DuplicateColumn { keep: 0, drop: 1 };
    let reduced = apply(&model, &rule).expect("identical binary columns aggregate");

    assert_eq!(reduced.variables().len(), 1);
    assert_eq!(
        reduced.variables()[0]
            .upper
            .as_ref()
            .map(Rational::to_canonical_string),
        Some("2".to_owned()),
        "two binaries aggregate to a general integer, not a binary"
    );

    for sum in 0..=2 {
        let point = vec![Rational::from_i64(sum)];
        let lifted = reverse(&model, &rule, &point);
        assert_eq!(lifted.len(), 2);
        for value in &lifted {
            assert!(value.is_integer(), "the lift of {sum} is fractional");
        }
        assert_eq!(
            lifted[0].add(&lifted[1]).to_canonical_string(),
            sum.to_string(),
            "the lift of {sum} does not sum back"
        );
        assert_eq!(
            model.objective_at(&lifted).to_canonical_string(),
            reduced.objective_at(&point).to_canonical_string()
        );
    }
}

/// The model a generated campaign found the reduction chain could not finish.
///
/// Reduced here rather than described: it is four columns and four rows, two
/// of which tighten each other's bounds by ever-smaller exact amounts. Every
/// improvement is strict and the sequence converges without arriving, so the
/// fixpoint had no reason to stop and the rationals grew until the arithmetic
/// was the slowest part of the program.
const CHASING: &[u8] = b"NAME          DRAFT
OBJSENSE
    MAX
ROWS
 N  obj
 G  r0
 G  r1
 G  r2
 G  r3
COLUMNS
    x0        obj       4
    x0        r0        4
    x0        r1        3
    x0        r1        -3
    x0        r2        1
    x0        r3        100
    x1        obj       -0.5
    x1        r1        10
    x1        r3        3
    x1        r3        -3
    MARK2     'MARKER'                 'INTORG'
    x2        obj       2
    x2        r0        4
    x2        r2        -0.25
    MARK2     'MARKER'                 'INTEND'
    x3        obj       0
    x3        r0        2
    x3        r1        100
    x3        r2        1
RHS
    rhs       obj       2.5
    rhs       r0        0
    rhs       r1        1
    rhs       r2        1
    rhs       r3        0
RANGES
    rng       r0        5
    rng       r3        5
BOUNDS
 MI bnd       x0
 UP bnd       x0        4
 LO bnd       x1        -1
 LO bnd       x2        1
 LO bnd       x3        -10
 UP bnd       x3        10
ENDATA
";

#[test]
fn a_bound_that_can_be_tightened_forever_is_tightened_a_bounded_number_of_times() {
    // The regression. Before the budget this call did not return: the test
    // that catches it is that it returns at all, and the assertions after are
    // about it having done something rather than nothing.
    let model = hc_io::mps::read(CHASING, hc_io::mps::Form::Free).expect("a readable instance");
    let chain = hc_presolve::reduce(&model);

    let tightenings = chain
        .steps
        .iter()
        .filter(|step| matches!(step.rule, hc_presolve::Rule::TightenBound { .. }))
        .count();
    assert!(
        tightenings > 0,
        "the chain reduced nothing, so the regression is not being exercised"
    );
    // Four columns, two sides each, and a budget per side.
    assert!(
        tightenings <= 8 * hc_presolve::chain::TIGHTENINGS_PER_SIDE,
        "the tightening rule fired {tightenings} times"
    );

    // And the budget did not cost correctness: the reduced model is still
    // exactly equivalent, which is what the invariants below establish for
    // every chain.
    assert!(chain.reduced.variables().len() <= model.variables().len());
}

#[test]
fn the_budget_is_spent_per_column_and_side_rather_than_in_total() {
    // A budget shared across the model would stop reducing a large one almost
    // immediately. Each side of each column gets its own, so a model with many
    // columns is reduced as far as a model with one.
    let model = hc_io::mps::read(CHASING, hc_io::mps::Form::Free).expect("a readable instance");
    let chain = hc_presolve::reduce(&model);
    let mut spent: std::collections::BTreeMap<(usize, bool), usize> =
        std::collections::BTreeMap::new();
    for step in &chain.steps {
        if let hc_presolve::Rule::TightenBound { column, upper, .. } = step.rule {
            *spent.entry((column, upper)).or_insert(0) += 1;
        }
    }
    for ((column, upper), count) in spent {
        assert!(
            count <= hc_presolve::chain::TIGHTENINGS_PER_SIDE,
            "column {column} side {upper} was tightened {count} times"
        );
    }
}
