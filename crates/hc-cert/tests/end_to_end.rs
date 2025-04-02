//! Parse, solve, certify, verify — on real instance files, through the
//! standalone verifier.
//!
//! This is the arrangement the project exists to demonstrate, and it is the
//! first place the whole of it runs. Each committed instance is parsed by the
//! solver's reader, solved in floating point, rebuilt exactly into a
//! certificate, and then handed with its *original bytes* to a verifier that
//! shares none of that: its own parser, its own arithmetic, its own model, its
//! own rules. Only the verifier's acceptance counts.
//!
//! Nothing here asserts that the solver reaches a particular answer. It
//! asserts that whatever answer the solver certifies survives being rechecked
//! by something that does not trust it — and, for the corpus entries whose
//! outcome is known by hand, that the certified status is the known one.

use hc_cert::assemble::assemble_lp;
use hc_core::{Model, Tol};
use hc_io::mps::{Form, read};
use hc_lp::{Options, Problem, Working, solve};
use hc_verify::vclose::{Verdict, verify};
use hc_verify::vhcc::Claim;

const CORPUS: &[(&str, &[u8], Option<Claim>)] = &[
    (
        "transport33.mps",
        include_bytes!("../../../instances/local/transport33.mps"),
        Some(Claim::Optimal),
    ),
    (
        "blend4.mps",
        include_bytes!("../../../instances/local/blend4.mps"),
        Some(Claim::Optimal),
    ),
    (
        "cover6.mps",
        include_bytes!("../../../instances/local/cover6.mps"),
        Some(Claim::Optimal),
    ),
    (
        "flow8.mps",
        include_bytes!("../../../instances/local/flow8.mps"),
        Some(Claim::Optimal),
    ),
    (
        "freevars5.mps",
        include_bytes!("../../../instances/local/freevars5.mps"),
        Some(Claim::Optimal),
    ),
    (
        "cycle7.mps",
        include_bytes!("../../../instances/local/cycle7.mps"),
        Some(Claim::Optimal),
    ),
    (
        "infeasible4.mps",
        include_bytes!("../../../instances/local/infeasible4.mps"),
        Some(Claim::Infeasible),
    ),
    (
        "unbounded3.mps",
        include_bytes!("../../../instances/local/unbounded3.mps"),
        Some(Claim::Unbounded),
    ),
    (
        "tiny.mps",
        include_bytes!("../../../spec/v1/vectors/mps/tiny.mps"),
        Some(Claim::Optimal),
    ),
    (
        "ranges.mps",
        include_bytes!("../../../spec/v1/vectors/mps/ranges.mps"),
        Some(Claim::Optimal),
    ),
    (
        "exact.mps",
        include_bytes!("../../../spec/v1/vectors/mps/exact.mps"),
        Some(Claim::Optimal),
    ),
];

/// Solves an instance and returns the certificate bytes it earns.
fn certify(name: &str, raw: &[u8]) -> Vec<u8> {
    let model: Model =
        read(raw, Form::Fixed).unwrap_or_else(|error| panic!("{name} does not parse: {error}"));
    let working = Working::from_model(&model);
    let problem = Problem::scaled(&working, Tol::solver());
    let (outcome, _) = solve(&problem, &Options::default());
    let certificate = assemble_lp(&model, raw, &outcome)
        .unwrap_or_else(|error| panic!("{name} produced no certificate: {error}"));
    certificate.encode()
}

/// Verifies a certificate the way a release script would: from the bytes.
fn recheck(name: &str, certificate: &[u8], raw: &[u8]) -> Verdict {
    verify(certificate, raw).unwrap_or_else(|refusal| panic!("{name} was rejected: {refusal}"))
}

#[test]
fn every_corpus_instance_certifies_and_the_verifier_accepts_it() {
    for (name, raw, expected) in CORPUS {
        let certificate = certify(name, raw);
        let verdict = recheck(name, &certificate, raw);
        if let Some(claim) = expected {
            assert_eq!(verdict.claim, *claim, "{name} certified the wrong status");
        }
    }
}

#[test]
fn a_certificate_reattached_to_another_instance_is_refused() {
    let (name, raw, _) = CORPUS[0];
    let certificate = certify(name, raw);
    let (_, other, _) = CORPUS[1];
    let refusal = verify(&certificate, other).expect_err("the binding must fail");
    assert_eq!(refusal.code.text(), "HCC-PARSE-03");
}

#[test]
fn one_edited_digit_in_a_multiplier_is_caught() {
    // The mutation the README quotes: a sound-looking certificate in which one
    // rational has been changed. It must fail, and it must fail by name.
    let (name, raw, _) = CORPUS[0];
    let mut certificate = certify(name, raw);

    // Find the first canonical rational token in the stream and lengthen its
    // last digit. Working at the byte level rather than through the writer is
    // deliberate: a tamper tool that re-encoded through the writer would only
    // ever produce streams the writer can produce.
    let at = certificate
        .iter()
        .position(|byte| byte.is_ascii_digit() && *byte != b'0')
        .expect("a certificate carries decimal digits");
    certificate[at] = if certificate[at] == b'9' {
        b'8'
    } else {
        certificate[at] + 1
    };

    let refusal = verify(&certificate, raw).expect_err("the tamper must be caught");
    // The checksum is the earliest rule that can see this, and it is allowed
    // to be the one that fires: it says the bytes changed, which is true.
    assert!(
        refusal.code.family() == "HCC-PARSE"
            || refusal.code.family() == "HCC-DUAL"
            || refusal.code.family() == "HCC-POINT"
            || refusal.code.family() == "HCC-CLAIM",
        "{name} gave {refusal}"
    );
}

#[test]
fn a_multiplier_edited_under_a_repaired_checksum_is_still_caught() {
    // The same edit with the checksum recomputed, so that the mathematics has
    // to be what refuses it. This is the case that matters: a forger with the
    // format specification can always repair a checksum.
    let (name, raw, _) = CORPUS[0];
    let mut certificate = certify(name, raw);
    let at = certificate
        .iter()
        .position(|byte| byte.is_ascii_digit() && *byte != b'0')
        .expect("a certificate carries decimal digits");
    certificate[at] = if certificate[at] == b'9' {
        b'8'
    } else {
        certificate[at] + 1
    };

    let split = certificate.len() - 4;
    let repaired = hc_verify::vhash::checksum(&certificate[..split]).to_le_bytes();
    certificate[split..].copy_from_slice(&repaired);

    let refusal = verify(&certificate, raw).expect_err("the tamper must be caught");
    assert_ne!(
        refusal.code.text(),
        "HCC-PARSE-09",
        "{name}: the checksum was repaired and still answered"
    );
}
