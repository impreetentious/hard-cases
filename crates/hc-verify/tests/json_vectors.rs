//! The verifier's JSON reader against every frozen vector.
//!
//! The refusals matter more than the acceptances. An accepted document shows
//! the reader reads; a rejected one shows a check exists, and names which one.

use hc_verify::vjson::read;
use hc_verify::vspec::Asset;
use hc_verify::{instance_digest, to_hex};

const MANIFEST: &str = include_str!("../../../spec/v1/vectors/json/manifest.toml");

const ACCEPTED: &[(&str, &[u8], &str)] = &[
    (
        "tiny.json",
        include_bytes!("../../../spec/v1/vectors/json/tiny.json"),
        include_str!("../../../spec/v1/vectors/json/tiny.canon"),
    ),
    (
        "ranges.json",
        include_bytes!("../../../spec/v1/vectors/json/ranges.json"),
        include_str!("../../../spec/v1/vectors/json/ranges.canon"),
    ),
    (
        "exact.json",
        include_bytes!("../../../spec/v1/vectors/json/exact.json"),
        include_str!("../../../spec/v1/vectors/json/exact.canon"),
    ),
];

const REFUSED: &[(&str, &[u8])] = &[
    (
        "reject/float-value.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/float-value.json"),
    ),
    (
        "reject/unknown-member.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/unknown-member.json"),
    ),
    (
        "reject/missing-member.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/missing-member.json"),
    ),
    (
        "reject/duplicate-member.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/duplicate-member.json"),
    ),
    (
        "reject/index-out-of-range.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/index-out-of-range.json"),
    ),
    (
        "reject/free-row-sense.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/free-row-sense.json"),
    ),
    (
        "reject/wrong-version.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/wrong-version.json"),
    ),
    (
        "reject/no-variables.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/no-variables.json"),
    ),
    (
        "reject/trailing-comma.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/trailing-comma.json"),
    ),
    (
        "reject/unicode-escape.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/unicode-escape.json"),
    ),
    (
        "reject/duplicate-variable-name.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/duplicate-variable-name.json"),
    ),
    (
        "reject/duplicate-row-name.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/duplicate-row-name.json"),
    ),
    (
        "reject/long-name.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/long-name.json"),
    ),
    (
        "reject/bad-number.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/bad-number.json"),
    ),
    (
        "reject/underflow-value.json",
        include_bytes!("../../../spec/v1/vectors/json/reject/underflow-value.json"),
    ),
];

fn manifest() -> Asset {
    Asset::read(MANIFEST).expect("the manifest is inside the subset")
}

fn field(asset: &Asset, table: &str, key: &str) -> String {
    asset
        .text(table, key)
        .unwrap_or_else(|| panic!("[{table}] has no {key}"))
        .to_owned()
}

#[test]
fn every_accepted_vector_renders_its_frozen_dump() {
    let asset = manifest();
    let mut seen = 0usize;
    for table in asset.tables_under("accept") {
        let file = field(&asset, table, "file");
        let entry = ACCEPTED
            .iter()
            .find(|(name, _, _)| *name == file)
            .unwrap_or_else(|| panic!("[{table}] names {file}, which is not compiled in"));
        let instance = match read(entry.1) {
            Ok(instance) => instance,
            Err(error) => panic!("[{table}] {file} was refused: {error}"),
        };
        assert_eq!(instance.dump(), entry.2, "[{table}] {file}");
        assert_eq!(
            to_hex(&instance.digest),
            field(&asset, table, "digest"),
            "[{table}] digest"
        );
        seen += 1;
    }
    assert_eq!(seen, ACCEPTED.len());
}

#[test]
fn every_refused_vector_is_refused_by_the_named_rule() {
    let asset = manifest();
    let mut seen = 0usize;
    for table in asset.tables_under("reject") {
        let file = field(&asset, table, "file");
        let expected = field(&asset, table, "id");
        let raw = REFUSED
            .iter()
            .find(|(name, _)| *name == file)
            .map(|(_, raw)| *raw)
            .unwrap_or_else(|| panic!("[{table}] names {file}, which is not compiled in"));
        let refusal = match read(raw) {
            Err(refusal) => refusal,
            Ok(_) => panic!("[{table}] {file} was accepted and must not be"),
        };
        assert_eq!(
            refusal.code.text(),
            expected,
            "[{table}] {file} was refused by the wrong rule: {refusal}"
        );
        seen += 1;
    }
    assert_eq!(seen, REFUSED.len());
}

#[test]
fn the_digest_is_over_the_tag_and_the_document_together() {
    for (name, raw, dump) in ACCEPTED {
        let stated = dump
            .lines()
            .find_map(|line| line.strip_prefix("digest "))
            .expect("a digest line");
        assert_eq!(to_hex(&instance_digest(4, raw)), stated, "{name}");
        assert_ne!(to_hex(&instance_digest(3, raw)), stated, "{name}");
    }
}
