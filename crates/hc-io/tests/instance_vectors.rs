//! The reader against every frozen instance vector.
//!
//! An accepted file names the canonical dump the reader must produce, byte for
//! byte. A refused file names the single identifier the reader must produce,
//! and that half of the corpus matters more: an accepted-file vector shows
//! that a reader works, and a refused-file vector shows that a check exists at
//! all.
//!
//! Every vector in the manifest is exercised. The manifest is walked rather
//! than transcribed, so a vector added there without a file compiled in here
//! fails rather than passing unnoticed.

use hc_core::{FormatTag, SpecDocument, SpecTable, hex, instance_digest};
use hc_io::mps::{Form, read};

const MANIFEST: &str = include_str!("../../../spec/v1/vectors/mps/manifest.toml");

/// Every instance file the manifest can name, with its expected dump.
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

/// Every file the manifest expects a refusal for.
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

fn manifest() -> SpecDocument {
    SpecDocument::parse(MANIFEST).expect("the manifest is inside the subset")
}

fn named<'a>(table: &'a SpecTable, key: &str) -> &'a str {
    table
        .text(key)
        .unwrap_or_else(|| panic!("[{}] has no {key}", table.name()))
}

fn bytes_of(name: &str) -> &'static [u8] {
    ACCEPTED
        .iter()
        .find(|(file, _, _)| *file == name)
        .map(|(_, raw, _)| *raw)
        .or_else(|| {
            REFUSED
                .iter()
                .find(|(file, _)| *file == name)
                .map(|(_, raw)| *raw)
        })
        .unwrap_or_else(|| panic!("the manifest names {name}, which is not compiled in"))
}

#[test]
fn every_accepted_vector_produces_its_frozen_dump() {
    let manifest = manifest();
    let mut seen = 0usize;
    for table in manifest.tables_under("accept") {
        let file = named(table, "file");
        let expected = ACCEPTED
            .iter()
            .find(|(name, _, _)| *name == file)
            .unwrap_or_else(|| panic!("[{}] names {file}, which is not compiled in", table.name()));
        let model = match read(expected.1, Form::Fixed) {
            Ok(model) => model,
            Err(refusal) => panic!("[{}] {file} was refused: {refusal}", table.name()),
        };
        assert_eq!(
            model.canonical_dump(),
            expected.2,
            "[{}] {file}",
            table.name()
        );
        assert_eq!(
            hex(model.digest()),
            named(table, "digest"),
            "[{}] {file} digest",
            table.name()
        );
        seen += 1;
    }
    assert_eq!(
        seen,
        ACCEPTED.len(),
        "the manifest and the compiled list disagree"
    );
}

#[test]
fn every_refused_vector_is_refused_by_the_rule_the_manifest_names() {
    let manifest = manifest();
    let mut seen = 0usize;
    for table in manifest.tables_under("reject") {
        let file = named(table, "file");
        let expected = named(table, "id");
        let raw = bytes_of(file);
        let refusal = match read(raw, Form::Fixed) {
            Err(refusal) => refusal,
            Ok(_) => panic!("[{}] {file} was accepted and must not be", table.name()),
        };
        assert_eq!(
            refusal.id.as_str(),
            expected,
            "[{}] {file} was refused by the wrong rule: {refusal}",
            table.name()
        );
        seen += 1;
    }
    assert_eq!(
        seen,
        REFUSED.len(),
        "the manifest and the compiled list disagree"
    );
}

#[test]
fn the_digest_covers_the_file_and_the_tag_together() {
    for (name, raw, dump) in ACCEPTED {
        let stated = dump
            .lines()
            .find_map(|line| line.strip_prefix("digest "))
            .expect("a digest line");
        assert_eq!(
            hex(&instance_digest(FormatTag::MpsFixed.byte(), raw)),
            stated,
            "{name}"
        );
        // The same bytes under the other variant's rules are a different
        // instance, and the dump says so on its second line.
        assert_ne!(
            hex(&instance_digest(FormatTag::MpsFree.byte(), raw)),
            stated,
            "{name} hashes the same under both variants"
        );
    }
}

#[test]
fn the_manifest_names_nothing_the_harness_cannot_reach() {
    // A manifest entry pointing at a file nobody reads is a vector that covers
    // nothing, which is the failure this whole corpus exists to prevent.
    let manifest = manifest();
    for table in manifest
        .tables_under("accept")
        .chain(manifest.tables_under("reject"))
    {
        let _ = bytes_of(named(table, "file"));
    }
    assert!(manifest.tables_under("accept").count() >= 3);
    assert!(manifest.tables_under("reject").count() >= 14);
}
