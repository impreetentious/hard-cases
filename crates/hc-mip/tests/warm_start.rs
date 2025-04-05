//! Warm starts must not change the answer.
//!
//! A child node starts from its parent's basis because a branch changes one
//! bound and nothing else, so the parent's basis still prices every column
//! correctly. That is a claim about efficiency, and efficiency claims are
//! where correctness quietly goes wrong: a warm start that begins from a basis
//! it should not have would reach an answer nobody would think to question.
//!
//! So the property is checked directly. The same search, run with warm starts
//! and with every node solved from cold, must reach the same status and the
//! same exact objective. The trees may differ — a different optimal vertex
//! leads to a different branching variable, and that is allowed — but what the
//! search concludes may not.

use hc_core::Model;
use hc_core::Xoshiro256PlusPlus;
use hc_io::mps::{Form, read};
use hc_mip::search::{Ending, Options, run};

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
        "infeasible4",
        include_bytes!("../../../instances/local/infeasible4.mps"),
    ),
    (
        "tiny",
        include_bytes!("../../../spec/v1/vectors/mps/tiny.mps"),
    ),
];

fn cold() -> Options {
    Options {
        warm_start: false,
        ..Options::default()
    }
}

/// Runs both ways and compares what each concluded.
fn compare(name: &str, model: &Model) {
    let warm = run(model, &Options::default());
    let chilly = run(model, &cold());

    assert_eq!(
        warm.ending, chilly.ending,
        "{name}: the two starts reach different statuses"
    );
    assert_eq!(
        warm.incumbent
            .as_ref()
            .map(|found| found.objective.to_canonical_string()),
        chilly
            .incumbent
            .as_ref()
            .map(|found| found.objective.to_canonical_string()),
        "{name}: the two starts reach different objectives"
    );
    if warm.ending == Ending::Optimal {
        assert!(
            warm.frontier().is_empty() && chilly.frontier().is_empty(),
            "{name}: an optimal search left something open"
        );
    }
}

#[test]
fn trap_warmstart_matches_coldstart() {
    for (name, raw) in CORPUS {
        let model = read(raw, Form::Fixed).expect("the instance parses");
        compare(name, &model);
    }
}

/// A small integer generator, so the trap is exercised on trees rather than on
/// the corpus's mostly-continuous models.
fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME W\nROWS\n N obj\n");
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
fn the_two_starts_agree_across_a_campaign_of_generated_trees() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x00a1_0000_0000_0001_u64);
    for round in 0..500 {
        let rows = 1 + (round % 3);
        let columns = 1 + (round % 4);
        let source = generate(&mut generator, rows, columns);
        let model = read(source.as_bytes(), Form::Free)
            .unwrap_or_else(|error| panic!("the generator produced {error}\n{source}"));
        compare(&format!("round {round}\n{source}"), &model);
    }
}
