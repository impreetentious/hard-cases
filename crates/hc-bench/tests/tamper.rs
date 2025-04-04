//! The tamper corpus, run against the standalone verifier.
//!
//! Two failures are being guarded against, and the second is the subtle one.
//!
//! The first is that a forged certificate is accepted. The second is that a
//! forged certificate is rejected *by the wrong rule* — most often by the
//! checksum, which would catch a stale terminator or a length that no longer
//! adds up and would say nothing whatsoever about the proof calculus. Every
//! entry therefore pins its identifier, and most entries carry a correct
//! checksum so that the mathematics is what has to answer.
//!
//! The manifest is walked rather than transcribed. An entry named there and
//! not implemented fails, and an implemented mutation not named there fails
//! too, because a corpus entry that nothing runs is a file that looks like
//! coverage.

use hc_bench::tamper::{CASES, forge, instance_for};
use hc_cert::assemble::assemble_lp;
use hc_cert::mip::assemble_mip;
use hc_core::{SpecDocument, Tol};
use hc_io::mps::{Form, read};
use hc_lp::{Options, Problem, Working, solve};
use hc_mip::search::{Options as MipOptions, run as search_tree};
use hc_verify::vclose::verify;

const MANIFEST: &str = include_str!("../../../tamper/manifest.toml");

/// Two integer programs, inline rather than committed to the corpus: they
/// exist to be forged, not to be solved for their own sake.
const BRANCHY: &str = concat!(
    "NAME B\n",
    "ROWS\n",
    " N obj\n",
    " L c1\n",
    " L c2\n",
    "COLUMNS\n",
    " MARKER 'MARKER' 'INTORG'\n",
    " x obj -4 c1 3\n",
    " x c2 2\n",
    " y obj -3 c1 4\n",
    " y c2 1\n",
    " MARKER 'MARKER' 'INTEND'\n",
    "RHS\n",
    " rhs c1 12 c2 6\n",
    "BOUNDS\n",
    " UI b x 10\n",
    " UI b y 10\n",
    "ENDATA\n",
);

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

const TREES: &[(&str, &str)] = &[("branchy", BRANCHY), ("knapsack", KNAPSACK)];

const BASE: &[(&str, &[u8])] = &[
    (
        "transport33.mps",
        include_bytes!("../../../instances/local/transport33.mps"),
    ),
    (
        "blend4.mps",
        include_bytes!("../../../instances/local/blend4.mps"),
    ),
    (
        "infeasible4.mps",
        include_bytes!("../../../instances/local/infeasible4.mps"),
    ),
    (
        "unbounded3.mps",
        include_bytes!("../../../instances/local/unbounded3.mps"),
    ),
    (
        "tiny.mps",
        include_bytes!("../../../spec/v1/vectors/mps/tiny.mps"),
    ),
];

/// Solves an instance and returns the certificate it earns.
fn genuine(name: &str, raw: &[u8]) -> Vec<u8> {
    let model = read(raw, Form::Fixed).unwrap_or_else(|e| panic!("{name} does not parse: {e}"));
    let working = Working::from_model(&model);
    let problem = Problem::scaled(&working, Tol::solver());
    let (outcome, _) = solve(&problem, &Options::default());
    assemble_lp(&model, raw, &outcome)
        .unwrap_or_else(|e| panic!("{name} produced no certificate: {e}"))
        .encode()
}

/// Searches an integer program and returns the tree certificate it earns.
fn genuine_tree(name: &str, raw: &[u8]) -> Vec<u8> {
    let model = read(raw, Form::Free).unwrap_or_else(|e| panic!("{name} does not parse: {e}"));
    let search = search_tree(&model, &MipOptions::default());
    assemble_mip(&model, raw, &search)
        .unwrap_or_else(|e| panic!("{name} produced no certificate: {e}"))
        .encode()
}

fn manifest() -> SpecDocument {
    SpecDocument::parse(MANIFEST).expect("the tamper manifest is inside the subset")
}

#[test]
fn the_manifest_and_the_implemented_mutations_are_the_same_list() {
    let document = manifest();
    let named: Vec<String> = document
        .tables_under("case")
        .map(|table| table.name().trim_start_matches("case.").to_owned())
        .collect();
    let implemented: Vec<String> = CASES.iter().map(|case| case.name.to_owned()).collect();
    assert_eq!(named, implemented, "the two lists have drifted apart");

    for case in CASES {
        let name = format!("case.{}", case.name);
        let table = document
            .table(&name)
            .unwrap_or_else(|| panic!("[{name}] is not in the manifest"));
        assert_eq!(
            table.text("expect"),
            Some(case.expect),
            "[{name}] expects a different rule than the harness does"
        );
        let resealed = table
            .get("resealed")
            .and_then(hc_core::SpecValue::as_boolean)
            .unwrap_or_else(|| panic!("[{name}] does not say whether it is resealed"));
        assert_eq!(
            resealed, case.resealed,
            "[{name}] disagrees with the harness about the checksum"
        );
        for key in ["edit", "note"] {
            assert!(table.text(key).is_some(), "[{name}] has no {key}");
        }
    }
}

#[test]
fn every_forgery_is_refused_by_the_rule_the_corpus_names() {
    // Which mutations found something to edit, so that a mutation that applies
    // nowhere can be reported rather than passing silently.
    let mut applied: Vec<&str> = Vec::new();

    for (instance_name, raw) in BASE {
        let certificate = genuine(instance_name, raw);
        // A different instance, for the reattachment case.
        let (_, other) = BASE
            .iter()
            .find(|(name, _)| name != instance_name)
            .expect("the corpus has more than one instance");

        // The genuine article must verify, or the forgeries below prove
        // nothing about forgery.
        verify(&certificate, raw).unwrap_or_else(|e| {
            panic!("{instance_name}: the genuine certificate was refused: {e}")
        });

        for case in CASES {
            let Ok(forged) = forge(case.name, &certificate, raw) else {
                continue;
            };
            if !applied.contains(&case.name) {
                applied.push(case.name);
            }
            let against = instance_for(case.name, raw, other);

            let refusal = match verify(&forged, against) {
                Err(refusal) => refusal,
                Ok(verdict) => panic!(
                    "{instance_name}/{}: a forged certificate was accepted as {}",
                    case.name,
                    verdict.claim.word()
                ),
            };
            assert_eq!(
                refusal.code.text(),
                case.expect,
                "{instance_name}/{}: refused by the wrong rule — {refusal}",
                case.name
            );
        }
    }

    for (instance_name, source) in TREES {
        let raw = source.as_bytes();
        let certificate = genuine_tree(instance_name, raw);
        let (_, other) = TREES
            .iter()
            .find(|(name, _)| name != instance_name)
            .expect("more than one tree instance");
        let other = other.as_bytes();

        verify(&certificate, raw).unwrap_or_else(|e| {
            panic!("{instance_name}: the genuine tree certificate was refused: {e}")
        });

        for case in CASES {
            let Ok(forged) = forge(case.name, &certificate, raw) else {
                continue;
            };
            if !applied.contains(&case.name) {
                applied.push(case.name);
            }
            let against = instance_for(case.name, raw, other);
            let refusal = match verify(&forged, against) {
                Err(refusal) => refusal,
                Ok(verdict) => panic!(
                    "{instance_name}/{}: a forged tree was accepted as {}",
                    case.name,
                    verdict.claim.word()
                ),
            };
            assert_eq!(
                refusal.code.text(),
                case.expect,
                "{instance_name}/{}: refused by the wrong rule — {refusal}",
                case.name
            );
        }
    }

    let missing: Vec<&str> = CASES
        .iter()
        .map(|case| case.name)
        .filter(|name| !applied.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "these mutations found nothing to edit anywhere in the corpus: {missing:?}"
    );
}
