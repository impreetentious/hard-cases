//! Cuts must not change what is true.
//!
//! A cut removes no integer-feasible point, so a search with cuts and the same
//! search without them must reach the same status and the same objective. That
//! is the property the whole validity discipline exists to have, and it is
//! checked here on generated instances rather than argued for.
//!
//! The node counts are compared too — not to claim a speed result, which this
//! project does not make, but because a cut round that produced exactly the
//! tree the uncut search produces would be a cut round that is not running.

use hc_core::Xoshiro256PlusPlus;
use hc_io::mps::{Form, read};
use hc_mip::search::{Ending, Options, run};

fn with_cuts(rounds: usize) -> Options {
    Options {
        cut_rounds: rounds,
        ..Options::default()
    }
}

fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME U\nROWS\n N obj\n");
    for row in 0..rows {
        let _ = writeln!(source, " L r{row}");
    }
    source.push_str("COLUMNS\n MARKER 'MARKER' 'INTORG'\n");
    for column in 0..columns {
        let _ = writeln!(source, " c{column} obj -{}", 1 + generator.next_below(9));
        for row in 0..rows {
            let value = 1 + generator.next_below(7);
            let _ = writeln!(source, " c{column} r{row} {value}");
        }
    }
    source.push_str(" MARKER 'MARKER' 'INTEND'\nRHS\n");
    for row in 0..rows {
        let _ = writeln!(source, " rhs r{row} {}", 5 + generator.next_below(20));
    }
    source.push_str("BOUNDS\n");
    for column in 0..columns {
        let _ = writeln!(source, " UI b c{column} {}", 1 + generator.next_below(3));
    }
    source.push_str("ENDATA\n");
    source
}

#[test]
fn cuts_change_the_tree_and_not_the_answer() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x0c07_0000_0000_0011);
    let mut compared = 0;
    let mut cut_something = 0;
    let mut different = 0;

    for round in 0..400 {
        let source = generate(&mut generator, 1 + (round % 2), 2 + (round % 3));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");

        let plain = run(&model, &Options::default());
        let sharpened = run(&model, &with_cuts(2));

        assert_eq!(
            plain.ending, sharpened.ending,
            "round {round}: cuts changed the status\n{source}"
        );
        assert_eq!(
            plain
                .incumbent
                .as_ref()
                .map(|found| found.objective.to_canonical_string()),
            sharpened
                .incumbent
                .as_ref()
                .map(|found| found.objective.to_canonical_string()),
            "round {round}: cuts changed the answer\n{source}"
        );
        compared += 1;
        if !sharpened.pool.is_empty() {
            cut_something += 1;
        }
        if plain.tree.len() != sharpened.tree.len() {
            different += 1;
        }
    }

    assert_eq!(compared, 400);
    assert!(
        cut_something > 20,
        "only {cut_something} instances produced any cut at all"
    );
    assert!(
        different > 0,
        "cuts never changed a tree, so the rounds are not running"
    );
}

#[test]
fn every_pooled_cut_is_true_of_the_subtree_it_names() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x0c07_0000_0000_0012);
    let mut pooled = 0;

    for round in 0..200 {
        let source = generate(&mut generator, 1 + (round % 2), 2 + (round % 3));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");
        let search = run(&model, &with_cuts(2));

        for entry in search.pool.entries() {
            pooled += 1;
            assert!(
                search.tree.get(entry.scope).is_some(),
                "a cut names node {} which the tree does not have",
                entry.scope
            );
            assert!(
                !entry.coefficients.is_empty(),
                "a cut with no coefficients says nothing"
            );
        }
    }
    assert!(pooled > 20, "only {pooled} cuts were pooled");
}

#[test]
fn cutting_is_reproducible() {
    let source = concat!(
        "NAME R\nROWS\n N obj\n L r0\n",
        "COLUMNS\n MARKER 'MARKER' 'INTORG'\n",
        " a obj -5 r0 3\n b obj -4 r0 3\n c obj -3 r0 2\n",
        " MARKER 'MARKER' 'INTEND'\n",
        "RHS\n rhs r0 7\nBOUNDS\n UI b a 2\n UI b b 2\n UI b c 2\nENDATA\n",
    );
    let model = read(source.as_bytes(), Form::Free).expect("the source parses");
    let first = run(&model, &with_cuts(3));
    let second = run(&model, &with_cuts(3));
    assert_eq!(first.pool, second.pool);
    assert_eq!(first.tree, second.tree);
    assert_eq!(first.ending, Ending::Optimal);
}
