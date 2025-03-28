//! Updated bases against refactored ones, and the drift that makes the
//! difference matter.
//!
//! Two claims are checked here. The first is that an updated basis answers
//! every solve the way a freshly factored one would, over long chains of
//! replacements rather than single ones — a scheme that applies its
//! transformations in the wrong order passes a one-update test and fails
//! everything after it.
//!
//! The second is harder and more important. Updates drift, and the drift is
//! not gradual: a basis can be accurate at one iteration and useless at the
//! next, without any single step doing anything a pivot fence would refuse.
//! The trap below builds exactly that situation and checks that the residual
//! measurement catches it, because nothing else does.

use hc_core::{Tol, Xoshiro256PlusPlus};
use hc_lp::{Inverse, Reason};

fn multiply(columns: &[Vec<(u32, f64)>], vector: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; vector.len()];
    for (position, column) in columns.iter().enumerate() {
        for &(row, value) in column {
            out[row as usize] += value * vector[position];
        }
    }
    out
}

/// The residual of one probe solve, measured against the original columns.
fn probe_residual(inverse: &Inverse, columns: &[Vec<(u32, f64)>], target: &[f64]) -> f64 {
    let mut solution = target.to_vec();
    inverse.ftran(&mut solution);
    let product = multiply(columns, &solution);
    product
        .iter()
        .zip(target)
        .fold(0.0f64, |largest, (got, wanted)| {
            largest.max((got - wanted).abs())
        })
}

fn dense_to_sparse(dense: &[f64]) -> Vec<(u32, f64)> {
    dense
        .iter()
        .enumerate()
        .filter(|(_, value)| **value != 0.0)
        .map(|(row, value)| (row as u32, *value))
        .collect()
}

/// A basis change whose entering column is nearly a copy of another one.
///
/// The entering column is one basis column plus a tiny multiple of the column
/// being replaced. Expressed in the current basis that is a unit vector at the
/// copied position plus the tiny multiple at the replaced one — so the pivot is
/// small by construction, while still sitting far above the fence that would
/// refuse a pivot outright. The rest of the basis is untouched, which is what
/// keeps the matrix well enough conditioned that factoring it again is a
/// remedy rather than a second refusal.
fn nearly_a_copy(
    columns: &[Vec<(u32, f64)>],
    position: usize,
    copied: usize,
    share: f64,
) -> Vec<f64> {
    let size = columns.len();
    let mut dense = vec![0.0; size];
    for &(row, value) in &columns[copied] {
        dense[row as usize] += value;
    }
    for &(row, value) in &columns[position] {
        dense[row as usize] += share * value;
    }
    dense
}

#[test]
fn trap_lu_drift_refactor() {
    let tol = Tol::solver();
    let size = 8usize;
    // A hundred times the pivot fence: every update below is accepted with
    // room to spare, so a solver that watched only its pivots would see
    // nothing wrong at any point in the chain.
    let share = tol.piv * 100.0;
    let target: Vec<f64> = (0..size).map(|index| index as f64 + 1.0).collect();

    let mut columns: Vec<Vec<(u32, f64)>> = (0..size)
        .map(|index| vec![(index as u32, 1.0 + index as f64)])
        .collect();
    let mut inverse = Inverse::factor(&columns, size, tol).expect("nonsingular");

    let mut caught_at: Option<usize> = None;
    let mut worst = 0.0f64;

    for step in 0..12usize {
        let position = step % size;
        let copied = (position + 1) % size;
        let dense = nearly_a_copy(&columns, position, copied, share);
        let mut alpha = dense.clone();
        inverse.ftran(&mut alpha);

        // The pivot clears the fence by two orders of magnitude. Nothing about
        // this step looks wrong from inside the iteration.
        assert!(
            alpha[position].abs() > tol.piv * 10.0,
            "step {step}: the trap needs an update nothing would question, and this pivot was marginal"
        );
        inverse
            .update(position, &alpha, tol)
            .expect("an accepted update");
        columns[position] = dense_to_sparse(&dense);

        let residual = probe_residual(&inverse, &columns, &target);
        worst = worst.max(residual);
        if residual > tol.feas * 0.1 {
            assert_eq!(
                inverse.should_refactor(residual, tol),
                Some(Reason::Drift),
                "step {step}: a residual of {residual} did not trigger a refactorization"
            );
            caught_at = Some(step);
            break;
        }
        assert_eq!(
            inverse.should_refactor(residual, tol),
            None,
            "step {step}: a residual of {residual} triggered a refactorization it should not have"
        );
    }

    // The trap has to actually spring, or it is a test that passes by never
    // reaching its own subject.
    let caught = caught_at.expect("the chain never drifted, so the trap checked nothing");
    assert!(
        worst > tol.feas,
        "the chain drifted by only {worst}, which is inside the fence and proves nothing"
    );

    // Factoring again from the same columns restores what the updates lost.
    // This is the half that makes the trigger worth having: a detector with no
    // remedy is an alarm. The columns are unchanged — the difference is
    // entirely in how the inverse was arrived at.
    let refactored = Inverse::factor(&columns, size, tol)
        .expect("the basis is well conditioned; only the update file was not");
    let after = probe_residual(&refactored, &columns, &target);
    assert!(
        after <= tol.feas * 0.1,
        "refactoring at step {caught} left a residual of {after}"
    );
    assert_eq!(refactored.updates(), 0);
}

#[test]
fn the_pivot_fence_alone_would_not_have_caught_it() {
    // Stated as its own check because it is the reason a residual is measured
    // at all. Every update in the trap above clears the pivot fence by two
    // orders of magnitude, so a solver watching only its pivots carries on
    // through the point where its answers stop meaning anything.
    let tol = Tol::solver();
    let size = 6usize;
    let share = tol.piv * 100.0;
    let mut columns: Vec<Vec<(u32, f64)>> = (0..size)
        .map(|index| vec![(index as u32, 1.0 + index as f64)])
        .collect();
    let mut inverse = Inverse::factor(&columns, size, tol).expect("nonsingular");

    let mut accepted = 0usize;
    for step in 0..3usize {
        let position = step % size;
        let copied = (position + 1) % size;
        let dense = nearly_a_copy(&columns, position, copied, share);
        let mut alpha = dense.clone();
        inverse.ftran(&mut alpha);
        assert!(
            alpha[position].abs() > tol.piv * 10.0,
            "step {step}: pivot was marginal"
        );
        inverse.update(position, &alpha, tol).expect("accepted");
        accepted += 1;
        columns[position] = dense_to_sparse(&dense);
    }
    assert_eq!(
        accepted, 3,
        "every update in the chain clears the pivot fence"
    );
}

#[test]
fn a_random_chain_of_updates_matches_refactoring_at_every_step() {
    let tol = Tol::solver();
    let mut generator = Xoshiro256PlusPlus::from_seed(0x0ba5_0001);
    let mut chains = 0usize;

    for round in 0..40u32 {
        let size = 3 + (round as usize % 9);
        let mut columns: Vec<Vec<(u32, f64)>> = (0..size)
            .map(|index| {
                let magnitude = 1.0 + generator.next_unit_interval() * 3.0;
                vec![(index as u32, magnitude + size as f64)]
            })
            .collect();
        // A little off-diagonal structure, so the bases are not trivially
        // triangular and the pivot order has something to decide.
        for _ in 0..size {
            let row = generator.next_below(size as u64) as u32;
            let column = generator.next_below(size as u64) as usize;
            if row as usize != column {
                columns[column].push((row, generator.next_unit_interval() - 0.5));
                columns[column].sort_unstable_by_key(|(index, _)| *index);
                columns[column].dedup_by_key(|(index, _)| *index);
            }
        }

        let mut inverse = Inverse::factor(&columns, size, tol).expect("nonsingular");

        for step in 0..12usize {
            let position = generator.next_below(size as u64) as usize;
            let dense: Vec<f64> = (0..size)
                .map(|_| {
                    if generator.next_below(3) == 0 {
                        0.0
                    } else {
                        (generator.next_unit_interval() - 0.5) * 4.0
                    }
                })
                .collect();

            let mut alpha = dense.clone();
            inverse.ftran(&mut alpha);
            if inverse.update(position, &alpha, tol).is_err() {
                // The draw would have made the basis singular. Refusing is the
                // right answer and the chain simply moves on.
                continue;
            }
            columns[position] = dense_to_sparse(&dense);
            chains += 1;

            let Ok(fresh) = Inverse::factor(&columns, size, tol) else {
                // The updated basis is usable and a fresh factorization of the
                // same columns is not, which happens near singularity. There
                // is nothing to compare against, so the chain restarts.
                break;
            };

            for probe in 0..size {
                let mut unit = vec![0.0; size];
                unit[probe] = 1.0;
                let mut updated = unit.clone();
                inverse.ftran(&mut updated);
                let mut refactored = unit.clone();
                fresh.ftran(&mut refactored);
                let gap = updated
                    .iter()
                    .zip(&refactored)
                    .fold(0.0f64, |largest, (a, b)| largest.max((a - b).abs()));
                let scale = refactored
                    .iter()
                    .fold(1.0f64, |largest, v| largest.max(v.abs()));
                assert!(
                    gap <= tol.feas * scale,
                    "round {round}, step {step}, probe {probe}: forward solves differ by {gap}"
                );

                let mut updated_dual = unit.clone();
                inverse.btran(&mut updated_dual);
                let mut refactored_dual = unit;
                fresh.btran(&mut refactored_dual);
                let dual_gap = updated_dual
                    .iter()
                    .zip(&refactored_dual)
                    .fold(0.0f64, |largest, (a, b)| largest.max((a - b).abs()));
                let dual_scale = refactored_dual
                    .iter()
                    .fold(1.0f64, |largest, v| largest.max(v.abs()));
                assert!(
                    dual_gap <= tol.feas * dual_scale,
                    "round {round}, step {step}, probe {probe}: transposed solves differ by {dual_gap}"
                );
            }
        }
    }

    assert!(chains > 200, "the campaign only made {chains} updates");
}
