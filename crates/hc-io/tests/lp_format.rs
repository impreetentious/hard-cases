//! The LP reader against every frozen vector.
//!
//! An accepted file names the canonical dump the reader must produce, byte for
//! byte. A refused file names the single identifier the reader must produce,
//! and that half of the corpus matters more: an accepted-file vector shows
//! that a reader works, and a refused-file vector shows that a check exists at
//! all.
//!
//! The manifest is walked rather than transcribed, so a vector added there
//! without a file compiled in here fails rather than passing unnoticed.

use hc_core::{FormatTag, Rejection, SpecDocument, SpecTable, hex, instance_digest};
use hc_io::lp::{Options, read, read_with};

const MANIFEST: &str = include_str!("../../../spec/v1/vectors/lp/manifest.toml");

/// The MPS vector whose model the LP vector of the same name repeats.
const EXACT_MPS_DUMP: &str = include_str!("../../../spec/v1/vectors/mps/exact.canon");

/// Every accepted file, with the dump it must produce.
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

/// Every file the manifest expects a refusal for.
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
fn the_digest_covers_the_file_and_the_tag_together() {
    for (name, raw, dump) in ACCEPTED {
        let stated = dump
            .lines()
            .find_map(|line| line.strip_prefix("digest "))
            .expect("a digest line");
        assert_eq!(
            hex(&instance_digest(FormatTag::Lp.byte(), raw)),
            stated,
            "{name}"
        );
        // The same bytes read under another format would be another instance.
        // The tag is inside the hash for exactly that reason.
        assert_ne!(
            hex(&instance_digest(FormatTag::MpsFree.byte(), raw)),
            stated,
            "{name} hashes the same whichever format read it"
        );
    }
}

#[test]
fn one_model_written_two_ways_dumps_the_same_below_its_digest() {
    // The round trip the two formats have to satisfy, checked as bytes. The
    // first three lines carry the magic, the format tag, and the digest, which
    // are properties of the file. Everything after them is the model, which is
    // not.
    let lp = ACCEPTED
        .iter()
        .find(|(name, _, _)| *name == "exact.lp")
        .expect("the exact vector");
    let below = |text: &str| text.lines().skip(3).collect::<Vec<_>>().join("\n");
    assert_eq!(below(lp.2), below(EXACT_MPS_DUMP));
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

#[test]
fn trap_lp_default_bounds() {
    // The fork this format shares with MPS: an upper bound below the default
    // lower bound of zero, on a variable whose lower bound nobody gave. Strict
    // refuses it; lenient reads it the way the other camp does and says so.
    let raw = bytes_of("reject/negative-upper-default-lower.lp");
    let refusal = read(raw).expect_err("strict refuses the fork");
    assert_eq!(refusal.id, Rejection::INSTANCE_AMBIGUOUS_NEGATIVE_UPPER);

    let parsed = read_with(raw, Options::lenient()).expect("lenient resolves it");
    assert_eq!(parsed.forgiven.len(), 1, "{:?}", parsed.forgiven);
    assert!(parsed.forgiven[0].contains("removing the lower bound"));
    assert_eq!(parsed.model.variables()[0].lower, None);

    // Giving the lower bound first removes the ambiguity, and then both modes
    // agree with nothing forgiven.
    let given = b"Minimize\n obj: x\nSubject To\n c1: x >= -8\nBounds\n x >= -9\n x <= -2\nEnd\n";
    let strict = read(given).expect("no ambiguity left");
    assert_eq!(
        strict.variables()[0]
            .upper
            .as_ref()
            .map(hc_exact::Rational::to_canonical_string),
        Some("-2".to_owned())
    );
    assert!(
        read_with(given, Options::lenient())
            .expect("also accepted")
            .forgiven
            .is_empty()
    );
}
