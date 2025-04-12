//! A proof about a file that is not MPS.
//!
//! The format tag is inside the instance digest, so a certificate is a claim
//! about a file *under a format's rules*, not about a model that happens to
//! resemble one. That makes this its own check rather than a variation on the
//! MPS one: the verifier reparses the original bytes with the reader the tag
//! names, and everything downstream depends on it having picked the right one.

use hc_cert::assemble::assemble_lp;
use hc_cert::mip::assemble_mip;
use hc_core::{Model, Tol};
use hc_io::{json, lp};
use hc_lp::{Options, Problem, Working, solve};
use hc_mip::search::{Options as MipOptions, run as search_tree};
use hc_verify::vclose::verify;

const LP_TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/lp/tiny.lp");
const LP_RANGES: &[u8] = include_bytes!("../../../spec/v1/vectors/lp/ranges.lp");
const JSON_TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/json/tiny.json");
const JSON_RANGES: &[u8] = include_bytes!("../../../spec/v1/vectors/json/ranges.json");
const JSON_EXACT: &[u8] = include_bytes!("../../../spec/v1/vectors/json/exact.json");

/// Solves, certifies, and has the standalone verifier recheck the result.
fn certified(model: &Model, raw: &[u8]) -> String {
    let search = search_tree(model, &MipOptions::default());
    let assembled = assemble_mip(model, raw, &search).or_else(|_| {
        let working = Working::from_model(model);
        let problem = Problem::scaled(&working, Tol::solver());
        let (outcome, _) = solve(&problem, &Options::default());
        assemble_lp(model, raw, &outcome)
    });
    let assembled = assembled.unwrap_or_else(|reason| panic!("nothing to certify: {reason}"));
    let bytes = assembled.encode();
    match verify(&bytes, raw) {
        Ok(verdict) => format!("{verdict:?}"),
        Err(refusal) => panic!("the verifier refused a certificate about this file: {refusal}"),
    }
}

#[test]
fn a_proof_about_an_lp_file_verifies_against_the_original_bytes() {
    for raw in [LP_TINY, LP_RANGES] {
        let model = lp::read(raw).expect("a frozen vector");
        let verdict = certified(&model, raw);
        assert!(verdict.contains("Optimal"), "{verdict}");
    }
}

#[test]
fn a_proof_about_a_json_document_verifies_against_the_original_bytes() {
    for raw in [JSON_TINY, JSON_RANGES, JSON_EXACT] {
        let model = json::read(raw).expect("a frozen vector");
        let verdict = certified(&model, raw);
        assert!(verdict.contains("Optimal"), "{verdict}");
    }
}

#[test]
fn a_certificate_is_about_one_file_and_not_about_the_model_it_names() {
    // The same model, written in two formats. Each certificate verifies
    // against its own file and neither verifies against the other, because the
    // format tag and the bytes are both inside the digest the claim carries.
    let lp_model = lp::read(LP_TINY).expect("a frozen vector");
    let json_model = json::read(JSON_TINY).expect("a frozen vector");
    assert_eq!(
        lp_model
            .canonical_dump()
            .lines()
            .skip(3)
            .collect::<Vec<_>>(),
        json_model
            .canonical_dump()
            .lines()
            .skip(3)
            .collect::<Vec<_>>(),
        "the two files were supposed to name the same model"
    );

    let search = search_tree(&lp_model, &MipOptions::default());
    let proof = assemble_mip(&lp_model, LP_TINY, &search)
        .expect("the tree closes")
        .encode();
    assert!(verify(&proof, LP_TINY).is_ok());
    let refusal = verify(&proof, JSON_TINY).expect_err("a different file");
    assert_eq!(refusal.code.text(), "HCC-PARSE-03", "{refusal}");
}
