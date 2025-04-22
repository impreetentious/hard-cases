//! The trusted base, measured and held to its budget.
//!
//! Three claims about the verifier are made in prose everywhere in this
//! project: it is small, it has no dependencies, and it contains no unsafe
//! code. This is where those become measurements rather than assertions, and
//! where exceeding the published budget fails the build instead of appearing
//! in a footnote after release.

use std::path::PathBuf;

use hc_core::SpecDocument;
use hc_receipt::of;

const CAPS: &str = include_str!("../../../spec/v1/numeric.toml");

fn verifier_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates directory")
        .join("hc-verify")
}

fn budget(key: &str) -> i64 {
    SpecDocument::parse(CAPS)
        .expect("the caps asset is inside the subset")
        .table("caps.tcb")
        .expect("the asset states a trust-boundary budget")
        .integer(key)
        .unwrap_or_else(|| panic!("the asset states no {key}"))
}

#[test]
fn the_trusted_base_fits_inside_its_published_budget() {
    let receipt = of(&verifier_root());
    let limit = budget("verifier_loc");
    assert!(
        receipt.lines as i64 <= limit,
        "the verifier holds {} counted lines against a budget of {limit}\n{}",
        receipt.lines,
        receipt.render()
    );
    // A budget with nothing near it is a budget nobody is watching. This is
    // not a target and not a floor; it is a check that the measurement is
    // measuring something, because a receipt reporting a handful of lines
    // would mean the walk found almost nothing.
    assert!(
        receipt.lines > 1_000,
        "the receipt found only {} lines, which cannot be the whole crate",
        receipt.lines
    );
}

#[test]
fn the_trusted_base_declares_nothing_and_carries_no_unsafe_code() {
    let receipt = of(&verifier_root());
    assert_eq!(
        budget("dependencies"),
        0,
        "the asset should budget zero dependencies"
    );
    assert!(
        receipt.dependencies.is_empty(),
        "the verifier declares {:?}",
        receipt.dependencies
    );
    assert_eq!(
        receipt.unsafe_uses, 0,
        "the verifier qualifies something unsafe {} times",
        receipt.unsafe_uses
    );
    assert!(
        receipt.forbids_unsafe,
        "the verifier's crate root does not forbid unsafe code"
    );
}

#[test]
fn the_receipt_names_every_file_and_hashes_the_source_it_measured() {
    let receipt = of(&verifier_root());
    assert!(
        receipt.files.len() >= 10,
        "the walk found only {} files",
        receipt.files.len()
    );
    assert!(receipt.files.contains_key("src/lib.rs"));
    assert!(receipt.files.contains_key("src/vint/nat.rs"));
    assert!(receipt.files.contains_key("src/bin/hc-verify.rs"));

    // The hash is over the source that did the checking, so it must change
    // when the source does and must not depend on the order of a directory
    // walk.
    assert_ne!(receipt.source, [0; 32]);
    assert_eq!(receipt.source, of(&verifier_root()).source);

    let rendered = receipt.render();
    assert!(rendered.starts_with("trust boundary\n"));
    assert!(rendered.contains("  dependencies   0\n"));
    assert!(rendered.contains("  unsafe         0\n"));
    assert!(rendered.contains("  forbids unsafe yes\n"));
}

#[test]
fn test_code_is_counted_separately_rather_than_hidden() {
    let receipt = of(&verifier_root());
    // Both numbers are published. A trusted base that reported only its
    // non-test lines would be reporting a smaller number without saying so.
    assert!(receipt.test_lines > 0, "the verifier has tests");
    assert!(receipt.render().contains("  test lines     "));
}
