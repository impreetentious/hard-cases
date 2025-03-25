//! Magnitude division: Knuth's algorithm D, with the correction step written
//! out rather than assumed.
//!
//! The trial quotient digit derived from the top two limbs of the remainder is
//! occasionally one too large. Most schoolbook implementations are correct
//! anyway, because the case is rare enough that a test suite of random
//! operands never reaches it. That is not a property to rely on in a proof
//! checker, so the add-back branch is exercised by a named vector whose only
//! purpose is to reach it.

use crate::uint::{BigUint, LIMB_BITS, LIMB_MASK};

impl BigUint {
    /// Quotient and remainder, or `None` when the divisor is zero.
    #[must_use]
    pub fn checked_divrem(&self, divisor: &Self) -> Option<(Self, Self)> {
        if divisor.is_zero() {
            return None;
        }
        if self.cmp_value(divisor) == core::cmp::Ordering::Less {
            // Short-circuit before normalization: the loop below assumes the
            // dividend is at least as long as the divisor.
            return Some((Self::zero(), self.clone()));
        }
        if let Some(small) = divisor.to_u64() {
            let (quotient, remainder) = self.divrem_small(small);
            return Some((quotient, Self::from_u64(remainder)));
        }
        let (quotient, remainder) = divrem_knuth(self.limbs(), divisor.limbs());
        Some((
            Self::from_limbs_le(quotient),
            Self::from_limbs_le(remainder),
        ))
    }

    /// Quotient and remainder.
    ///
    /// # Panics
    ///
    /// Panics when `divisor` is zero.
    #[must_use]
    pub fn divrem(&self, divisor: &Self) -> (Self, Self) {
        self.checked_divrem(divisor)
            .expect("BigUint::divrem by zero")
    }

    /// Quotient alone.
    ///
    /// # Panics
    ///
    /// Panics when `divisor` is zero.
    #[must_use]
    pub fn div(&self, divisor: &Self) -> Self {
        self.divrem(divisor).0
    }

    /// Remainder alone.
    ///
    /// # Panics
    ///
    /// Panics when `divisor` is zero.
    #[must_use]
    pub fn rem(&self, divisor: &Self) -> Self {
        self.divrem(divisor).1
    }

    /// Whether the value divides evenly by `divisor`.
    ///
    /// # Panics
    ///
    /// Panics when `divisor` is zero.
    #[must_use]
    pub fn divides_evenly(&self, divisor: &Self) -> bool {
        self.rem(divisor).is_zero()
    }
}

/// Knuth's algorithm D over normalized limb slices.
///
/// `dividend` is at least as long as `divisor`, and `divisor` has at least two
/// limbs; both conditions are established by the caller.
fn divrem_knuth(dividend: &[u64], divisor: &[u64]) -> (Vec<u64>, Vec<u64>) {
    let n = divisor.len();
    let m = dividend.len() - n;

    // Normalizing so the divisor's top limb has its high bit set is what makes
    // the trial digit wrong by at most one instead of by an unbounded amount.
    let shift = u64::from(divisor[n - 1].leading_zeros());
    let scaled_divisor = BigUint::from_limbs_le(divisor.to_vec()).shl_bits(shift);
    let mut top = scaled_divisor.limbs().to_vec();
    top.resize(n, 0);
    let scaled_divisor = top;

    let mut remainder = BigUint::from_limbs_le(dividend.to_vec())
        .shl_bits(shift)
        .limbs()
        .to_vec();
    remainder.resize(dividend.len() + 1, 0);

    let divisor_high = u128::from(scaled_divisor[n - 1]);
    let divisor_next = u128::from(scaled_divisor[n - 2]);
    let mut quotient = vec![0u64; m + 1];

    for j in (0..=m).rev() {
        let numerator =
            (u128::from(remainder[j + n]) << LIMB_BITS) | u128::from(remainder[j + n - 1]);
        let mut trial = numerator / divisor_high;
        let mut trial_rem = numerator % divisor_high;
        // Two rounds of the cheap test remove every overestimate but one; the
        // add-back below removes that one.
        while trial > LIMB_MASK
            || trial * divisor_next > ((trial_rem << LIMB_BITS) | u128::from(remainder[j + n - 2]))
        {
            trial -= 1;
            trial_rem += divisor_high;
            if trial_rem > LIMB_MASK {
                break;
            }
        }

        let mut carry = 0u128;
        let mut borrow = 0i128;
        for (i, &limb) in scaled_divisor.iter().enumerate() {
            let product = trial * u128::from(limb) + carry;
            carry = product >> LIMB_BITS;
            let difference =
                i128::from(remainder[i + j]) - i128::from((product & LIMB_MASK) as u64) + borrow;
            remainder[i + j] = difference as u64;
            borrow = difference >> LIMB_BITS;
        }
        let difference = i128::from(remainder[j + n]) - i128::from(carry as u64) + borrow;
        remainder[j + n] = difference as u64;
        borrow = difference >> LIMB_BITS;

        if borrow != 0 {
            // The trial digit was one too large after all. Give the digit back
            // and add the divisor once; the carry out of that addition cancels
            // the borrow exactly.
            trial -= 1;
            let mut add_carry = 0u128;
            for (i, &limb) in scaled_divisor.iter().enumerate() {
                let total = u128::from(remainder[i + j]) + u128::from(limb) + add_carry;
                remainder[i + j] = (total & LIMB_MASK) as u64;
                add_carry = total >> LIMB_BITS;
            }
            let total = u128::from(remainder[j + n]) + add_carry;
            remainder[j + n] = (total & LIMB_MASK) as u64;
        }

        quotient[j] = trial as u64;
    }

    let scaled_remainder = BigUint::from_limbs_le(remainder[..n].to_vec()).shr_bits(shift);
    (quotient, scaled_remainder.limbs().to_vec())
}

#[cfg(test)]
mod tests {
    use crate::uint::BigUint;

    fn parse(text: &str) -> BigUint {
        BigUint::parse_dec(text).expect("vector operand parses")
    }

    fn check(dividend: &BigUint, divisor: &BigUint) -> (BigUint, BigUint) {
        let (quotient, remainder) = dividend.divrem(divisor);
        assert!(
            remainder.cmp_value(divisor) == core::cmp::Ordering::Less,
            "remainder must be strictly below the divisor"
        );
        assert_eq!(
            quotient.mul(divisor).add(&remainder),
            *dividend,
            "quotient and remainder must reconstruct the dividend"
        );
        (quotient, remainder)
    }

    #[test]
    fn division_by_zero_is_reported_rather_than_trapped() {
        assert_eq!(BigUint::one().checked_divrem(&BigUint::zero()), None);
    }

    #[test]
    fn a_smaller_numerator_short_circuits() {
        let (quotient, remainder) = check(&BigUint::from_u64(5), &BigUint::from_u64(7));
        assert!(quotient.is_zero());
        assert_eq!(remainder.to_dec_string(), "5");
    }

    #[test]
    fn division_by_one_shifts_by_nothing() {
        let a = parse("265252859812191058636308480000000");
        let (quotient, remainder) = check(&a, &BigUint::one());
        assert_eq!(quotient, a);
        assert!(remainder.is_zero());
    }

    #[test]
    fn a_divisor_just_past_a_limb_boundary_normalizes_by_nearly_a_limb() {
        let a =
            parse("115792089237316195423570985008687907853269984665640564039457584007913129639935");
        let b = parse("18446744073709551617");
        let (quotient, remainder) = check(&a, &b);
        assert_eq!(
            quotient.to_dec_string(),
            "6277101735386680763495507056286727952657427581105975853055"
        );
        assert!(remainder.is_zero());
    }

    #[test]
    fn the_trial_digit_correction_runs() {
        // An implementation that omits the add-back produces a quotient one
        // too large here and nowhere else in the vector file.
        let a = parse("6277101735386680763835789423207666416083908700390324961280");
        let b = parse("340282366920938463463374607431768211455");
        let (quotient, remainder) = check(&a, &b);
        assert_eq!(quotient.to_dec_string(), "18446744073709551616");
        assert!(remainder.is_zero());
    }

    #[test]
    fn the_identity_holds_across_a_sweep_of_shapes() {
        let mut dividend = BigUint::pow10(120).add_small(999_983);
        for exponent in [1u32, 7, 19, 20, 41, 63, 64, 100, 119] {
            let divisor = BigUint::pow10(exponent).add_small(7);
            check(&dividend, &divisor);
            dividend = dividend.mul_small(3).add_small(1);
        }
    }

    #[test]
    fn even_division_is_recognized() {
        let a = parse("37737254063669622951147441236213760000000000");
        let b = parse("2432902008176640000");
        assert!(a.divides_evenly(&b));
        assert_eq!(a.div(&b).to_dec_string(), "15511210043330985984000000");
        assert!(!a.add_small(1).divides_evenly(&b));
        assert_eq!(a.add_small(1).rem(&b).to_dec_string(), "1");
    }

    #[test]
    fn division_undoes_multiplication_above_the_recursive_threshold() {
        let a = BigUint::pow10(700).add_small(12345);
        let b = BigUint::pow10(690).sub(&BigUint::one());
        let product = a.mul(&b);
        let (quotient, remainder) = check(&product, &b);
        assert_eq!(quotient, a);
        assert!(remainder.is_zero());
    }
}
