//! The solver-side arithmetic against the frozen vectors.
//!
//! These operands and results were written down before either arithmetic
//! implementation existed, and neither implementation generated them. That is
//! the only reason they are worth anything: two stacks written by the same
//! hand in the same week will agree with each other long before either agrees
//! with the truth, so the thing they are held against has to come from
//! outside both.
//!
//! The harness does two jobs. It checks each stated result, and — where the
//! operation admits an independent route — it recomputes the vector's own
//! claim a second way. A quotient is checked against its remainder, a divisor
//! against the coprimality of the quotients it leaves, a comparison against
//! the sign of the difference. Hand-worked vectors are valuable because no
//! implementation produced them and risky because no implementation checked
//! them, and the second job is what keeps the risk in hand.
//!
//! Every table in the asset must be consumed. A harness that skips what it
//! does not recognise reports success for coverage it does not have.

use core::cmp::Ordering;

use hc_core::{SpecDocument, SpecTable};
use hc_exact::{BigInt, BigUint, Rational};

const ARITHMETIC: &str = include_str!("../../../spec/v1/vectors/arith/arithmetic.toml");

/// Reads a magnitude written in the vector file.
fn magnitude(table: &SpecTable, key: &str) -> BigUint {
    let text = table
        .text(key)
        .unwrap_or_else(|| panic!("[{}] has no {key}", table.name()));
    BigUint::parse_dec(text).unwrap_or_else(|error| panic!("[{}] {key}: {error}", table.name()))
}

/// Reads a rational written in the vector file.
///
/// Operands are deliberately allowed to be unreduced — `6/4` is one of them —
/// so this is not the canonical-text reader. Results that are meant to be
/// canonical are checked as text against the canonical renderer instead.
fn rational(table: &SpecTable, key: &str) -> Rational {
    let text = table
        .text(key)
        .unwrap_or_else(|| panic!("[{}] has no {key}", table.name()));
    let (top, bottom) = match text.split_once('/') {
        Some((left, right)) => (left, right),
        None => (text, "1"),
    };
    let numerator = BigInt::parse_dec(top)
        .unwrap_or_else(|error| panic!("[{}] {key} numerator: {error}", table.name()));
    let denominator = BigInt::parse_dec(bottom)
        .unwrap_or_else(|error| panic!("[{}] {key} denominator: {error}", table.name()));
    Rational::new(numerator, denominator)
        .unwrap_or_else(|error| panic!("[{}] {key}: {error}", table.name()))
}

fn expect_text(table: &SpecTable, key: &str) -> String {
    table
        .text(key)
        .unwrap_or_else(|| panic!("[{}] has no {key}", table.name()))
        .to_owned()
}

fn document() -> SpecDocument {
    SpecDocument::parse(ARITHMETIC).expect("the arithmetic vectors are inside the subset")
}

#[test]
fn every_magnitude_vector_holds() {
    let document = document();
    let mut seen = 0usize;
    for table in document.tables_under("biguint") {
        let name = table.name();
        let operation = expect_text(table, "op");
        let a = magnitude(table, "a");
        let expected = expect_text(table, "result");
        let actual = match operation.as_str() {
            "add" => a.add(&magnitude(table, "b")),
            "sub" => a.sub(&magnitude(table, "b")),
            "mul" => a.mul(&magnitude(table, "b")),
            "gcd" => a.gcd(&magnitude(table, "b")),
            "divmod_q" => a.divrem(&magnitude(table, "b")).0,
            "divmod_r" => a.divrem(&magnitude(table, "b")).1,
            other => panic!("[{name}] names an operation the harness does not implement: {other}"),
        };
        assert_eq!(actual.to_dec_string(), expected, "[{name}]");
        assert!(
            table.text("covers").is_some(),
            "[{name}] states no rationale"
        );
        seen += 1;
    }
    assert!(seen >= 17, "only {seen} magnitude vectors were read");
}

#[test]
fn every_division_vector_reconstructs_its_dividend() {
    // The stated quotient and remainder are checked above. Here they are
    // checked against each other, which is a route the vector's author did not
    // take and therefore cannot have made the same mistake on.
    let document = document();
    let mut checked = 0usize;
    for table in document.tables_under("biguint") {
        if expect_text(table, "op") != "divmod_q" {
            continue;
        }
        let dividend = magnitude(table, "a");
        let divisor = magnitude(table, "b");
        let (quotient, remainder) = dividend.divrem(&divisor);
        assert_eq!(
            quotient.to_dec_string(),
            expect_text(table, "result"),
            "[{}]",
            table.name()
        );
        assert!(
            remainder.cmp_value(&divisor) == Ordering::Less,
            "[{}]",
            table.name()
        );
        assert_eq!(
            quotient.mul(&divisor).add(&remainder),
            dividend,
            "[{}]",
            table.name()
        );
        checked += 1;
    }
    assert!(checked >= 4, "only {checked} quotient vectors were read");
}

#[test]
fn every_divisor_vector_divides_and_leaves_a_coprime_pair() {
    let document = document();
    let mut checked = 0usize;
    for table in document.tables_under("biguint") {
        if expect_text(table, "op") != "gcd" {
            continue;
        }
        let a = magnitude(table, "a");
        let b = magnitude(table, "b");
        let divisor = a.gcd(&b);
        assert_eq!(
            divisor.to_dec_string(),
            expect_text(table, "result"),
            "[{}]",
            table.name()
        );
        if divisor.is_zero() {
            assert!(a.is_zero() && b.is_zero(), "[{}]", table.name());
            checked += 1;
            continue;
        }
        assert!(
            a.is_zero() || a.divides_evenly(&divisor),
            "[{}]",
            table.name()
        );
        assert!(
            b.is_zero() || b.divides_evenly(&divisor),
            "[{}]",
            table.name()
        );
        if !a.is_zero() && !b.is_zero() {
            let left = a.div(&divisor);
            let right = b.div(&divisor);
            assert_eq!(
                left.gcd(&right),
                BigUint::one(),
                "[{}] leaves a common factor",
                table.name()
            );
        }
        checked += 1;
    }
    assert!(checked >= 3, "only {checked} divisor vectors were read");
}

#[test]
fn every_rational_vector_holds() {
    let document = document();
    let mut seen = 0usize;
    for table in document.tables_under("rational") {
        let name = table.name();
        let operation = expect_text(table, "op");
        let a = rational(table, "a");
        let expected = expect_text(table, "result");
        match operation.as_str() {
            "normalize" => {
                assert_eq!(a.to_canonical_string(), expected, "[{name}]");
                // Reducing an already reduced value changes nothing, and the
                // canonical text reads back to the same number.
                let reread = Rational::parse_canonical(&a.to_canonical_string())
                    .unwrap_or_else(|error| panic!("[{name}] canonical text: {error}"));
                assert_eq!(reread, a, "[{name}]");
                assert_eq!(
                    a.numerator().magnitude().gcd(a.denominator()),
                    BigUint::one(),
                    "[{name}] is not reduced"
                );
            }
            "add" | "sub" | "mul" | "div" => {
                let b = rational(table, "b");
                let actual = match operation.as_str() {
                    "add" => a.add(&b),
                    "sub" => a.sub(&b),
                    "mul" => a.mul(&b),
                    _ => a
                        .checked_div(&b)
                        .unwrap_or_else(|| panic!("[{name}] divides by zero")),
                };
                assert_eq!(actual.to_canonical_string(), expected, "[{name}]");
                // Each operation is undone by its inverse, which recomputes
                // the vector by a route its author did not use.
                let recovered = match operation.as_str() {
                    "add" => actual.sub(&b),
                    "sub" => actual.add(&b),
                    "mul" => actual.checked_div(&b).unwrap_or_else(|| a.clone()),
                    _ => actual.mul(&b),
                };
                assert_eq!(recovered, a, "[{name}] does not invert");
            }
            "compare" => {
                let b = rational(table, "b");
                let ordering = a.cmp_value(&b);
                let stated = match expected.as_str() {
                    "greater" => Ordering::Greater,
                    "less" => Ordering::Less,
                    "equal" => Ordering::Equal,
                    other => {
                        panic!("[{name}] names an ordering the harness does not know: {other}")
                    }
                };
                assert_eq!(ordering, stated, "[{name}]");
                assert_eq!(
                    b.cmp_value(&a),
                    stated.reverse(),
                    "[{name}] is not antisymmetric"
                );
                // The sign of the difference is the same claim reached without
                // cross multiplication.
                let difference = a.sub(&b);
                let by_sign = match difference.signum() {
                    1 => Ordering::Greater,
                    -1 => Ordering::Less,
                    _ => Ordering::Equal,
                };
                assert_eq!(
                    by_sign, stated,
                    "[{name}] disagrees with the sign of the difference"
                );
            }
            other => panic!("[{name}] names an operation the harness does not implement: {other}"),
        }
        assert!(
            table.text("covers").is_some(),
            "[{name}] states no rationale"
        );
        seen += 1;
    }
    assert!(seen >= 12, "only {seen} rational vectors were read");
}

#[test]
fn every_decimal_vector_becomes_the_number_its_characters_name() {
    let document = document();
    let mut seen = 0usize;
    for table in document.tables_under("decimal") {
        let name = table.name();
        let text = expect_text(table, "text");
        let expected = expect_text(table, "result");
        let value = Rational::parse_decimal(&text)
            .unwrap_or_else(|error| panic!("[{name}] {text}: {error}"));
        assert_eq!(
            value.to_canonical_string(),
            expected,
            "[{name}] from {text}"
        );
        // The canonical text reads back to the same value, so the two
        // directions of the text boundary agree on every vector.
        let reread = Rational::parse_canonical(&expected)
            .unwrap_or_else(|error| panic!("[{name}] canonical {expected}: {error}"));
        assert_eq!(reread, value, "[{name}]");
        assert!(
            table.text("covers").is_some(),
            "[{name}] states no rationale"
        );
        seen += 1;
    }
    assert!(seen >= 9, "only {seen} decimal vectors were read");
}

#[test]
fn every_table_in_the_asset_is_consumed_by_some_check() {
    // A vector nobody reads is a vector nobody is held to. The groups below
    // are exactly the ones the checks above iterate; a new group in the asset
    // fails here until a check exists for it.
    let document = document();
    let known = ["meta", "biguint", "rational", "decimal"];
    for table in document.tables() {
        let group = table.name().split('.').next().unwrap_or_default();
        assert!(
            known.contains(&group),
            "[{}] is in no group any check reads",
            table.name()
        );
    }
    let counted = document.tables_under("biguint").count()
        + document.tables_under("rational").count()
        + document.tables_under("decimal").count();
    assert_eq!(
        counted + 1,
        document.tables().len(),
        "a table escaped every group"
    );
}
