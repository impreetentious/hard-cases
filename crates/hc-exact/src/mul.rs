//! Magnitude multiplication: schoolbook, and Karatsuba above a threshold.
//!
//! Two algorithms live here and the second exists only to make the first
//! affordable on the operands a certificate actually carries. That is a risk
//! worth naming: a recursive multiplication that disagrees with schoolbook on
//! one split size produces wrong proofs on large instances and correct ones on
//! every small test. The vectors under `spec/v1/vectors/arith/` therefore carry
//! operands on both sides of the threshold and one deliberately lopsided pair,
//! because the asymmetric split is where a recursive implementation is most
//! often wrong.

use crate::uint::{BigUint, LIMB_BITS, LIMB_MASK};

/// Operand length, in limbs, at or above which the recursive path is taken.
///
/// Fixed by the normative constants rather than tuned, so that two
/// implementations of this crate's contract agree on which path ran when a
/// disagreement has to be minimized.
pub const KARATSUBA_LIMBS: usize = 32;

impl BigUint {
    /// Product.
    #[must_use]
    pub fn mul(&self, rhs: &Self) -> Self {
        if self.is_zero() || rhs.is_zero() {
            return Self::zero();
        }
        let shortest = core::cmp::min(self.limbs().len(), rhs.limbs().len());
        if shortest < KARATSUBA_LIMBS {
            return Self::from_limbs_le(mul_schoolbook(self.limbs(), rhs.limbs()));
        }
        mul_karatsuba(self, rhs)
    }

    /// The value squared.
    #[must_use]
    pub fn square(&self) -> Self {
        self.mul(self)
    }

    /// The value raised to a small exponent, by repeated squaring.
    #[must_use]
    pub fn pow(&self, exponent: u32) -> Self {
        let mut result = Self::one();
        let mut base = self.clone();
        let mut remaining = exponent;
        while remaining > 0 {
            if remaining & 1 == 1 {
                result = result.mul(&base);
            }
            remaining >>= 1;
            if remaining > 0 {
                base = base.square();
            }
        }
        result
    }
}

/// The quadratic product, limb by limb.
fn mul_schoolbook(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut out = vec![0u64; a.len() + b.len()];
    for (i, &left) in a.iter().enumerate() {
        if left == 0 {
            continue;
        }
        let mut carry = 0u128;
        for (j, &right) in b.iter().enumerate() {
            let total = u128::from(left) * u128::from(right) + u128::from(out[i + j]) + carry;
            out[i + j] = (total & LIMB_MASK) as u64;
            carry = total >> LIMB_BITS;
        }
        // The carry out of the row cannot run past the end of `out`, because
        // the product of an `a.len()` and a `b.len()` magnitude never needs
        // more than their sum of limbs.
        let mut position = i + b.len();
        while carry != 0 {
            let total = u128::from(out[position]) + carry;
            out[position] = (total & LIMB_MASK) as u64;
            carry = total >> LIMB_BITS;
            position += 1;
        }
    }
    out
}

/// The three-multiplication recursive product.
fn mul_karatsuba(a: &BigUint, b: &BigUint) -> BigUint {
    let split = core::cmp::max(a.limbs().len(), b.limbs().len()).div_ceil(2);
    let (a_low, a_high) = split_limbs(a, split);
    let (b_low, b_high) = split_limbs(b, split);

    let low = a_low.mul(&b_low);
    let high = a_high.mul(&b_high);
    // The middle term is the sum of the two cross products, obtained as one
    // product minus the two already computed. Subtracting here is safe without
    // a sign because the middle product dominates both of them.
    let middle = a_low
        .add(&a_high)
        .mul(&b_low.add(&b_high))
        .sub(&low)
        .sub(&high);

    let shift = split as u64 * u64::from(LIMB_BITS);
    high.shl_bits(shift * 2)
        .add(&middle.shl_bits(shift))
        .add(&low)
}

/// Splits a magnitude at a limb index into its low and high halves.
fn split_limbs(value: &BigUint, at: usize) -> (BigUint, BigUint) {
    let limbs = value.limbs();
    if limbs.len() <= at {
        return (value.clone(), BigUint::zero());
    }
    (
        BigUint::from_limbs_le(limbs[..at].to_vec()),
        BigUint::from_limbs_le(limbs[at..].to_vec()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> BigUint {
        BigUint::parse_dec(text).expect("vector operand parses")
    }

    /// The reference the recursive path is held against inside this module.
    /// It is the same code the dispatcher uses below the threshold, which is
    /// the point: above the threshold the two must still agree.
    fn schoolbook(a: &BigUint, b: &BigUint) -> BigUint {
        if a.is_zero() || b.is_zero() {
            return BigUint::zero();
        }
        BigUint::from_limbs_le(mul_schoolbook(a.limbs(), b.limbs()))
    }

    #[test]
    fn zero_absorbs_and_one_preserves() {
        let a = parse("2432902008176640000");
        assert!(a.mul(&BigUint::zero()).is_zero());
        assert!(BigUint::zero().mul(&a).is_zero());
        assert_eq!(a.mul(&BigUint::one()), a);
    }

    #[test]
    fn the_largest_single_limb_product_needs_a_widening_multiply() {
        let a = parse("18446744073709551615");
        assert_eq!(
            a.mul(&a).to_dec_string(),
            "340282366920938463426481119284349108225"
        );
    }

    #[test]
    fn a_published_multi_limb_product_matches() {
        let a = parse("2432902008176640000");
        let b = parse("15511210043330985984000000");
        assert_eq!(
            a.mul(&b).to_dec_string(),
            "37737254063669622951147441236213760000000000"
        );
    }

    #[test]
    fn the_recursive_path_agrees_with_schoolbook_on_even_splits() {
        // Both operands sit above the threshold, so `mul` dispatches to the
        // recursive path while the reference stays quadratic.
        let a = BigUint::pow10(700).add_small(12345);
        let b = BigUint::pow10(690).sub(&BigUint::one());
        assert!(a.limbs().len() >= KARATSUBA_LIMBS && b.limbs().len() >= KARATSUBA_LIMBS);
        assert_eq!(a.mul(&b), schoolbook(&a, &b));
    }

    #[test]
    fn the_recursive_path_agrees_with_schoolbook_on_lopsided_splits() {
        // Both halves stay above the threshold, so the recursion runs on a
        // split where one side is nearly empty.
        let long = BigUint::pow10(2000).add_small(7);
        let short = BigUint::pow10(640).add_small(3);
        assert!(short.limbs().len() >= KARATSUBA_LIMBS);
        assert_eq!(long.mul(&short), schoolbook(&long, &short));
        assert_eq!(short.mul(&long), schoolbook(&short, &long));
    }

    #[test]
    fn one_limb_against_a_long_operand_stays_quadratic_and_correct() {
        let long = BigUint::pow10(600).add_small(1);
        let seven = BigUint::from_u64(7);
        assert_eq!(long.mul(&seven), schoolbook(&long, &seven));
        assert_eq!(long.mul(&seven), long.mul_small(7));
    }

    #[test]
    fn multiplication_commutes_and_associates_across_the_threshold() {
        let a = BigUint::pow10(700).add_small(1);
        let b = BigUint::pow10(710).add_small(2);
        let c = BigUint::from_u64(1_000_003);
        assert_eq!(a.mul(&b), b.mul(&a));
        assert_eq!(a.mul(&b).mul(&c), a.mul(&b.mul(&c)));
    }

    #[test]
    fn powers_agree_with_repeated_multiplication() {
        let base = parse("123456789012345678901234567890");
        let mut expected = BigUint::one();
        for _ in 0..17 {
            expected = expected.mul(&base);
        }
        assert_eq!(base.pow(17), expected);
        assert_eq!(base.pow(0), BigUint::one());
        assert_eq!(base.pow(1), base);
        assert_eq!(base.square(), base.mul(&base));
        assert_eq!(BigUint::from_u64(10).pow(45), BigUint::pow10(45));
    }
}
