//! The verifier's parser against every frozen instance vector.
//!
//! The refusals matter more than the acceptances. An accepted file shows the
//! parser reads; a refused file shows a check exists, and names which one. The
//! manifest pins the identifier as well as the outcome, so a file caught by
//! the right rule for the wrong reason still fails here.

use hc_verify::vparse::{Layout, read};
use hc_verify::vspec::Asset;
use hc_verify::{instance_digest, to_hex};

const MANIFEST: &str = include_str!("../../../spec/v1/vectors/mps/manifest.toml");

const ACCEPTED: &[(&str, &[u8], &str)] = &[
    (
        "tiny.mps",
        include_bytes!("../../../spec/v1/vectors/mps/tiny.mps"),
        include_str!("../../../spec/v1/vectors/mps/tiny.canon"),
    ),
    (
        "ranges.mps",
        include_bytes!("../../../spec/v1/vectors/mps/ranges.mps"),
        include_str!("../../../spec/v1/vectors/mps/ranges.canon"),
    ),
    (
        "exact.mps",
        include_bytes!("../../../spec/v1/vectors/mps/exact.mps"),
        include_str!("../../../spec/v1/vectors/mps/exact.canon"),
    ),
];

const REFUSED: &[(&str, &[u8])] = &[
    (
        "reject/unknown-bound-type.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/unknown-bound-type.mps"),
    ),
    (
        "reject/multiple-rhs-vectors.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/multiple-rhs-vectors.mps"),
    ),
    (
        "reject/no-objective-row.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/no-objective-row.mps"),
    ),
    (
        "reject/duplicate-row-name.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/duplicate-row-name.mps"),
    ),
    (
        "reject/negative-up-default-lower.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/negative-up-default-lower.mps"),
    ),
    (
        "reject/noninteger-ui.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/noninteger-ui.mps"),
    ),
    (
        "reject/ranges-on-objective.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/ranges-on-objective.mps"),
    ),
    (
        "reject/noncontiguous-column.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/noncontiguous-column.mps"),
    ),
    (
        "reject/unbalanced-marker.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/unbalanced-marker.mps"),
    ),
    (
        "reject/infinity-token.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/infinity-token.mps"),
    ),
    (
        "reject/underflow-value.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/underflow-value.mps"),
    ),
    (
        "reject/exponent-cap.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/exponent-cap.mps"),
    ),
    (
        "reject/unknown-section.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/unknown-section.mps"),
    ),
    (
        "reject/missing-endata.mps",
        include_bytes!("../../../spec/v1/vectors/mps/reject/missing-endata.mps"),
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
        let instance = match read(entry.1, Layout::Fixed) {
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
        let refusal = match read(raw, Layout::Fixed) {
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
fn the_digest_is_over_the_tag_and_the_file_together() {
    for (name, raw, dump) in ACCEPTED {
        let stated = dump
            .lines()
            .find_map(|line| line.strip_prefix("digest "))
            .expect("a digest line");
        assert_eq!(to_hex(&instance_digest(1, raw)), stated, "{name}");
        assert_ne!(to_hex(&instance_digest(2, raw)), stated, "{name}");
    }
}
