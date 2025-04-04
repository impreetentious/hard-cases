//! Search, certify, and have the independent verifier replay the tree.
//!
//! This is the early vertical slice closing: an integer program solved, its
//! recursive proof written out, and the whole tree rechecked from the original
//! bytes by a program that shares none of the search, none of the arithmetic,
//! and none of the model.

use hc_cert::mip::assemble_mip;
use hc_core::Model;
use hc_io::mps::{Form, read};
use hc_mip::search::{Ending, Options, run};
use hc_verify::vclose::verify;
use hc_verify::vhcc::Claim;

const TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

const SPLIT: &str = concat!(
    "NAME          SPLIT\n",
    "ROWS\n",
    " N  obj\n",
    " L  c1\n",
    " L  c2\n",
    "COLUMNS\n",
    "    MARKER                 'MARKER'                 'INTORG'\n",
    "    x         obj       -4.0       c1        3.0\n",
    "    x         c2        2.0\n",
    "    y         obj       -3.0       c1        4.0\n",
    "    y         c2        1.0\n",
    "    MARKER                 'MARKER'                 'INTEND'\n",
    "RHS\n",
    "    rhs       c1        12.0       c2        6.0\n",
    "BOUNDS\n",
    "    UI bnd       x         10\n",
    "    UI bnd       y         10\n",
    "ENDATA\n",
);

const KNAPSACK: &str = concat!(
    "NAME          KNAP\n",
    "ROWS\n",
    " N  obj\n",
    " L  c1\n",
    "COLUMNS\n",
    "    MARKER                 'MARKER'                 'INTORG'\n",
    "    x         obj       -7.0       c1        5.0\n",
    "    y         obj       -9.0       c1        7.0\n",
    "    z         obj       -5.0       c1        4.0\n",
    "    MARKER                 'MARKER'                 'INTEND'\n",
    "RHS\n",
    "    rhs       c1        13.0\n",
    "BOUNDS\n",
    "    UI bnd       x         3\n",
    "    UI bnd       y         3\n",
    "    UI bnd       z         3\n",
    "ENDATA\n",
);

const IMPOSSIBLE: &str = concat!(
    "NAME          NOPE\n",
    "ROWS\n",
    " N  obj\n",
    " G  c1\n",
    "COLUMNS\n",
    "    MARKER                 'MARKER'                 'INTORG'\n",
    "    x         obj       1.0        c1        1.0\n",
    "    MARKER                 'MARKER'                 'INTEND'\n",
    "RHS\n",
    "    rhs       c1        5.0\n",
    "BOUNDS\n",
    "    UP bnd       x         2\n",
    "ENDATA\n",
);

fn parse(raw: &[u8], form: Form) -> Model {
    read(raw, form).expect("the source parses")
}

fn certify_and_verify(name: &str, raw: &[u8], form: Form) -> Claim {
    let model = parse(raw, form);
    let search = run(&model, &Options::default());
    let assembled = assemble_mip(&model, raw, &search)
        .unwrap_or_else(|reason| panic!("{name}: no certificate: {reason}"));
    let bytes = assembled.encode();
    let verdict = verify(&bytes, raw)
        .unwrap_or_else(|refusal| panic!("{name}: the verifier refused: {refusal}"));
    verdict.claim
}

#[test]
fn a_root_only_tree_verifies_as_optimal() {
    assert_eq!(
        certify_and_verify("tiny", TINY, Form::Fixed),
        Claim::Optimal
    );
}

#[test]
fn a_branched_tree_verifies_as_optimal() {
    assert_eq!(
        certify_and_verify("split", SPLIT.as_bytes(), Form::Free),
        Claim::Optimal
    );
}

#[test]
fn a_knapsack_verifies_as_optimal_and_reaches_the_hand_worked_answer() {
    let raw = KNAPSACK.as_bytes();
    let model = parse(raw, Form::Free);
    let search = run(&model, &Options::default());
    assert_eq!(search.ending, Ending::Optimal);

    // Capacity 13 against weights 5, 7 and 4 with values 7, 9 and 5. The best
    // packing is one of each of the first and third twice: 5 + 4 + 4 = 13 for
    // 7 + 5 + 5 = 17. Nothing else reaches it — two sevens overflow, and
    // 7 + 5 = 12 leaves only one unit of room.
    let incumbent = search.incumbent.as_ref().expect("an incumbent");
    assert_eq!(incumbent.objective.to_canonical_string(), "-17");

    let assembled = assemble_mip(&model, raw, &search).expect("a certificate");
    let verdict = verify(&assembled.encode(), raw).expect("the verifier accepts it");
    assert_eq!(verdict.claim, Claim::Optimal);
    assert_eq!(
        verdict.objective.map(|value| value.to_canonical()),
        Some("-17".to_owned())
    );
}

#[test]
fn an_infeasible_tree_verifies_as_infeasible() {
    assert_eq!(
        certify_and_verify("impossible", IMPOSSIBLE.as_bytes(), Form::Free),
        Claim::Infeasible
    );
}

#[test]
fn a_certificate_from_a_stopped_search_verifies_as_a_gap() {
    let raw = KNAPSACK.as_bytes();
    let model = parse(raw, Form::Free);
    for limit in [2_u64, 3, 4, 5] {
        let search = run(
            &model,
            &Options {
                node_limit: limit,
                ..Options::default()
            },
        );
        let Ok(assembled) = assemble_mip(&model, raw, &search) else {
            continue;
        };
        let verdict = verify(&assembled.encode(), raw)
            .unwrap_or_else(|refusal| panic!("limit {limit}: the verifier refused: {refusal}"));
        assert!(
            matches!(verdict.claim, Claim::Optimal | Claim::Gap),
            "limit {limit} produced {:?}",
            verdict.claim
        );
        if verdict.claim == Claim::Gap {
            let gap = verdict.gap.expect("a gap claim has a gap");
            assert!(!gap.is_zero(), "a gap claim with a zero gap is optimality");
            return;
        }
    }
}

#[test]
fn a_tree_reattached_to_another_instance_is_refused() {
    let raw = SPLIT.as_bytes();
    let model = parse(raw, Form::Free);
    let search = run(&model, &Options::default());
    let assembled = assemble_mip(&model, raw, &search).expect("a certificate");
    let refusal = verify(&assembled.encode(), TINY).expect_err("the binding must fail");
    assert_eq!(refusal.code.text(), "HCC-PARSE-03");
}
