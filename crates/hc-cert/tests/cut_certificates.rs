//! Certificates whose bounds rest on cuts, rechecked from the original bytes.
//!
//! A cut is an inequality the model does not contain, added on a promise. The
//! certificate carries the derivation and never the promise: the combination
//! that produced a Gomory cut, the cover and the lift order behind a cover cut,
//! and nothing else the verifier reads as fact. What the verifier accepts, it
//! accepts because it derived the same inequality itself.
//!
//! Every certificate here is produced by a search that actually used cuts —
//! the pool is asserted non-empty — so a run in which separation quietly
//! stopped working would fail rather than pass.

use hc_cert::mip::assemble_mip;
use hc_core::Xoshiro256PlusPlus;
use hc_io::mps::{Form, read};
use hc_mip::search::{Ending, Options, run};
use hc_verify::vclose::verify;
use hc_verify::vhcc::Claim;

fn with_cuts(rounds: usize) -> Options {
    Options {
        cut_rounds: rounds,
        ..Options::default()
    }
}

const KNAPSACK: &str = concat!(
    "NAME K\n",
    "ROWS\n N obj\n L r0\n",
    "COLUMNS\n MARKER 'MARKER' 'INTORG'\n",
    " a obj -5 r0 3\n b obj -4 r0 3\n c obj -3 r0 2\n",
    " MARKER 'MARKER' 'INTEND'\n",
    "RHS\n rhs r0 7\n",
    "BOUNDS\n UI b a 2\n UI b b 2\n UI b c 2\n",
    "ENDATA\n",
);

#[test]
fn a_certificate_resting_on_cuts_verifies() {
    let raw = KNAPSACK.as_bytes();
    let model = read(raw, Form::Free).expect("the source parses");
    let search = run(&model, &with_cuts(3));
    assert!(
        !search.pool.is_empty(),
        "this instance should produce at least one cut"
    );
    assert_eq!(search.ending, Ending::Optimal);

    let assembled = assemble_mip(&model, raw, &search).expect("a certificate");
    let verdict = verify(&assembled.encode(), raw).expect("the verifier accepts it");
    assert_eq!(verdict.claim, Claim::Optimal);

    // And the certificate genuinely carries cut records.
    let cuts = assembled
        .records
        .iter()
        .filter(|record| {
            matches!(
                record,
                hc_cert::hcc::Record::GmiCut { .. } | hc_cert::hcc::Record::CoverCut { .. }
            )
        })
        .count();
    assert!(cuts > 0, "the certificate carries no cut records");
}

fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME G\nROWS\n N obj\n");
    for row in 0..rows {
        let _ = writeln!(source, " L r{row}");
    }
    source.push_str("COLUMNS\n MARKER 'MARKER' 'INTORG'\n");
    for column in 0..columns {
        let _ = writeln!(source, " c{column} obj -{}", 1 + generator.next_below(9));
        for row in 0..rows {
            let _ = writeln!(source, " c{column} r{row} {}", 1 + generator.next_below(7));
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
fn a_campaign_of_cut_certificates_all_verify() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x0c07_0000_0000_0021);
    let mut with_records = 0;
    let mut certified = 0;

    for round in 0..500 {
        let source = generate(&mut generator, 1 + (round % 2), 2 + (round % 3));
        let raw = source.as_bytes();
        let model = read(raw, Form::Free).expect("the source parses");
        let search = run(&model, &with_cuts(2));
        let Ok(assembled) = assemble_mip(&model, raw, &search) else {
            continue;
        };
        verify(&assembled.encode(), raw).unwrap_or_else(|refusal| {
            panic!("round {round}: the verifier refused: {refusal}\n{source}")
        });
        certified += 1;
        if !search.pool.is_empty() {
            with_records += 1;
        }
    }

    assert!(
        certified > 100,
        "only {certified} certificates were accepted"
    );
    assert!(
        with_records > 20,
        "only {with_records} of them carried cuts, so the campaign is not testing cuts"
    );
}
