//! Node selection, plunging, and the bound that may not regress.
//!
//! What the search chooses to open next affects how quickly it finishes and
//! must not affect what it concludes. These check both halves: that the choices
//! are the ones described, and that changing them changes nothing about the
//! answer.

use hc_core::Xoshiro256PlusPlus;
use hc_io::mps::{Form, read};
use hc_mip::search::{Ending, Node, Options, run};

const KNAPSACK: &str = concat!(
    "NAME K\n",
    "ROWS\n",
    " N obj\n",
    " L c1\n",
    "COLUMNS\n",
    " MARKER 'MARKER' 'INTORG'\n",
    " x obj -7 c1 5\n",
    " y obj -9 c1 7\n",
    " z obj -5 c1 4\n",
    " MARKER 'MARKER' 'INTEND'\n",
    "RHS\n",
    " rhs c1 13\n",
    "BOUNDS\n",
    " UI b x 3\n",
    " UI b y 3\n",
    " UI b z 3\n",
    "ENDATA\n",
);

fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME N\nROWS\n N obj\n");
    for row in 0..rows {
        let sense = if generator.next_below(3) == 0 {
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
            generator.next_below(15) as i64 - 7
        );
        for row in 0..rows {
            let value = generator.next_below(9) as i64 - 4;
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
            generator.next_below(18) as i64 - 6
        );
    }
    source.push_str("BOUNDS\n");
    for column in 0..columns {
        let _ = writeln!(source, " UI b c{column} {}", 1 + generator.next_below(4));
    }
    source.push_str("ENDATA\n");
    source
}

#[test]
fn trap_node_bound_monotone() {
    // A child's region sits inside its parent's, so its bound cannot be
    // weaker. A node whose bound regressed is recorded as suspect and left
    // open; it is never used to close anything.
    let mut generator = Xoshiro256PlusPlus::from_seed(0x0d0e_0000_0000_0001_u64);
    let mut suspects = 0;
    let mut trees = 0;

    for round in 0..800 {
        let source = generate(&mut generator, 1 + (round % 3), 1 + (round % 4));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");
        let search = run(&model, &Options::default());
        if search.tree.len() > 1 {
            trees += 1;
        }
        for node in search.nodes.iter().flatten() {
            if matches!(node, Node::Suspect { .. }) {
                suspects += 1;
            }
        }
        // Whatever else happened, a suspect node keeps the search from
        // claiming optimality.
        if suspects > 0 {
            assert_ne!(search.ending, Ending::Optimal);
        }
    }
    assert!(trees > 100, "only {trees} instances branched");
    assert_eq!(
        suspects, 0,
        "exact bounds regressed on {suspects} node(s), which cannot happen and did"
    );
}

#[test]
fn plunging_finds_an_incumbent_and_the_answer_does_not_depend_on_it() {
    let model = read(KNAPSACK.as_bytes(), Form::Free).expect("the source parses");

    let diving = run(&model, &Options::default());
    let flat = run(
        &model,
        &Options {
            plunge_depth: 0,
            ..Options::default()
        },
    );

    assert_eq!(diving.ending, Ending::Optimal);
    assert_eq!(flat.ending, Ending::Optimal);
    assert_eq!(
        diving
            .incumbent
            .as_ref()
            .map(|found| found.objective.to_canonical_string()),
        flat.incumbent
            .as_ref()
            .map(|found| found.objective.to_canonical_string()),
        "the dive changed the answer"
    );
}

#[test]
fn a_small_basis_cache_changes_the_effort_and_not_the_answer() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x0d0e_0000_0000_0002_u64);
    for round in 0..200 {
        let source = generate(&mut generator, 1 + (round % 3), 1 + (round % 3));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");

        let roomy = run(&model, &Options::default());
        let cramped = run(
            &model,
            &Options {
                basis_cache: 1,
                ..Options::default()
            },
        );

        assert_eq!(
            roomy.ending, cramped.ending,
            "round {round}: the cache size changed the status\n{source}"
        );
        assert_eq!(
            roomy
                .incumbent
                .as_ref()
                .map(|found| found.objective.to_canonical_string()),
            cramped
                .incumbent
                .as_ref()
                .map(|found| found.objective.to_canonical_string()),
            "round {round}: the cache size changed the answer\n{source}"
        );
    }
}

#[test]
fn best_bound_selection_is_reproducible() {
    let model = read(KNAPSACK.as_bytes(), Form::Free).expect("the source parses");
    let first = run(&model, &Options::default());
    let second = run(&model, &Options::default());
    assert_eq!(first.tree, second.tree);
    assert_eq!(first.opened, second.opened);
    assert_eq!(first.iterations, second.iterations);
}
