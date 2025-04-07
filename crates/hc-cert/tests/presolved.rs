//! Presolve, end to end: reduce, search the reduced model, certify the chain
//! and the tree together, and have the independent verifier replay all of it.
//!
//! This is the arrangement presolve is usually missing. The reduction is not a
//! preprocessing step the proof is silent about; it is the first part of the
//! proof, and the verifier recomputes every stage from the original bytes
//! before it looks at a single bound.

use hc_cert::mip::assemble_presolved;
use hc_core::{Model, Xoshiro256PlusPlus};
use hc_io::mps::{Form, read};
use hc_mip::search::{Ending, Options, run};
use hc_presolve::reduce;
use hc_verify::vclose::verify;
use hc_verify::vhcc::Claim;

fn parse(source: &str) -> Model {
    read(source.as_bytes(), Form::Free)
        .unwrap_or_else(|error| panic!("the source does not parse: {error}\n{source}"))
}

/// Reduces, searches the reduced model, certifies, and verifies.
fn round_trip(name: &str, source: &str) -> Option<Claim> {
    let model = parse(source);
    let chain = reduce(&model);
    let search = run(&chain.reduced, &Options::default());
    let assembled = assemble_presolved(&model, &chain, source.as_bytes(), &search).ok()?;
    let verdict = verify(&assembled.encode(), source.as_bytes())
        .unwrap_or_else(|refusal| panic!("{name}: the verifier refused: {refusal}"));
    Some(verdict.claim)
}

const REDUCIBLE: &str = concat!(
    "NAME R\n",
    "ROWS\n",
    " N obj\n",
    " L r0\n",
    " L r1\n",
    "COLUMNS\n",
    " MARKER 'MARKER' 'INTORG'\n",
    " x obj -4 r0 3\n",
    " x r1 2\n",
    " y obj -3 r0 4\n",
    " y r1 1\n",
    " z obj -1 r0 0\n",
    " MARKER 'MARKER' 'INTEND'\n",
    "RHS\n",
    " rhs r0 12 r1 6\n",
    "BOUNDS\n",
    " UI b x 10\n",
    " UI b y 10\n",
    " FX b z 2\n",
    "ENDATA\n",
);

#[test]
fn a_reduced_model_certifies_and_the_chain_verifies() {
    let claim = round_trip("reducible", REDUCIBLE).expect("a certificate");
    assert_eq!(claim, Claim::Optimal);
}

#[test]
fn the_original_witness_is_checked_against_the_original_bytes() {
    // The chain fixes z out of the model, so the reduced witness has two
    // components and the original has three. The verifier checks the second
    // against the file it parsed.
    let model = parse(REDUCIBLE);
    let chain = reduce(&model);
    assert!(!chain.steps.is_empty(), "this instance should reduce");

    let search = run(&chain.reduced, &Options::default());
    assert_eq!(search.ending, Ending::Optimal);
    let assembled =
        assemble_presolved(&model, &chain, REDUCIBLE.as_bytes(), &search).expect("a certificate");

    let incumbent = assembled
        .records
        .iter()
        .find_map(|record| match record {
            hc_cert::hcc::Record::Incumbent {
                reduced, original, ..
            } => Some((reduced.len(), original.len())),
            _ => None,
        })
        .expect("an incumbent");
    assert_eq!(incumbent.1, model.variables().len());
    assert!(
        incumbent.0 < incumbent.1,
        "the reduced witness should be shorter"
    );

    verify(&assembled.encode(), REDUCIBLE.as_bytes()).expect("the verifier accepts it");
}

#[test]
fn a_presolve_contradiction_proves_infeasibility_on_its_own() {
    // x >= 5 as a singleton row against an upper bound of 2.
    let source = concat!(
        "NAME C\n",
        "ROWS\n",
        " N obj\n",
        " G r0\n",
        "COLUMNS\n",
        " MARKER 'MARKER' 'INTORG'\n",
        " x obj 1 r0 1\n",
        " MARKER 'MARKER' 'INTEND'\n",
        "RHS\n",
        " rhs r0 5\n",
        "BOUNDS\n",
        " UP b x 2\n",
        "ENDATA\n",
    );
    let model = parse(source);
    let chain = reduce(&model);
    assert!(
        chain.contradiction.is_some(),
        "the reduction should close the domain"
    );

    let search = run(&chain.reduced, &Options::default());
    let assembled =
        assemble_presolved(&model, &chain, source.as_bytes(), &search).expect("a certificate");
    let verdict = verify(&assembled.encode(), source.as_bytes()).expect("the verifier accepts it");
    assert_eq!(verdict.claim, Claim::Infeasible);

    // No root and no tree: the contradiction stands alone.
    assert!(
        !assembled
            .records
            .iter()
            .any(|record| matches!(record, hc_cert::hcc::Record::Root { .. })),
        "a presolve contradiction carries no root"
    );
}

/// A small bounded integer program.
fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME G\nROWS\n N obj\n");
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
fn a_campaign_of_reduced_searches_all_verify() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x9105_0000_0000_0001);
    let mut certified = 0;
    let mut reduced_chains = 0;

    for round in 0..1_000 {
        let source = generate(&mut generator, 1 + (round % 3), 1 + (round % 3));
        let model = parse(&source);
        let chain = reduce(&model);
        if !chain.steps.is_empty() {
            reduced_chains += 1;
        }
        let search = run(&chain.reduced, &Options::default());
        let Ok(assembled) = assemble_presolved(&model, &chain, source.as_bytes(), &search) else {
            continue;
        };
        verify(&assembled.encode(), source.as_bytes()).unwrap_or_else(|refusal| {
            panic!("round {round}: the verifier refused: {refusal}\n{source}")
        });
        certified += 1;
    }

    assert!(reduced_chains > 50, "only {reduced_chains} chains reduced");
    assert!(
        certified > 200,
        "only {certified} certificates were accepted"
    );
}
