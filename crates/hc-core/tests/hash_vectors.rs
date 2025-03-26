//! The hashes against their frozen vectors.
//!
//! Both functions are published and both have well-known test values, which
//! is exactly why they are written from scratch here: a few hundred lines with
//! an unarguable specification is the wrong place to acquire a dependency that
//! would then sit on both sides of the proof boundary.
//!
//! The vectors include the certificate magic and the project name alongside
//! the published cases. Those are the strings a header vector has to be
//! assembled from by hand, and having them checked here means a hand-worked
//! certificate can be built without running the code that will read it.

use hc_core::{SpecDocument, SpecTable, crc32c, hex, instance_digest, sha256};

const HASHES: &str = include_str!("../../../spec/v1/vectors/hash/hashes.toml");
const TINY_INSTANCE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");
const GENERATION: &str = include_str!("../../../spec/v1/rng.toml");

fn document() -> SpecDocument {
    SpecDocument::parse(HASHES).expect("the hash vectors are inside the subset")
}

/// The message a vector names, with any repeat applied.
fn message(table: &SpecTable) -> Vec<u8> {
    let input = table
        .text("input")
        .unwrap_or_else(|| panic!("[{}] has no input", table.name()));
    let repeat = table.integer("repeat").unwrap_or(1);
    assert!(repeat >= 1, "[{}] has a nonpositive repeat", table.name());
    input.as_bytes().repeat(repeat as usize)
}

#[test]
fn every_digest_vector_holds() {
    let document = document();
    let mut seen = 0usize;
    for table in document.tables_under("sha256") {
        let expected = table
            .text("digest")
            .unwrap_or_else(|| panic!("[{}] has no digest", table.name()));
        assert_eq!(
            hex(&sha256(&message(table))),
            expected,
            "[{}]",
            table.name()
        );
        assert!(
            table.text("covers").is_some(),
            "[{}] states no rationale",
            table.name()
        );
        seen += 1;
    }
    assert!(seen >= 5, "only {seen} digest vectors were read");
}

#[test]
fn every_checksum_vector_holds() {
    let document = document();
    let mut seen = 0usize;
    for table in document.tables_under("crc32c") {
        let expected = table
            .text("crc")
            .unwrap_or_else(|| panic!("[{}] has no crc", table.name()));
        let digits = expected
            .strip_prefix("0x")
            .unwrap_or_else(|| panic!("[{}] crc is not hexadecimal", table.name()));
        let wanted = u32::from_str_radix(digits, 16).expect("a hexadecimal word");
        assert_eq!(crc32c(&message(table)), wanted, "[{}]", table.name());
        assert!(
            table.text("covers").is_some(),
            "[{}] states no rationale",
            table.name()
        );
        seen += 1;
    }
    assert!(seen >= 5, "only {seen} checksum vectors were read");
}

#[test]
fn the_checksum_parameters_are_the_ones_the_asset_names() {
    // A checksum is a family, not a function. Reproducing one published value
    // pins the family only if the parameters are pinned too, so they are
    // stated in the asset and read back here.
    let document = document();
    let table = document
        .table("crc32c")
        .expect("the parameter table is present");
    assert_eq!(table.text("polynomial"), Some("0x82f63b78"));
    assert_eq!(table.text("initial"), Some("0xffffffff"));
    assert_eq!(table.text("final_xor"), Some("0xffffffff"));
    assert_eq!(
        table
            .get("reflected")
            .and_then(hc_core::SpecValue::as_boolean),
        Some(true)
    );
}

#[test]
fn the_instance_vector_reaches_its_seed_word_from_the_file_on_disk() {
    // The end-to-end case: bytes, through the format tag and the digest, to
    // the little-endian read that starts the generator. Every step is
    // checkable alone; this checks that they compose.
    let generation = SpecDocument::parse(GENERATION).expect("inside the subset");
    let vector = generation
        .table("vector.from_instance")
        .expect("the vector is present");

    let tag = vector.integer("format_tag").expect("a format tag") as u8;
    let digest = instance_digest(tag, TINY_INSTANCE);
    assert_eq!(
        hex(&digest),
        vector.text("instance_digest").expect("a stated digest"),
        "digest of the instance file"
    );

    let mut head = [0u8; 8];
    head.copy_from_slice(&digest[..8]);
    let user_seed = vector.integer("user_seed").expect("a user seed") as u64;
    let seed = u64::from_le_bytes(head) ^ user_seed;
    let stated = vector.text("seed64").expect("a stated seed word");
    let wanted = u64::from_str_radix(stated.strip_prefix("0x").expect("hexadecimal"), 16)
        .expect("a hexadecimal word");
    assert_eq!(seed, wanted, "seed word from the digest");
}

#[test]
fn the_manifest_digest_of_every_accepted_instance_matches() {
    // The parser vectors state a digest for each accepted file. Only the file
    // this crate compiles in can be checked here; the rest are checked when
    // the reader that consumes them exists. The count is asserted so that a
    // file added to the manifest is not silently unchecked.
    let manifest = SpecDocument::parse(include_str!("../../../spec/v1/vectors/mps/manifest.toml"))
        .expect("inside the subset");
    let tag = manifest
        .table("meta")
        .and_then(|meta| meta.integer("format_tag"))
        .expect("a tag");
    let tiny = manifest
        .table("accept.tiny")
        .expect("the smallest accepted vector");
    assert_eq!(tiny.text("file"), Some("tiny.mps"));
    assert_eq!(
        hex(&instance_digest(tag as u8, TINY_INSTANCE)),
        tiny.text("digest").expect("a stated digest")
    );
    assert_eq!(manifest.tables_under("accept").count(), 3);
}
