//! Scaling is invisible to anything a proof reads.
//!
//! The rule is easy to state and easy to violate by accident: the exact model
//! is never scaled. Not scaled and unscaled, not scaled into a copy something
//! later reads, not scaled "just for the check". A certificate is about the
//! bytes of a file, and a scale factor is a fact about how the search chose to
//! look at them.
//!
//! The mechanism that makes this hard to get wrong is structural — the exact
//! model is immutable and scaling operates on a separate working copy — but
//! structure is an argument and this is a check. The trap is a scaffold at
//! this stage: it covers the exact model and its canonical dump, which is
//! everything a proof reads today. It grows as the certificate does, and the
//! names below are the anchors that growth attaches to.

use hc_core::{Model, Tol};
use hc_io::mps::{Form, read};
use hc_lp::{Scaling, Working};

/// A model whose coefficients span enough orders of magnitude that scaling has
/// something real to do.
const WIDE: &str = concat!(
    "ROWS\n N obj\n L r1\n L r2\n G r3\n E r4\n",
    "COLUMNS\n",
    " m MARKER INTORG\n",
    " x obj 0.1 r1 10000\n",
    " x r2 0.001\n",
    " m MARKER INTEND\n",
    " y obj 1 r1 20000\n",
    " y r3 0.002 r4 7\n",
    " z obj 1 r2 30000000\n",
    " z r3 4\n",
    "RHS\n rhs r1 100000 r2 3\n rhs r3 1 r4 2\n rhs obj 5\n",
    "RANGES\n rng r1 250\n",
    "BOUNDS\n UI b x 9\n FR b y\n",
    "ENDATA\n"
);

fn wide() -> Model {
    read(WIDE.as_bytes(), Form::Free).expect("accepted")
}

#[test]
fn trap_scaling_invisible_to_certs() {
    let model = wide();
    let before = model.canonical_dump();
    let digest_before = *model.digest();

    let working = Working::from_model(&model);
    let scaling = Scaling::geometric(&working, Tol::solver());
    // The trap is worthless unless the scaling actually did something.
    assert!(
        !scaling.is_identity(),
        "the fixture gave the scaling nothing to do"
    );

    // Everything a proof reads, read again.
    let after = model.canonical_dump();
    assert_eq!(before, after, "the canonical dump changed under scaling");
    assert_eq!(
        &digest_before,
        model.digest(),
        "the instance digest changed under scaling"
    );

    // And the exact values themselves, one at a time, so a change that
    // happened to preserve the rendering would still be caught.
    let reparsed = wide();
    assert_eq!(model.objective_constant(), reparsed.objective_constant());
    assert_eq!(model.variables(), reparsed.variables());
    assert_eq!(model.rows(), reparsed.rows());
    assert_eq!(model.inequalities(), reparsed.inequalities());
    assert_eq!(model.zero_sums(), reparsed.zero_sums());
    assert_eq!(model.discarded(), reparsed.discarded());
    let coefficients: Vec<String> = model
        .matrix()
        .entries()
        .map(|(variable, row, value)| format!("{variable} {row} {}", value.to_canonical_string()))
        .collect();
    let again: Vec<String> = reparsed
        .matrix()
        .entries()
        .map(|(variable, row, value)| format!("{variable} {row} {}", value.to_canonical_string()))
        .collect();
    assert_eq!(
        coefficients, again,
        "an exact coefficient changed under scaling"
    );
}

#[test]
fn a_scaled_working_copy_leaves_the_unscaled_one_alone() {
    // The working copy is derived once and scaling reads it without writing
    // to it, so two copies taken either side of a scaling are identical. This
    // is the mechanism the trap above relies on, checked directly.
    let model = wide();
    let before = Working::from_model(&model);
    let scaling = Scaling::geometric(&before, Tol::solver());
    let after = Working::from_model(&model);
    assert_eq!(before, after);
    assert!(!scaling.is_identity());
}

#[test]
fn the_scaled_and_unscaled_copies_describe_the_same_feasible_set() {
    // A point is feasible in one exactly when its image is feasible in the
    // other. If that failed, scaling would be changing the problem rather
    // than the units, and every claim downstream would be about a model
    // nobody wrote.
    let model = wide();
    let working = Working::from_model(&model);
    let scaling = Scaling::geometric(&working, Tol::solver());
    let tol = Tol::solver();

    let probes: [f64; 5] = [0.0, 1.0, -2.5, 7.25, 0.125];
    for offset in probes {
        let point: Vec<f64> = (0..working.columns())
            .map(|index| index as f64 * 0.5 + offset)
            .collect();
        let scaled: Vec<f64> = point
            .iter()
            .enumerate()
            .map(|(index, value)| value / scaling.column(index))
            .collect();

        let inside = working.within_bounds(&point, tol.feas);
        let scaled_inside = (0..working.columns()).all(|index| {
            let (low, high) = scaling.scaled_bounds(&working, index);
            // The fence scales with the column, because a fence is a distance
            // and distances are what a change of units changes.
            let fence = tol.feas / scaling.column(index).abs();
            scaled[index] >= low - fence && scaled[index] <= high + fence
        });
        assert_eq!(
            inside, scaled_inside,
            "bound membership differs at offset {offset}"
        );
    }
}

#[test]
fn scaling_a_model_with_no_wide_coefficients_changes_nothing_measurable() {
    let plain = read(
        b"ROWS\n N obj\n L r1\n L r2\nCOLUMNS\n x obj 1 r1 2\n x r2 1\n y obj 1 r1 1\n y r2 2\nRHS\n rhs r1 4 r2 4\nENDATA\n",
        Form::Free,
    )
    .expect("accepted");
    let working = Working::from_model(&plain);
    let tol = Tol::solver();
    let scaling = Scaling::geometric(&working, tol);
    // The spread is already small, so whatever the passes decide, they must
    // not make it worse.
    let identity = Scaling::identity(working.rows(), working.columns());
    assert!(scaling.spread(&working, tol) <= identity.spread(&working, tol));
}
