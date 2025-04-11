//! The JSON reader against every frozen vector.
//!
//! An accepted document names the canonical dump the reader must produce, byte
//! for byte. A rejected one names the single identifier it must produce, and
//! that half of the corpus matters more: an accepted document shows that a
//! reader works, and a rejected one shows that a check exists at all.
//!
//! The manifest is walked rather than transcribed, so a vector added there
//! without a file compiled in here fails rather than passing unnoticed.

use hc_core::{FormatTag, SpecDocument, SpecTable, hex, instance_digest};
use hc_io::json::read;

const MANIFEST: &str = include_str!("../../../spec/v1/vectors/json/manifest.toml");

/// The LP vectors carrying the same three models.
const LP_DUMPS: &[(&str, &str)] = &[
    (
        "tiny.json",
        include_str!("../../../spec/v1/vectors/lp/tiny.canon"),
    ),
    (
        "ranges.json",
        include_str!("../../../spec/v1/vectors/lp/ranges.canon"),
    ),
    (
        "exact.json",
        include_str!("../../../spec/v1/vectors/lp/exact.canon"),
    ),
];

/// Every accepted document, with the dump it must produce.
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

/// Every document the manifest expects a refusal for.
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
        let model = match read(expected.1) {
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
        let refusal = match read(raw) {
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
fn the_digest_covers_the_document_and_the_tag_together() {
    for (name, raw, dump) in ACCEPTED {
        let stated = dump
            .lines()
            .find_map(|line| line.strip_prefix("digest "))
            .expect("a digest line");
        assert_eq!(
            hex(&instance_digest(FormatTag::Json.byte(), raw)),
            stated,
            "{name}"
        );
        assert_ne!(
            hex(&instance_digest(FormatTag::Lp.byte(), raw)),
            stated,
            "{name} hashes the same whichever format read it"
        );
    }
}

#[test]
fn one_model_written_two_ways_dumps_the_same_below_its_digest() {
    // Three models, each written once in LP format and once here. The first
    // three lines carry the magic, the format tag, and the digest, which are
    // properties of a file. Everything after them is the model, which is not.
    let below = |text: &str| text.lines().skip(3).collect::<Vec<_>>().join("\n");
    for (name, lp) in LP_DUMPS {
        let json = ACCEPTED
            .iter()
            .find(|(file, _, _)| file == name)
            .unwrap_or_else(|| panic!("{name} is not compiled in"));
        assert_eq!(below(json.2), below(lp), "{name}");
    }
}

#[test]
fn the_manifest_names_nothing_the_harness_cannot_reach() {
    // A manifest entry pointing at a document nobody reads is a vector that
    // covers nothing, which is the failure this corpus exists to prevent.
    let manifest = manifest();
    for table in manifest
        .tables_under("accept")
        .chain(manifest.tables_under("reject"))
    {
        let _ = bytes_of(named(table, "file"));
    }
    assert!(manifest.tables_under("accept").count() >= 3);
    assert!(manifest.tables_under("reject").count() >= 15);
}
