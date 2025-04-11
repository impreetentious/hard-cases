//! The verifier's LP reader against every frozen vector.
//!
//! The refusals matter more than the acceptances. An accepted file shows the
//! reader reads; a refused file shows a check exists, and names which one. The
//! manifest pins the identifier as well as the outcome, so a file caught by
//! the right rule for the wrong reason still fails here.

use hc_verify::vlp::read;
use hc_verify::vspec::Asset;
use hc_verify::{instance_digest, to_hex};

const MANIFEST: &str = include_str!("../../../spec/v1/vectors/lp/manifest.toml");

const ACCEPTED: &[(&str, &[u8], &str)] = &[
    (
        "tiny.lp",
        include_bytes!("../../../spec/v1/vectors/lp/tiny.lp"),
        include_str!("../../../spec/v1/vectors/lp/tiny.canon"),
    ),
    (
        "ranges.lp",
        include_bytes!("../../../spec/v1/vectors/lp/ranges.lp"),
        include_str!("../../../spec/v1/vectors/lp/ranges.canon"),
    ),
    (
        "exact.lp",
        include_bytes!("../../../spec/v1/vectors/lp/exact.lp"),
        include_str!("../../../spec/v1/vectors/lp/exact.canon"),
    ),
];

const REFUSED: &[(&str, &[u8])] = &[
    (
        "reject/missing-label.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/missing-label.lp"),
    ),
    (
        "reject/constant-in-constraint.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/constant-in-constraint.lp"),
    ),
    (
        "reject/reserved-word-as-name.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/reserved-word-as-name.lp"),
    ),
    (
        "reject/negative-upper-default-lower.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/negative-upper-default-lower.lp"),
    ),
    (
        "reject/duplicate-constraint-label.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/duplicate-constraint-label.lp"),
    ),
    (
        "reject/bound-on-undeclared.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/bound-on-undeclared.lp"),
    ),
    (
        "reject/duplicate-bound.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/duplicate-bound.lp"),
    ),
    (
        "reject/bounded-binary.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/bounded-binary.lp"),
    ),
    (
        "reject/section-out-of-order.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/section-out-of-order.lp"),
    ),
    (
        "reject/unknown-section.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/unknown-section.lp"),
    ),
    (
        "reject/missing-end.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/missing-end.lp"),
    ),
    (
        "reject/three-operator-row.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/three-operator-row.lp"),
    ),
    (
        "reject/two-signs.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/two-signs.lp"),
    ),
    (
        "reject/unsigned-infinity.lp",
        include_bytes!("../../../spec/v1/vectors/lp/reject/unsigned-infinity.lp"),
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
fn the_digest_is_over_the_tag_and_the_file_together() {
    for (name, raw, dump) in ACCEPTED {
        let stated = dump
            .lines()
            .find_map(|line| line.strip_prefix("digest "))
            .expect("a digest line");
        assert_eq!(to_hex(&instance_digest(3, raw)), stated, "{name}");
        assert_ne!(to_hex(&instance_digest(2, raw)), stated, "{name}");
    }
}
