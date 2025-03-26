//! The verifier's hashes against the same frozen vectors the solver's are held
//! to, read by the verifier's own asset reader.
//!
//! The end-to-end case matters most: the bytes of an instance file on disk,
//! through the format tag, to the digest a certificate header has to carry.
//! That is the step where a certificate stops being about a model in memory
//! and starts being about a file, and it is checked here against a value
//! written down before either side existed.

use hc_verify::vspec::{Asset, Value};
use hc_verify::{checksum, digest, instance_digest, to_hex};

const HASHES: &str = include_str!("../../../spec/v1/vectors/hash/hashes.toml");
const GENERATION: &str = include_str!("../../../spec/v1/rng.toml");
const MANIFEST: &str = include_str!("../../../spec/v1/vectors/mps/manifest.toml");
const TINY_INSTANCE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

fn vectors() -> Asset {
    Asset::read(HASHES).expect("the hash vectors are inside the subset")
}

/// The message a vector names, with any repeat applied.
fn message(asset: &Asset, table: &str) -> Vec<u8> {
    let input = asset
        .text(table, "input")
        .unwrap_or_else(|| panic!("[{table}] has no input"));
    let repeat = asset.number(table, "repeat").unwrap_or(1);
    assert!(repeat >= 1, "[{table}] has a nonpositive repeat");
    input.as_bytes().repeat(repeat as usize)
}

fn hex_word(text: &str) -> u32 {
    u32::from_str_radix(text.strip_prefix("0x").expect("a hexadecimal word"), 16)
        .expect("a hexadecimal word")
}

#[test]
fn every_digest_vector_holds() {
    let asset = vectors();
    let mut seen = 0usize;
    for table in asset.tables_under("sha256") {
        let expected = asset
            .text(table, "digest")
            .unwrap_or_else(|| panic!("[{table}] has no digest"));
        assert_eq!(
            to_hex(&digest(&message(&asset, table))),
            expected,
            "[{table}]"
        );
        seen += 1;
    }
    assert!(seen >= 5, "only {seen} digest vectors were read");
}

#[test]
fn every_checksum_vector_holds() {
    let asset = vectors();
    let mut seen = 0usize;
    for table in asset.tables_under("crc32c") {
        let expected = asset
            .text(table, "crc")
            .unwrap_or_else(|| panic!("[{table}] has no crc"));
        assert_eq!(
            checksum(&message(&asset, table)),
            hex_word(expected),
            "[{table}]"
        );
        seen += 1;
    }
    assert!(seen >= 5, "only {seen} checksum vectors were read");
}

#[test]
fn the_checksum_family_is_pinned_by_its_parameters() {
    let asset = vectors();
    assert_eq!(asset.text("crc32c", "polynomial"), Some("0x82f63b78"));
    assert_eq!(asset.text("crc32c", "initial"), Some("0xffffffff"));
    assert_eq!(asset.text("crc32c", "final_xor"), Some("0xffffffff"));
    assert_eq!(asset.value("crc32c", "reflected"), Some(&Value::Flag(true)));
}

#[test]
fn the_instance_file_hashes_to_the_digest_both_vectors_state() {
    let generation = Asset::read(GENERATION).expect("inside the subset");
    let tag = generation
        .number("vector.from_instance", "format_tag")
        .expect("a tag");
    let computed = instance_digest(tag as u8, TINY_INSTANCE);
    assert_eq!(
        to_hex(&computed),
        generation
            .text("vector.from_instance", "instance_digest")
            .expect("a stated digest")
    );

    // The parser vectors state the same digest for the same file, through a
    // different asset. Both are checked so that a change to one of them cannot
    // pass unnoticed by agreeing with nothing.
    let manifest = Asset::read(MANIFEST).expect("inside the subset");
    let manifest_tag = manifest.number("meta", "format_tag").expect("a tag");
    assert_eq!(
        manifest_tag, tag,
        "the two assets disagree about the format tag"
    );
    assert_eq!(
        to_hex(&computed),
        manifest
            .text("accept.tiny", "digest")
            .expect("a stated digest")
    );
}

#[test]
fn the_seed_word_follows_from_the_digest() {
    let generation = Asset::read(GENERATION).expect("inside the subset");
    let tag = generation
        .number("vector.from_instance", "format_tag")
        .expect("a tag");
    let computed = instance_digest(tag as u8, TINY_INSTANCE);

    let mut head = [0u8; 8];
    head.copy_from_slice(&computed[..8]);
    let user_seed = generation
        .number("vector.from_instance", "user_seed")
        .expect("a seed") as u64;
    let seed = u64::from_le_bytes(head) ^ user_seed;

    let stated = generation
        .text("vector.from_instance", "seed64")
        .expect("a stated seed");
    let wanted = u64::from_str_radix(stated.strip_prefix("0x").expect("hexadecimal"), 16)
        .expect("a hexadecimal word");
    assert_eq!(seed, wanted);
}
