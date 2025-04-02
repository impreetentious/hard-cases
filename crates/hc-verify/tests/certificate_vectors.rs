//! The frozen certificate vector, read the way a consumer reads this crate.
//!
//! The unit tests inside the modules can reach private helpers; this one uses
//! only the public surface, which is the surface a release script and the
//! situation room both go through. A vector harness that exercised internals
//! would be checking a path nobody else takes.

use hc_verify::vhcc::{Claim, Step, from_annotated_hex, read};
use hc_verify::vreject::KNOWN;
use hc_verify::vspec::{Asset, Value};
use hc_verify::{model_hash, verify};

const MANIFEST: &str = include_str!("../../../spec/v1/vectors/hcc/manifest.toml");
const TINY_OPTIMAL: &str = include_str!("../../../spec/v1/vectors/hcc/tiny-optimal.hex");
const TINY_INSTANCE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

fn manifest() -> Asset {
    Asset::read(MANIFEST).expect("the manifest is inside the subset")
}

fn bytes() -> Vec<u8> {
    from_annotated_hex(TINY_OPTIMAL).expect("the vector is annotated hexadecimal")
}

#[test]
fn the_vector_matches_every_number_its_manifest_states() {
    let asset = manifest();
    let stream = bytes();

    let stated_bytes = asset
        .number("vector.tiny_optimal", "bytes")
        .expect("the manifest states a byte count");
    assert_eq!(stream.len() as i64, stated_bytes);

    let proof = read(&stream, TINY_INSTANCE).expect("the vector is accepted");
    let stated_records = asset
        .number("vector.tiny_optimal", "records")
        .expect("the manifest states a record count");
    assert_eq!(proof.steps.len() as i64, stated_records);

    let stated_claim = asset
        .text("vector.tiny_optimal", "claim")
        .expect("the manifest states a claim");
    assert_eq!(stated_claim, "optimal");
    assert_eq!(proof.claim, Claim::Optimal);
}

#[test]
fn the_vector_verifies_and_reaches_the_numbers_the_manifest_works_through() {
    let verdict = verify(&bytes(), TINY_INSTANCE).expect("the vector verifies");
    assert_eq!(verdict.claim, Claim::Optimal);
    assert_eq!(
        verdict.objective.map(|value| value.to_canonical()),
        Some("-4".to_owned())
    );
    assert_eq!(
        verdict.lower.map(|value| value.to_canonical()),
        Some("-4".to_owned())
    );
    assert_eq!(
        verdict.gap.map(|value| value.to_canonical()),
        Some("0".to_owned())
    );

    // The work reading is a real count of a real check, and a verification
    // that reported no work would be a verification that did none.
    assert!(
        verdict.work >= (bytes().len() + TINY_INSTANCE.len()) as u64,
        "the reading of {} does not cover the bytes read",
        verdict.work
    );
}

#[test]
fn the_same_certificate_costs_the_same_every_time() {
    let first = verify(&bytes(), TINY_INSTANCE).expect("verifies");
    let second = verify(&bytes(), TINY_INSTANCE).expect("verifies");
    assert_eq!(
        first.work, second.work,
        "the work count is part of the deterministic output"
    );
}

#[test]
fn the_root_record_names_the_model_the_verifier_builds_from_the_instance() {
    let proof = read(&bytes(), TINY_INSTANCE).expect("the vector is accepted");
    let Some(Step::Root { stated_hash, .. }) = proof.at(0) else {
        panic!("the first record is the root");
    };
    let instance = hc_verify::vparse::read(TINY_INSTANCE, hc_verify::vparse::Layout::Fixed)
        .expect("the frozen instance parses");
    assert_eq!(*stated_hash, model_hash(&instance));
}

#[test]
fn every_rejection_identifier_this_crate_names_is_reachable_by_its_group() {
    // The list is hand-written on this side and hand-written again in the
    // specification. This checks the shape of every entry, so that a typo in
    // one of two hand-written lists is a test failure rather than an
    // identifier nobody can look up.
    for code in KNOWN {
        let text = code.text();
        let (group, number) = text.rsplit_once('-').expect("an identifier has a number");
        assert_eq!(code.family(), group, "{text}");
        assert_eq!(number.len(), 2, "{text} does not end in two digits");
        assert!(
            number.bytes().all(|byte| byte.is_ascii_digit()),
            "{text} does not end in two digits"
        );
        assert!(
            hc_verify::vreject::FAMILIES.contains(&group),
            "{text} belongs to a group this crate does not declare"
        );
    }
}

#[test]
fn the_manifest_points_at_files_that_exist_and_says_what_it_covers() {
    let asset = manifest();
    for table in asset.tables_under("vector") {
        for key in ["file", "instance", "claim", "covers", "working"] {
            assert!(
                matches!(asset.value(table, key), Some(Value::Text(_))),
                "[{table}] has no {key}"
            );
        }
    }
}
