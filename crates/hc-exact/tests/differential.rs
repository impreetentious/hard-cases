//! The arbitrary-precision arithmetic against the machine's own.
//!
//! The frozen vectors pin the cases an implementation is most likely to get
//! wrong; this pins everything else. Operands are drawn small enough that the
//! answer fits in a machine word, so the reference is the hardware and there
//! is nothing to be mistaken about — and small enough is where multi-limb code
//! is at its least exercised, which is exactly why the campaign lives at that
//! size rather than at a comfortable one.
//!
//! Widths are drawn as well as values. A campaign over uniformly random
//! 128-bit numbers almost never produces an operand with a zero high limb, a
//! single-limb operand, or a zero, and those are the shapes where a
//! normalization bug lives.
//!
//! The default run is small enough to sit inside a local preflight. The full
//! campaign is the same code with a larger count, run under its own name so it
//! can be asked for with `--ignored` without the ordinary suite paying for it.

use hc_core::Xoshiro256PlusPlus;
use hc_exact::{BigInt, BigUint, Rational};

/// Iterations in the run that every preflight performs.
const PREFLIGHT_ROUNDS: u32 = 60_000;
/// Iterations in the full campaign.
const FULL_ROUNDS: u32 = 10_000_000;

/// Draws an operand whose width is itself drawn, so every limb shape appears.
fn draw(generator: &mut Xoshiro256PlusPlus, max_bits: u32) -> u128 {
    let width = generator.next_below(u64::from(max_bits) + 1) as u32;
    if width == 0 {
        return 0;
    }
    let value = (u128::from(generator.next_u64()) << 64) | u128::from(generator.next_u64());
    value >> (128 - width)
}

fn big(value: u128) -> BigUint {
    BigUint::parse_dec(&value.to_string()).expect("a decimal machine value parses")
}

/// One round of every magnitude operation, against the machine's arithmetic.
fn magnitude_round(generator: &mut Xoshiro256PlusPlus) {
    // Sixty-three bits keeps every product inside a `u128`, so the reference
    // never has to be anything but a native operation.
    let left = draw(generator, 63);
    let right = draw(generator, 63);
    let (big_left, big_right) = (big(left), big(right));

    assert_eq!(
        big_left.add(&big_right),
        big(left + right),
        "{left} + {right}"
    );
    assert_eq!(
        big_left.mul(&big_right),
        big(left * right),
        "{left} * {right}"
    );

    let (low, high) = if left <= right {
        (left, right)
    } else {
        (right, left)
    };
    assert_eq!(big(high).sub(&big(low)), big(high - low), "{high} - {low}");
    assert_eq!(
        big(low).checked_sub(&big(high)).is_none(),
        low < high,
        "{low} - {high}"
    );

    assert_eq!(
        big_left.cmp_value(&big_right),
        left.cmp(&right),
        "{left} vs {right}"
    );

    match (left.checked_div(right), left.checked_rem(right)) {
        (Some(quotient), Some(remainder)) => {
            let (actual_quotient, actual_remainder) = big_left.divrem(&big_right);
            assert_eq!(actual_quotient, big(quotient), "{left} / {right}");
            assert_eq!(actual_remainder, big(remainder), "{left} % {right}");
        }
        _ => assert!(
            big_left.checked_divrem(&big_right).is_none(),
            "division by zero"
        ),
    }

    assert_eq!(
        big_left.gcd(&big_right),
        big(gcd_u128(left, right)),
        "gcd {left} {right}"
    );
    assert_eq!(big_left.to_dec_string(), left.to_string(), "text of {left}");
    assert_eq!(
        big_left.bit_len(),
        u64::from(128 - left.leading_zeros()),
        "bits of {left}"
    );
    assert_eq!(
        big_left.is_even(),
        {
            let __multiple_value = left;
            __multiple_value % 2 == 0
        },
        "parity of {left}"
    );
    if left != 0 {
        assert_eq!(
            big_left.trailing_zeros(),
            u64::from(left.trailing_zeros()),
            "trailing zeros of {left}"
        );
    }
}

/// One round of signed and rational arithmetic, against the machine's.
fn signed_round(generator: &mut Xoshiro256PlusPlus) {
    let left = draw(generator, 31) as i64 * if generator.next_below(2) == 0 { 1 } else { -1 };
    let right = draw(generator, 31) as i64 * if generator.next_below(2) == 0 { 1 } else { -1 };
    let (big_left, big_right) = (BigInt::from_i64(left), BigInt::from_i64(right));

    assert_eq!(
        big_left.add(&big_right),
        BigInt::from_i64(left + right),
        "{left} + {right}"
    );
    assert_eq!(
        big_left.sub(&big_right),
        BigInt::from_i64(left - right),
        "{left} - {right}"
    );
    assert_eq!(
        big_left.mul(&big_right),
        BigInt::from_i64(left * right),
        "{left} * {right}"
    );
    assert_eq!(
        big_left.cmp_value(&big_right),
        left.cmp(&right),
        "{left} vs {right}"
    );
    assert_eq!(big_left.signum(), left.signum() as i32, "sign of {left}");

    if right != 0 {
        let (quotient, remainder) = big_left
            .checked_divrem(&big_right)
            .expect("nonzero divisor");
        assert_eq!(
            quotient,
            BigInt::from_i64(left / right),
            "trunc {left} / {right}"
        );
        assert_eq!(
            remainder,
            BigInt::from_i64(left % right),
            "trunc {left} % {right}"
        );
        // Floored division is characterized rather than recomputed. Deriving
        // the reference the same way the implementation derives its answer
        // would compare a routine against itself; the properties below
        // determine the pair uniquely instead.
        let (floored, modulo) = big_left
            .checked_divrem_floor(&big_right)
            .expect("nonzero divisor");
        assert_eq!(
            floored.mul(&big_right).add(&modulo),
            big_left,
            "floor reconstruction"
        );
        assert!(
            modulo.is_zero() || modulo.is_negative() == big_right.is_negative(),
            "floor remainder sign for {left} over {right}"
        );
        assert!(
            modulo.abs() < big_right.abs(),
            "floor remainder magnitude for {left} over {right}"
        );
        assert!(floored <= quotient, "floor exceeds truncation");
        assert!(
            quotient.sub(&floored) <= BigInt::one(),
            "floor is more than one below truncation"
        );
        // A floored remainder carries the divisor's sign, so the machine's
        // Euclidean remainder and this one agree exactly when that sign is
        // positive.
        if right > 0 {
            assert_eq!(
                modulo,
                BigInt::from_i64(left.rem_euclid(right)),
                "floor {left} % {right}"
            );
        }

        let exact = Rational::new(big_left.clone(), big_right.clone()).expect("nonzero divisor");
        let machine = (left as f64) / (right as f64);
        assert_eq!(exact.to_binary64(), machine, "{left} over {right}");
        let reconstructed = exact.mul(&Rational::from_int(big_right.clone()));
        assert_eq!(
            reconstructed,
            Rational::from_int(big_left.clone()),
            "rational round trip"
        );
        assert_eq!(
            exact.frac().is_zero(),
            left % right == 0,
            "fractional part of {left} over {right}"
        );
    }
}

/// The machine's greatest common divisor, by the Euclidean algorithm.
fn gcd_u128(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let next = left % right;
        left = right;
        right = next;
    }
    left
}

fn campaign(rounds: u32, seed: u64) {
    let mut generator = Xoshiro256PlusPlus::from_seed(seed);
    for _ in 0..rounds {
        magnitude_round(&mut generator);
        signed_round(&mut generator);
    }
}

#[test]
fn the_preflight_campaign_finds_no_disagreement() {
    campaign(PREFLIGHT_ROUNDS, 0);
}

#[test]
fn a_second_seed_finds_no_disagreement() {
    // A campaign is only as good as the region it visits, and one stream
    // visits one region.
    campaign(PREFLIGHT_ROUNDS, 0x0123_4567_89ab_cdef);
}

#[test]
fn the_campaign_is_reproducible() {
    // Determinism is a property of the harness as much as of the solver: a
    // campaign that cannot be replayed cannot be minimized when it fails.
    let mut left = Xoshiro256PlusPlus::from_seed(99);
    let mut right = Xoshiro256PlusPlus::from_seed(99);
    for _ in 0..1024 {
        assert_eq!(draw(&mut left, 63), draw(&mut right, 63));
    }
}

#[test]
#[ignore = "the full campaign; run with --ignored"]
fn the_full_campaign_finds_no_disagreement() {
    campaign(FULL_ROUNDS, 0);
}
