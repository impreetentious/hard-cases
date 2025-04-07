//! The two presolve implementations against each other.
//!
//! One reduces; the other replays. They share the specification, the rule
//! identifiers, and nothing else — different model types, different
//! arithmetic, different code. After every step of every chain, both render a
//! canonical dump, and the two dumps are compared byte for byte.
//!
//! The comparison is on the dumps rather than on the models for the same
//! reason the parser differential is: a comparison performed through either
//! implementation's model type would be comparing one of them against itself,
//! and would pass for exactly the disagreements it exists to find.
//!
//! This is also where the hashes in a chain stop being decoration. A step's
//! stated hash is the hash of a dump, so a byte-identical dump on both sides
//! means both sides compute the same hash, which is what makes the record
//! checkable at all.

use hc_core::{Model, Xoshiro256PlusPlus};
use hc_io::mps::{Form, read as read_solver};
use hc_presolve::chain::model_hash;
use hc_presolve::{encode, reduce};
use hc_verify::vparse::{Layout, read as read_verifier};
use hc_verify::vpresolve::{apply as replay, read as read_rule};

/// A small bounded integer program in free-form MPS.
fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME X\nROWS\n N obj\n");
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

/// Replays one chain on both sides, comparing every stage.
fn compare(source: &str) -> usize {
    let model: Model =
        read_solver(source.as_bytes(), Form::Free).expect("the solver reads the source");
    let mut theirs =
        read_verifier(source.as_bytes(), Layout::Free).expect("the verifier reads the source");

    assert_eq!(
        model.canonical_dump(),
        theirs.dump(),
        "the two parsers disagree before any reduction\n{source}"
    );

    let chain = reduce(&model);
    let mut ours = model.clone();
    for (position, step) in chain.steps.iter().enumerate() {
        ours = hc_presolve::apply(&ours, &step.rule)
            .unwrap_or_else(|reason| panic!("step {position} no longer applies: {reason}"));

        let (rule, parameters) = encode(&step.rule);
        let parameters: Vec<hc_verify::vrat::Frac> = parameters
            .iter()
            .map(|value| {
                hc_verify::vrat::Frac::from_canonical(&value.to_canonical_string())
                    .expect("canonical rational text")
            })
            .collect();
        let reduction = read_rule(rule, &parameters).unwrap_or_else(|refusal| {
            panic!("the verifier will not read step {position}: {refusal}\n{source}")
        });
        theirs = replay(&theirs, &reduction).unwrap_or_else(|refusal| {
            panic!("the verifier refuses step {position}: {refusal}\n{source}")
        });

        assert_eq!(
            ours.canonical_dump(),
            theirs.dump(),
            "the two implementations disagree after step {position} ({:?})\n{source}",
            step.rule
        );
        assert_eq!(
            model_hash(&ours),
            step.result,
            "step {position} states a hash it does not produce\n{source}"
        );
    }
    chain.steps.len()
}

#[test]
fn trap_transform_after_data_not_trusted() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0xc205_0000_0000_0001);
    let mut steps = 0;
    let mut reduced = 0;

    for round in 0..1_500 {
        let source = generate(&mut generator, 1 + (round % 3), 1 + (round % 3));
        let taken = compare(&source);
        steps += taken;
        if taken > 0 {
            reduced += 1;
        }
    }

    assert!(reduced > 100, "only {reduced} instances reduced at all");
    assert!(steps > 200, "only {steps} steps were replayed");
}

#[test]
fn the_committed_corpus_replays_on_both_sides() {
    const CORPUS: &[(&str, &[u8])] = &[
        (
            "transport33",
            include_bytes!("../../../instances/local/transport33.mps"),
        ),
        (
            "blend4",
            include_bytes!("../../../instances/local/blend4.mps"),
        ),
        (
            "cover6",
            include_bytes!("../../../instances/local/cover6.mps"),
        ),
        (
            "flow8",
            include_bytes!("../../../instances/local/flow8.mps"),
        ),
        (
            "cycle7",
            include_bytes!("../../../instances/local/cycle7.mps"),
        ),
        (
            "tiny",
            include_bytes!("../../../spec/v1/vectors/mps/tiny.mps"),
        ),
    ];

    for (name, raw) in CORPUS {
        let model = read_solver(raw, Form::Fixed).expect("the instance parses");
        let mut theirs =
            read_verifier(raw, Layout::Fixed).expect("the verifier reads the instance");
        let chain = reduce(&model);
        let mut ours = model.clone();
        for (position, step) in chain.steps.iter().enumerate() {
            ours = hc_presolve::apply(&ours, &step.rule).expect("a recorded rule applies");
            let (rule, parameters) = encode(&step.rule);
            let parameters: Vec<hc_verify::vrat::Frac> = parameters
                .iter()
                .map(|value| {
                    hc_verify::vrat::Frac::from_canonical(&value.to_canonical_string())
                        .expect("canonical rational text")
                })
                .collect();
            let reduction = read_rule(rule, &parameters).expect("the verifier reads the rule");
            theirs = replay(&theirs, &reduction).unwrap_or_else(|refusal| {
                panic!("{name}: the verifier refuses step {position}: {refusal}")
            });
            assert_eq!(
                ours.canonical_dump(),
                theirs.dump(),
                "{name}: the two implementations disagree after step {position}"
            );
        }
    }
}
