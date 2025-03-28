//! Random sparse bases, factored and re-solved, with the drift measured.
//!
//! A factorization is only as good as the residual it leaves behind, and a
//! residual is only meaningful when it is computed against the original
//! columns rather than through the factors that produced it. Every case below
//! does exactly that: factor a basis, solve for a right-hand side built from a
//! known solution, and measure how far the recovered solution's product is
//! from the target.
//!
//! The bases are drawn rather than written, because the shapes that break a
//! factorization are not the shapes anyone writes by hand — a column whose
//! only large entry is eliminated early, a row that fills in from three
//! directions at once, a pivot that was fine until the step before it. The
//! stream is the deterministic generator, so a failure replays exactly.
//!
//! Every basis here is built to be nonsingular by construction, with a
//! diagonal that dominates. That is deliberate: this campaign measures drift
//! on bases a simplex would actually hold, and a singular draw would measure
//! the error path instead.

use hc_core::{Tol, Xoshiro256PlusPlus};
use hc_lp::Factorization;

/// How far a re-solve may land from its target, before scaling by the size of
/// the problem and the size of the numbers in it.
///
/// Machine epsilon times an allowance of two to the twenty-fourth for the
/// conditioning of a drawn basis. Written that way rather than as a round
/// decimal because the quantity being bounded really is accumulated rounding,
/// and a bound stated in units of rounding says what it is measuring.
///
/// A drift bound rather than an equality: the factorization is floating point
/// and always will be, and the honest thing is to state how far it may be
/// wrong rather than to pretend it is exact. Nothing is proved against this
/// number — the exact machinery elsewhere does that — so it is a health
/// measurement and not a threshold any claim rests on.
const DRIFT_BOUND: f64 = f64::EPSILON * 16_777_216.0;

/// Draws a nonsingular sparse basis with a dominant diagonal.
fn draw_basis(
    generator: &mut Xoshiro256PlusPlus,
    size: usize,
    off_diagonal: usize,
) -> Vec<Vec<(u32, f64)>> {
    let mut columns: Vec<Vec<(u32, f64)>> = vec![Vec::new(); size];
    for (index, column) in columns.iter_mut().enumerate() {
        // A diagonal entry large enough that no ordering can make the matrix
        // singular, with a sign drawn so the values are not all positive.
        let magnitude = 1.0 + generator.next_unit_interval() * 4.0;
        let sign = if generator.next_below(2) == 0 {
            1.0
        } else {
            -1.0
        };
        column.push((index as u32, sign * (magnitude + off_diagonal as f64)));
    }
    for _ in 0..off_diagonal * size {
        let row = generator.next_below(size as u64) as u32;
        let column = generator.next_below(size as u64) as usize;
        if row as usize == column {
            continue;
        }
        let value = (generator.next_unit_interval() - 0.5) * 2.0;
        if value == 0.0 {
            continue;
        }
        columns[column].push((row, value));
    }
    for column in &mut columns {
        column.sort_unstable_by_key(|(row, _)| *row);
        column.dedup_by_key(|(row, _)| *row);
    }
    columns
}

fn multiply(columns: &[Vec<(u32, f64)>], vector: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; vector.len()];
    for (position, column) in columns.iter().enumerate() {
        for &(row, value) in column {
            out[row as usize] += value * vector[position];
        }
    }
    out
}

fn transpose_multiply(columns: &[Vec<(u32, f64)>], vector: &[f64]) -> Vec<f64> {
    columns
        .iter()
        .map(|column| {
            column
                .iter()
                .map(|(row, value)| value * vector[*row as usize])
                .sum()
        })
        .collect()
}

/// The largest absolute entry, used to make the drift bound relative.
fn scale_of(values: &[f64]) -> f64 {
    values
        .iter()
        .fold(1.0f64, |largest, value| largest.max(value.abs()))
}

#[test]
fn every_drawn_basis_re_solves_within_the_drift_bound() {
    let tol = Tol::solver();
    let mut generator = Xoshiro256PlusPlus::from_seed(0x5eed_0001);
    let mut factored = 0usize;

    for round in 0..200u32 {
        let size = 3 + (round as usize % 24);
        let density = 1 + (round as usize % 4);
        let columns = draw_basis(&mut generator, size, density);
        let factorization = match Factorization::build(&columns, size, tol) {
            Ok(factorization) => factorization,
            // A dominant diagonal makes this unreachable, and saying so is
            // better than skipping quietly: a campaign that silently drops
            // half its cases reports a pass it did not earn.
            Err(error) => panic!("round {round} drew a basis that would not factor: {error}"),
        };
        factored += 1;

        // A known solution, so the target is exact and the comparison is
        // against something the factorization never saw.
        let wanted: Vec<f64> = (0..size)
            .map(|_| (generator.next_unit_interval() - 0.5) * 20.0)
            .collect();
        let target = multiply(&columns, &wanted);

        let mut solution = target.clone();
        factorization.ftran(&mut solution);
        let residual = Factorization::residual(&columns, &solution, &target);
        let bound = DRIFT_BOUND * scale_of(&target) * size as f64;
        assert!(
            residual <= bound,
            "round {round}: forward residual {residual} over the bound {bound}"
        );

        let dual_target: Vec<f64> = (0..size)
            .map(|_| (generator.next_unit_interval() - 0.5) * 20.0)
            .collect();
        let mut dual = dual_target.clone();
        factorization.btran(&mut dual);
        let recovered = transpose_multiply(&columns, &dual);
        let dual_residual = recovered
            .iter()
            .zip(&dual_target)
            .fold(0.0f64, |largest, (got, want)| {
                largest.max((got - want).abs())
            });
        let dual_bound = DRIFT_BOUND * scale_of(&dual_target) * size as f64;
        assert!(
            dual_residual <= dual_bound,
            "round {round}: transposed residual {dual_residual} over the bound {dual_bound}"
        );
    }

    assert_eq!(
        factored, 200,
        "the campaign factored fewer bases than it drew"
    );
}

#[test]
fn the_two_solves_are_adjoint_to_each_other() {
    // For any pair of vectors, the forward solve of one against the second
    // equals the first against the transposed solve of the second. That is a
    // property of the inverse rather than of either routine, so it catches a
    // permutation applied in one direction and not the other — which is the
    // defect a residual check on its own would miss, because both solves would
    // be individually wrong in matching ways.
    let tol = Tol::solver();
    let mut generator = Xoshiro256PlusPlus::from_seed(0x5eed_0002);

    for round in 0..60u32 {
        let size = 2 + (round as usize % 16);
        let columns = draw_basis(&mut generator, size, 2);
        let factorization = Factorization::build(&columns, size, tol).expect("nonsingular");

        let left: Vec<f64> = (0..size)
            .map(|_| generator.next_unit_interval() - 0.5)
            .collect();
        let right: Vec<f64> = (0..size)
            .map(|_| generator.next_unit_interval() - 0.5)
            .collect();

        let mut forward = left.clone();
        factorization.ftran(&mut forward);
        let one: f64 = forward.iter().zip(&right).map(|(a, b)| a * b).sum();

        let mut backward = right.clone();
        factorization.btran(&mut backward);
        let other: f64 = left.iter().zip(&backward).map(|(a, b)| a * b).sum();

        let bound = DRIFT_BOUND * scale_of(&left) * scale_of(&right) * size as f64;
        assert!(
            (one - other).abs() <= bound,
            "round {round}: the solves are not adjoint, {one} against {other}"
        );
    }
}

#[test]
fn solving_for_each_unit_vector_reproduces_the_inverse() {
    // The columns of the solved identity are the inverse, and multiplying the
    // basis by them has to give the identity back. This is the slowest check
    // available and the least forgiving: it exercises every column of the
    // factorization rather than one random combination of them.
    let tol = Tol::solver();
    let mut generator = Xoshiro256PlusPlus::from_seed(0x5eed_0003);

    for round in 0..25u32 {
        let size = 2 + (round as usize % 10);
        let columns = draw_basis(&mut generator, size, 2);
        let factorization = Factorization::build(&columns, size, tol).expect("nonsingular");

        for position in 0..size {
            let mut unit = vec![0.0; size];
            unit[position] = 1.0;
            let mut solution = unit.clone();
            factorization.ftran(&mut solution);
            let product = multiply(&columns, &solution);
            for (row, value) in product.iter().enumerate() {
                let wanted = if row == position { 1.0 } else { 0.0 };
                assert!(
                    (value - wanted).abs() <= DRIFT_BOUND * size as f64,
                    "round {round}, column {position}, row {row}: {value} against {wanted}"
                );
            }
        }
    }
}

#[test]
fn the_factorization_stays_far_smaller_than_a_dense_one() {
    // Fill-in is the failure mode a pivot order exists to prevent, and it is
    // silent: a factorization that fills in completely is still correct and
    // makes every later solve cost what a dense inverse would. The bound here
    // is loose enough to survive an unlucky draw and tight enough that an
    // ordering which ignored sparsity would fail it.
    let tol = Tol::solver();
    let mut generator = Xoshiro256PlusPlus::from_seed(0x5eed_0004);

    for round in 0..20u32 {
        let size = 40 + round as usize;
        let columns = draw_basis(&mut generator, size, 2);
        let entries: usize = columns.iter().map(Vec::len).sum();
        let factorization = Factorization::build(&columns, size, tol).expect("nonsingular");
        assert!(
            factorization.nonzeros() < entries * 8,
            "round {round}: {} entries grew to {}",
            entries,
            factorization.nonzeros()
        );
        assert!(factorization.smallest_pivot() > tol.piv);
    }
}
