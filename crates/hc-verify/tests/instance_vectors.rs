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

// ---------------------------------------------------------------------------
// The traps the manifest names
// ---------------------------------------------------------------------------
//
// The walks above run every entry, so these prove nothing the suite did not
// already prove. What they add is the other end of a contract: the manifest
// names a trap beside a rule, and a reader who greps for that name has to land
// on the assertion that makes it true rather than on a loop that happens to
// include it. Each of these runs exactly the entry its name is attached to, so
// an entry quietly removed from the manifest fails here by name.

/// Reads the one accepted vector a manifest table names and returns its dump.
fn accepted(table: &str) -> String {
    let asset = manifest();
    let file = field(&asset, table, "file");
    let entry = ACCEPTED
        .iter()
        .find(|(name, _, _)| *name == file)
        .unwrap_or_else(|| panic!("[{table}] names {file}, which is not compiled in"));
    read(entry.1, Layout::Fixed)
        .unwrap_or_else(|error| panic!("[{table}] {file} was refused: {error}"))
        .dump()
}

/// Runs the one refused vector a manifest table names and returns its rule.
fn refused(table: &str) -> String {
    let asset = manifest();
    let file = field(&asset, table, "file");
    let raw = REFUSED
        .iter()
        .find(|(name, _)| *name == file)
        .map(|(_, raw)| *raw)
        .unwrap_or_else(|| panic!("[{table}] names {file}, which is not compiled in"));
    match read(raw, Layout::Fixed) {
        Err(refusal) => refusal.code.text().to_owned(),
        Ok(_) => panic!("[{table}] {file} was accepted and must not be"),
    }
}

/// The range table is the single most consequential disagreement between
/// readers of this format, and the vector is the entry that would notice.
#[test]
fn trap_free_mps_ranges() {
    let dump = accepted("accept.ranges");
    assert!(dump.contains("digest "), "the dump is a canonical dump");
    assert_eq!(
        dump,
        include_str!("../../../spec/v1/vectors/mps/ranges.canon"),
        "the range table renders something other than the frozen dump"
    );
}

/// Values come from the decimal text of the file, not from a float round trip.
#[test]
fn trap_exact_from_text() {
    assert_eq!(
        accepted("accept.exact"),
        include_str!("../../../spec/v1/vectors/mps/exact.canon"),
        "an exact value did not survive the reader"
    );
}

/// A bound type outside the accepted list is refused rather than guessed at.
#[test]
fn trap_mps_unsupported_bound_type() {
    assert_eq!(
        refused("reject.unknown_bound_type"),
        field(&manifest(), "reject.unknown_bound_type", "id")
    );
}

/// A second distinct vector name is refused rather than silently chosen between.
#[test]
fn trap_mps_multiple_vectors() {
    assert_eq!(
        refused("reject.multiple_rhs_vectors"),
        field(&manifest(), "reject.multiple_rhs_vectors", "id")
    );
}

/// A numeric token naming a value wider than the cap is refused before it is
/// projected onto anything.
#[test]
fn trap_numeric_exponent_cap() {
    assert_eq!(
        refused("reject.exponent_cap"),
        field(&manifest(), "reject.exponent_cap", "id")
    );
}
