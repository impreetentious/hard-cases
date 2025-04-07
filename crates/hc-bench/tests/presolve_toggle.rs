//! Presolve must not change what is true.
//!
//! A reduction chain is supposed to be an equivalence, so a search with
//! presolve on and the same search with it off must certify the same status and
//! the same objective. That is a stronger statement than the invariants alone:
//! the invariants say each transform preserves optima, and this says the whole
//! arrangement — reduce, search the reduced model, lift the witness, verify
//! the chain — arrives where the unreduced one does.
//!
//! Both certificates are verified independently. A campaign that compared two
//! answers without checking either would be comparing two guesses.

use hc_cert::mip::{assemble_mip, assemble_presolved};
use hc_core::{Model, Xoshiro256PlusPlus};
use hc_io::mps::{Form, read};
use hc_mip::search::{Options, run};
use hc_presolve::reduce;
use hc_verify::vclose::verify;
use hc_verify::vhcc::Claim;

/// What one route concluded, when it concluded anything.
fn route(model: &Model, raw: &[u8], presolve: bool) -> Option<(Claim, String)> {
    if presolve {
        let chain = reduce(model);
        let search = run(&chain.reduced, &Options::default());
        let assembled = assemble_presolved(model, &chain, raw, &search).ok()?;
        let verdict = verify(&assembled.encode(), raw)
            .unwrap_or_else(|refusal| panic!("the reduced certificate was refused: {refusal}"));
        Some((
            verdict.claim,
            verdict
                .objective
                .map(|value| value.to_canonical())
                .unwrap_or_default(),
        ))
    } else {
        let search = run(model, &Options::default());
        let assembled = assemble_mip(model, raw, &search).ok()?;
        let verdict = verify(&assembled.encode(), raw)
            .unwrap_or_else(|refusal| panic!("the unreduced certificate was refused: {refusal}"));
        Some((
            verdict.claim,
            verdict
                .objective
                .map(|value| value.to_canonical())
                .unwrap_or_default(),
        ))
    }
}

/// A small bounded integer program.
fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME T\nROWS\n N obj\n");
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
fn the_two_routes_certify_the_same_thing() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x7066_0000_0000_0001);
    let mut compared = 0;
    let mut reduced_paths = 0;

    for round in 0..1_500 {
        let source = generate(&mut generator, 1 + (round % 3), 1 + (round % 3));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");
        if !reduce(&model).steps.is_empty() {
            reduced_paths += 1;
        }

        let with = route(&model, source.as_bytes(), true);
        let without = route(&model, source.as_bytes(), false);

        // One route certifying where the other declines is a difference in
        // what could be established, not a disagreement about what is true.
        // It is allowed, and it is counted nowhere.
        if let (Some(left), Some(right)) = (with, without) {
            assert_eq!(
                left, right,
                "round {round}: presolve changed the answer\n{source}"
            );
            compared += 1;
        }
    }

    assert!(
        compared > 300,
        "only {compared} instances were compared both ways"
    );
    assert!(reduced_paths > 50, "only {reduced_paths} instances reduced");
}

#[test]
fn the_committed_corpus_agrees_both_ways() {
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

    for (name, raw) in CORPUS {
        let model = read(raw, Form::Fixed).expect("the instance parses");
        let with = route(&model, raw, true);
        let without = route(&model, raw, false);
        if let (Some(left), Some(right)) = (with, without) {
            assert_eq!(left, right, "{name}: presolve changed the answer");
        }
    }
}
