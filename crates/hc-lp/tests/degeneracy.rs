//! Degeneracy: the vertex a simplex can circle forever, and the ladder that
//! gets it out.
//!
//! A degenerate step changes the basis and moves nothing. That is normal and
//! frequent. What is not normal is a sequence of them that returns to a basis
//! it has already held, at which point the search will repeat that sequence
//! for as long as it is allowed to run. Every rule that avoids this trades
//! something away, so the response here is graded rather than uniform: try the
//! arithmetic again, then break the ties, and only then fall back to the rule
//! that provably cannot cycle and is slow enough that nobody wants it by
//! default.
//!
//! The second trap in this file is about honesty rather than termination.
//! Perturbation invents bounds. An inference that cited one would be an
//! inference about a model nobody wrote, and it would look completely normal.

use hc_core::Tol;
use hc_io::mps::{Form, read};
use hc_lp::{Options, Outcome, Problem, Rule, Working, solve};

/// Beale's example, the standard demonstration that a simplex can cycle.
///
/// With textbook pricing and the lowest-index leaving rule this returns to its
/// starting basis after six pivots, none of which moves anything, and does so
/// again forever. It is written here in the same form it is always written in,
/// with explicit slack columns, so that it is recognisable.
const BEALE: &str = concat!(
    "ROWS\n N obj\n E r1\n E r2\n E r3\n",
    "COLUMNS\n",
    " x4 obj -0.75 r1 0.25\n",
    " x4 r2 0.5\n",
    " x5 obj 150 r1 -60\n",
    " x5 r2 -90\n",
    " x6 obj -0.02 r1 -0.04\n",
    " x6 r2 -0.02 r3 1\n",
    " x7 obj 6 r1 9\n",
    " x7 r2 3\n",
    " s1 r1 1\n",
    " s2 r2 1\n",
    " s3 r3 1\n",
    "RHS\n rhs r3 1\n",
    "ENDATA\n"
);

fn problem(source: &str) -> Problem {
    let model = read(source.as_bytes(), Form::Free).expect("accepted");
    Problem::unscaled(&Working::from_model(&model))
}

fn optimum(outcome: &Outcome) -> (f64, Vec<f64>) {
    match outcome {
        Outcome::Optimal {
            objective, point, ..
        } => (*objective, point.clone()),
        other => panic!("expected an optimum, got {}", other.as_str()),
    }
}

/// A fence a notch inside the search's own.
fn fence() -> f64 {
    Tol::solver().feas / 10.0
}

#[test]
fn trap_degenerate_cycling() {
    let problem = problem(BEALE);
    // Every rule must terminate on it, including the textbook one that is the
    // reason the example exists. A generous but finite budget is the point: a
    // solver that cycles exhausts it, and one that escalates does not.
    let budget = 5_000u64;
    let mut objectives = Vec::new();
    for rule in [Rule::Devex, Rule::Dantzig, Rule::Bland] {
        let options = Options {
            rule,
            iteration_limit: budget,
            ..Options::default()
        };
        let (outcome, effort) = solve(&problem, &options);
        assert!(
            effort.iterations < budget,
            "{} exhausted the budget, which is what cycling looks like",
            rule.as_str()
        );
        let (objective, point) = optimum(&outcome);
        let residual = problem.residual_at(&point);
        assert!(
            residual.iter().all(|value| value.abs() <= fence()),
            "{}: the answer does not satisfy the system: {residual:?}",
            rule.as_str()
        );
        objectives.push((rule, objective));
    }
    for pair in objectives.windows(2) {
        assert!(
            (pair[0].1 - pair[1].1).abs() < fence(),
            "the rules disagreed about the optimum: {objectives:?}"
        );
    }
    // The known optimum of this example.
    assert!(
        (objectives[0].1 - -0.05).abs() < fence(),
        "the optimum was {}",
        objectives[0].1
    );
}

#[test]
fn trap_perturbation_removed() {
    // Perturbation widens bounds by roughly ten feasibility fences. If a
    // widened bound survived into the answer, the answer would sit outside the
    // model's real bounds by far more than the fence, and its objective would
    // be wrong by a similar margin.
    //
    // The comparison is against the rule that never perturbs, from a cold
    // start, so the two paths through the search have to agree about a number
    // that a leaked perturbation would move.
    let problem = problem(BEALE);

    let plain = Options {
        rule: Rule::Bland,
        iteration_limit: 5_000,
        ..Options::default()
    };
    let (reference, _) = solve(&problem, &plain);
    let (reference_objective, _) = optimum(&reference);

    // Several seeds, because a perturbation that leaked would leak by a
    // different amount under each one.
    for seed in [0u64, 1, 7, 0x0123_4567_89ab_cdef] {
        let options = Options {
            seed,
            iteration_limit: 5_000,
            ..Options::default()
        };
        let (outcome, _) = solve(&problem, &options);
        let (objective, point) = optimum(&outcome);
        assert!(
            (objective - reference_objective).abs() < fence(),
            "seed {seed}: objective {objective} against {reference_objective}"
        );
        for (index, value) in point.iter().enumerate() {
            assert!(
                *value >= problem.lower(index) - fence(),
                "seed {seed}: column {index} is below a bound nobody widened"
            );
            assert!(
                *value <= problem.upper(index) + fence(),
                "seed {seed}: column {index} is above a bound nobody widened"
            );
        }
    }
}

#[test]
fn the_seed_changes_nothing_when_nothing_stalls() {
    // A model that never reaches the ladder must be identical under every
    // seed, because a perturbation that was drawn but never applied would
    // still show up here.
    let problem = problem(concat!(
        "OBJSENSE MAX\nROWS\n N obj\n L c1\n L c2\n",
        "COLUMNS\n x obj 3 c1 1\n x c2 1\n y obj 2 c1 1\n y c2 3\n",
        "RHS\n r c1 4 c2 6\n",
        "ENDATA\n"
    ));
    let (first, first_effort) = solve(
        &problem,
        &Options {
            seed: 0,
            ..Options::default()
        },
    );
    let (second, second_effort) = solve(
        &problem,
        &Options {
            seed: 99,
            ..Options::default()
        },
    );
    assert_eq!(first_effort, second_effort);
    assert_eq!(optimum(&first), optimum(&second));
    assert_eq!(
        first_effort.escalations, 0,
        "this model should not reach the ladder"
    );
}

#[test]
fn a_degenerate_vertex_is_solved_and_counted() {
    // Three constraints meeting at one point. The step into that vertex is
    // real; the steps around it are not, and they are counted so that a stall
    // is visible rather than inferred.
    let problem = problem(concat!(
        "ROWS\n N obj\n L c1\n L c2\n L c3\n",
        "COLUMNS\n x obj -1 c1 1\n x c2 1\n x c3 2\n y obj -1 c1 1\n y c2 2\n y c3 1\n",
        "RHS\n r c1 4 c2 6\n r c3 6\n",
        "ENDATA\n"
    ));
    let (outcome, effort) = solve(&problem, &Options::default());
    let (objective, point) = optimum(&outcome);
    assert!(
        (objective - -4.0).abs() < fence(),
        "the optimum was {objective}"
    );
    assert!(
        problem
            .residual_at(&point)
            .iter()
            .all(|value| value.abs() <= fence())
    );
    assert!(effort.iterations >= 2);
}

#[test]
fn the_solve_is_reproducible_under_every_rule_and_seed() {
    // Determinism is a property of the whole search, escalation included: two
    // runs of the same configuration must spend the same effort and reach the
    // same numbers, or a failure cannot be replayed to be minimized.
    let problem = problem(BEALE);
    for rule in [Rule::Devex, Rule::Dantzig, Rule::Bland] {
        for seed in [0u64, 42] {
            let options = Options {
                rule,
                seed,
                iteration_limit: 5_000,
                ..Options::default()
            };
            let (first, first_effort) = solve(&problem, &options);
            let (second, second_effort) = solve(&problem, &options);
            assert_eq!(first_effort, second_effort, "{} seed {seed}", rule.as_str());
            assert_eq!(
                optimum(&first),
                optimum(&second),
                "{} seed {seed}",
                rule.as_str()
            );
        }
    }
}
