//! The two branching rules must reach the same answers.
//!
//! Which variable a node splits on decides how big the tree gets and decides
//! nothing about what the tree proves — the two children of any integer split
//! cover their parent whichever variable was chosen. So the rules are free to
//! be heuristic, and the property worth checking is that they are
//! interchangeable in every way that matters.
//!
//! The effort is also compared, not to assert that one rule wins — this
//! project makes no speed claims — but because a pseudocost rule that produced
//! exactly the tree the textbook rule produces would be a pseudocost rule that
//! is not running.

use hc_core::Xoshiro256PlusPlus;
use hc_io::mps::{Form, read};
use hc_mip::branching::Branching;
use hc_mip::search::{Options, run};

fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME B\nROWS\n N obj\n");
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
            generator.next_below(20) as i64 - 6
        );
    }
    source.push_str("BOUNDS\n");
    for column in 0..columns {
        let _ = writeln!(source, " UI b c{column} {}", 1 + generator.next_below(4));
    }
    source.push_str("ENDATA\n");
    source
}

fn options(rule: Branching) -> Options {
    Options {
        branching: rule,
        ..Options::default()
    }
}

#[test]
fn the_two_rules_reach_the_same_answers() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0xb1a1_0000_0000_0001_u64);
    let mut different_trees = 0;
    let mut compared = 0;

    for round in 0..600 {
        let source = generate(&mut generator, 1 + (round % 3), 1 + (round % 4));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");

        let simple = run(&model, &options(Branching::MostFractional));
        let clever = run(&model, &options(Branching::ReliabilityPseudocost));

        assert_eq!(
            simple.ending, clever.ending,
            "round {round}: the rules disagree about the status\n{source}"
        );
        assert_eq!(
            simple
                .incumbent
                .as_ref()
                .map(|found| found.objective.to_canonical_string()),
            clever
                .incumbent
                .as_ref()
                .map(|found| found.objective.to_canonical_string()),
            "round {round}: the rules disagree about the answer\n{source}"
        );
        compared += 1;
        if simple.tree.len() != clever.tree.len() {
            different_trees += 1;
        }
    }

    assert_eq!(compared, 600);
    assert!(
        different_trees > 0,
        "the two rules built identical trees everywhere, so one of them is not running"
    );
}

#[test]
fn each_rule_is_reproducible() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0xb1a1_0000_0000_0002_u64);
    for round in 0..50 {
        let source = generate(&mut generator, 2, 3);
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");
        for rule in [Branching::MostFractional, Branching::ReliabilityPseudocost] {
            let first = run(&model, &options(rule));
            let second = run(&model, &options(rule));
            assert_eq!(
                first.tree, second.tree,
                "round {round}: {rule:?} is not reproducible"
            );
            assert_eq!(first.opened, second.opened);
        }
    }
}
