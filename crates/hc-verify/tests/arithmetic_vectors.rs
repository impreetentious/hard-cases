//! The verifier's arithmetic against the same frozen vectors the solver's is
//! held to.
//!
//! Same file, read by a different reader, checked by a different arithmetic.
//! Neither implementation is the other's oracle and neither generated the
//! numbers; the only thing they have in common is this asset, which is the
//! arrangement the whole project rests on.
//!
//! The assets are compiled in rather than read from disk. This crate does not
//! touch the filesystem — a verifier that reaches for a file it was not handed
//! is a verifier with an input nobody audited — so `include_str!` is how inert
//! data reaches a test here at all.

use core::cmp::Ordering;

use hc_verify::vspec::Asset;
use hc_verify::{Frac, Int, Nat};

const ARITHMETIC: &str = include_str!("../../../spec/v1/vectors/arith/arithmetic.toml");

/// Every asset in the specification directory, read by this side's reader.
const ASSETS: &[(&str, &str)] = &[
    (
        "canonical.toml",
        include_str!("../../../spec/v1/canonical.toml"),
    ),
    ("claims.toml", include_str!("../../../spec/v1/claims.toml")),
    ("cuts.toml", include_str!("../../../spec/v1/cuts.toml")),
    ("hcc.toml", include_str!("../../../spec/v1/hcc.toml")),
    ("mps.toml", include_str!("../../../spec/v1/mps.toml")),
    (
        "numeric.toml",
        include_str!("../../../spec/v1/numeric.toml"),
    ),
    (
        "rejections.toml",
        include_str!("../../../spec/v1/rejections.toml"),
    ),
    ("rng.toml", include_str!("../../../spec/v1/rng.toml")),
    (
        "transforms.toml",
        include_str!("../../../spec/v1/transforms.toml"),
    ),
    ("vectors/arith/arithmetic.toml", ARITHMETIC),
    (
        "vectors/hash/hashes.toml",
        include_str!("../../../spec/v1/vectors/hash/hashes.toml"),
    ),
    (
        "vectors/mps/manifest.toml",
        include_str!("../../../spec/v1/vectors/mps/manifest.toml"),
    ),
    (
        "vectors/proof/inferences.toml",
        include_str!("../../../spec/v1/vectors/proof/inferences.toml"),
    ),
    (
        "vectors/proof/transforms.toml",
        include_str!("../../../spec/v1/vectors/proof/transforms.toml"),
    ),
];

fn asset() -> Asset {
    Asset::read(ARITHMETIC).expect("the arithmetic vectors are inside the subset")
}

fn field(asset: &Asset, table: &str, key: &str) -> String {
    asset
        .text(table, key)
        .unwrap_or_else(|| panic!("[{table}] has no {key}"))
        .to_owned()
}

fn size(asset: &Asset, table: &str, key: &str) -> Nat {
    Nat::from_decimal(&field(asset, table, key))
        .unwrap_or_else(|error| panic!("[{table}] {key}: {error}"))
}

/// Reads an operand, which may be written unreduced and may be a bare integer.
fn value(asset: &Asset, table: &str, key: &str) -> Frac {
    let text = field(asset, table, key);
    let (top, bottom) = match text.find('/') {
        Some(at) => (&text[..at], &text[at + 1..]),
        None => (text.as_str(), "1"),
    };
    let numerator =
        Int::from_decimal(top).unwrap_or_else(|error| panic!("[{table}] {key} top: {error}"));
    let denominator =
        Int::from_decimal(bottom).unwrap_or_else(|error| panic!("[{table}] {key} bottom: {error}"));
    Frac::ratio(&numerator, &denominator)
        .unwrap_or_else(|| panic!("[{table}] {key} divides by zero"))
}

#[test]
fn every_asset_is_readable_by_this_side() {
    for (name, text) in ASSETS {
        let parsed = match Asset::read(text) {
            Ok(parsed) => parsed,
            Err(error) => panic!("{name}: {error}"),
        };
        assert!(!parsed.tables().is_empty(), "{name}: no tables");
        assert_eq!(parsed.number("meta", "version"), Some(1), "{name}: version");
        assert!(parsed.text("meta", "title").is_some(), "{name}: title");
    }
    assert!(
        ASSETS.len() >= 14,
        "the asset list is too short to be complete"
    );
}

#[test]
fn every_magnitude_vector_holds() {
    let asset = asset();
    let mut seen = 0usize;
    for table in asset.tables_under("biguint") {
        let operation = field(&asset, table, "op");
        let left = size(&asset, table, "a");
        let expected = field(&asset, table, "result");
        let actual = match operation.as_str() {
            "add" => left.plus(&size(&asset, table, "b")),
            "sub" => left
                .minus(&size(&asset, table, "b"))
                .expect("the vector stays nonnegative"),
            "mul" => left.times(&size(&asset, table, "b")),
            "gcd" => left.gcd(&size(&asset, table, "b")),
            "divmod_q" => {
                left.divide(&size(&asset, table, "b"))
                    .expect("nonzero divisor")
                    .0
            }
            "divmod_r" => {
                left.divide(&size(&asset, table, "b"))
                    .expect("nonzero divisor")
                    .1
            }
            other => {
                panic!("[{table}] names an operation this harness does not implement: {other}")
            }
        };
        assert_eq!(actual.to_decimal(), expected, "[{table}]");
        seen += 1;
    }
    assert!(seen >= 17, "only {seen} magnitude vectors were read");
}

#[test]
fn every_division_vector_reconstructs_its_dividend() {
    let asset = asset();
    let mut checked = 0usize;
    for table in asset.tables_under("biguint") {
        if field(&asset, table, "op") != "divmod_q" {
            continue;
        }
        let dividend = size(&asset, table, "a");
        let divisor = size(&asset, table, "b");
        let (quotient, remainder) = dividend.divide(&divisor).expect("nonzero divisor");
        assert_eq!(
            quotient.to_decimal(),
            field(&asset, table, "result"),
            "[{table}]"
        );
        assert_eq!(remainder.compare(&divisor), Ordering::Less, "[{table}]");
        assert_eq!(
            quotient.times(&divisor).plus(&remainder),
            dividend,
            "[{table}]"
        );
        checked += 1;
    }
    assert!(checked >= 4, "only {checked} quotient vectors were read");
}

#[test]
fn every_divisor_vector_divides_both_operands() {
    let asset = asset();
    let mut checked = 0usize;
    for table in asset.tables_under("biguint") {
        if field(&asset, table, "op") != "gcd" {
            continue;
        }
        let left = size(&asset, table, "a");
        let right = size(&asset, table, "b");
        let divisor = left.gcd(&right);
        assert_eq!(
            divisor.to_decimal(),
            field(&asset, table, "result"),
            "[{table}]"
        );
        if divisor.is_zero() {
            assert!(left.is_zero() && right.is_zero(), "[{table}]");
            checked += 1;
            continue;
        }
        for operand in [&left, &right] {
            if operand.is_zero() {
                continue;
            }
            let (_, remainder) = operand.divide(&divisor).expect("nonzero divisor");
            assert!(remainder.is_zero(), "[{table}] does not divide an operand");
        }
        checked += 1;
    }
    assert!(checked >= 3, "only {checked} divisor vectors were read");
}

#[test]
fn every_rational_vector_holds() {
    let asset = asset();
    let mut seen = 0usize;
    for table in asset.tables_under("rational") {
        let operation = field(&asset, table, "op");
        let left = value(&asset, table, "a");
        let expected = field(&asset, table, "result");
        match operation.as_str() {
            "normalize" => {
                assert_eq!(left.to_canonical(), expected, "[{table}]");
                let reread = Frac::from_canonical(&left.to_canonical())
                    .unwrap_or_else(|error| panic!("[{table}] canonical text: {error}"));
                assert_eq!(reread, left, "[{table}]");
                assert!(
                    left.top().size().gcd(left.bottom()).is_one(),
                    "[{table}] is not reduced"
                );
            }
            "add" | "sub" | "mul" | "div" => {
                let right = value(&asset, table, "b");
                let actual = match operation.as_str() {
                    "add" => left.plus(&right),
                    "sub" => left.minus(&right),
                    "mul" => left.times(&right),
                    _ => left
                        .divided(&right)
                        .unwrap_or_else(|| panic!("[{table}] divides by zero")),
                };
                assert_eq!(actual.to_canonical(), expected, "[{table}]");
                let recovered = match operation.as_str() {
                    "add" => actual.minus(&right),
                    "sub" => actual.plus(&right),
                    "mul" => actual.divided(&right).unwrap_or_else(|| left.clone()),
                    _ => actual.times(&right),
                };
                assert_eq!(recovered, left, "[{table}] does not invert");
            }
            "compare" => {
                let right = value(&asset, table, "b");
                let stated = match expected.as_str() {
                    "greater" => Ordering::Greater,
                    "less" => Ordering::Less,
                    "equal" => Ordering::Equal,
                    other => {
                        panic!("[{table}] names an ordering this harness does not know: {other}")
                    }
                };
                assert_eq!(left.compare(&right), stated, "[{table}]");
                assert_eq!(
                    right.compare(&left),
                    stated.reverse(),
                    "[{table}] is not antisymmetric"
                );
                let by_sign = match left.minus(&right).sign() {
                    1 => Ordering::Greater,
                    -1 => Ordering::Less,
                    _ => Ordering::Equal,
                };
                assert_eq!(
                    by_sign, stated,
                    "[{table}] disagrees with the sign of the difference"
                );
            }
            other => {
                panic!("[{table}] names an operation this harness does not implement: {other}")
            }
        }
        seen += 1;
    }
    assert!(seen >= 12, "only {seen} rational vectors were read");
}

#[test]
fn every_decimal_vector_becomes_the_number_its_characters_name() {
    let asset = asset();
    let mut seen = 0usize;
    for table in asset.tables_under("decimal") {
        let text = field(&asset, table, "text");
        let expected = field(&asset, table, "result");
        let parsed = Frac::from_decimal_uncapped(&text)
            .unwrap_or_else(|error| panic!("[{table}] {text}: {error}"));
        assert_eq!(parsed.to_canonical(), expected, "[{table}] from {text}");
        let reread = Frac::from_canonical(&expected)
            .unwrap_or_else(|error| panic!("[{table}] canonical {expected}: {error}"));
        assert_eq!(reread, parsed, "[{table}]");
        seen += 1;
    }
    assert!(seen >= 9, "only {seen} decimal vectors were read");
}

#[test]
fn every_table_in_the_asset_is_consumed_by_some_check() {
    let asset = asset();
    let known = ["meta", "biguint", "rational", "decimal"];
    for table in asset.tables() {
        let group = table.split('.').next().unwrap_or_default();
        assert!(
            known.contains(&group),
            "[{table}] is in no group any check reads"
        );
    }
    let counted = asset.tables_under("biguint").len()
        + asset.tables_under("rational").len()
        + asset.tables_under("decimal").len();
    assert_eq!(
        counted + 1,
        asset.tables().len(),
        "a table escaped every group"
    );
}
