//! Arbitrary-precision unsigned integers.
//!
//! The representation is a little-endian vector of 64-bit limbs with no
//! trailing zero limb, so that zero is the empty vector and every value has
//! exactly one encoding. That invariant is what makes equality and ordering
//! structural rather than semantic, and it is restored by [`BigUint::trim`]
//! after every operation that can shorten a number.
//!
//! Limb-level arithmetic is done by widening into `u128` rather than by
//! chasing carry flags. It is slower than the intrinsic and it is obviously
//! correct, which is the trade this crate makes everywhere.

use core::cmp::Ordering;
use core::fmt;

/// Bits per limb.
pub const LIMB_BITS: u32 = 64;

/// The low half of a widened product, as a mask.
pub(crate) const LIMB_MASK: u128 = 0xffff_ffff_ffff_ffff;

/// An arbitrary-precision unsigned integer.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct BigUint {
    /// Little-endian limbs with no trailing zero. Empty is zero.
    limbs: Vec<u64>,
}

impl BigUint {
    /// The additive identity.
    #[must_use]
    pub const fn zero() -> Self {
        Self { limbs: Vec::new() }
    }

    /// The multiplicative identity.
    #[must_use]
    pub fn one() -> Self {
        Self::from_u64(1)
    }

    /// Builds a value from a single machine word.
    #[must_use]
    pub fn from_u64(value: u64) -> Self {
        if value == 0 {
            Self::zero()
        } else {
            Self { limbs: vec![value] }
        }
    }

    /// Builds a value from a double-width machine word.
    #[must_use]
    pub fn from_u128(value: u128) -> Self {
        let mut limbs = vec![(value & LIMB_MASK) as u64, (value >> LIMB_BITS) as u64];
        let mut out = Self {
            limbs: core::mem::take(&mut limbs),
        };
        out.trim();
        out
    }

    /// Builds a value from limbs given least significant first.
    #[must_use]
    pub fn from_limbs_le(limbs: Vec<u64>) -> Self {
        let mut out = Self { limbs };
        out.trim();
        out
    }

    /// The limbs, least significant first, with no trailing zero.
    #[must_use]
    pub fn limbs(&self) -> &[u64] {
        &self.limbs
    }

    /// Whether the value is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    /// Whether the value is even. Zero is even.
    #[must_use]
    pub fn is_even(&self) -> bool {
        self.limbs.first().is_none_or(|low| low & 1 == 0)
    }

    /// The number of significant bits; zero has none.
    #[must_use]
    pub fn bit_len(&self) -> u64 {
        match self.limbs.last() {
            None => 0,
            Some(&top) => {
                let full = (self.limbs.len() as u64 - 1) * u64::from(LIMB_BITS);
                full + u64::from(LIMB_BITS - top.leading_zeros())
            }
        }
    }

    /// The number of trailing zero bits. Zero reports zero, which callers must
    /// special-case; the binary greatest common divisor does exactly that.
    #[must_use]
    pub fn trailing_zeros(&self) -> u64 {
        for (index, &limb) in self.limbs.iter().enumerate() {
            if limb != 0 {
                return index as u64 * u64::from(LIMB_BITS) + u64::from(limb.trailing_zeros());
            }
        }
        0
    }

    /// The value as a machine word, when it fits.
    #[must_use]
    pub fn to_u64(&self) -> Option<u64> {
        match self.limbs.len() {
            0 => Some(0),
            1 => Some(self.limbs[0]),
            _ => None,
        }
    }

    /// The value as a double-width machine word, when it fits.
    #[must_use]
    pub fn to_u128(&self) -> Option<u128> {
        match self.limbs.len() {
            0 => Some(0),
            1 => Some(u128::from(self.limbs[0])),
            2 => Some(u128::from(self.limbs[0]) | (u128::from(self.limbs[1]) << LIMB_BITS)),
            _ => None,
        }
    }

    /// Drops trailing zero limbs, restoring the unique-encoding invariant.
    fn trim(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }

    /// Sum.
    #[must_use]
    pub fn add(&self, rhs: &Self) -> Self {
        let (long, short) = if self.limbs.len() >= rhs.limbs.len() {
            (&self.limbs, &rhs.limbs)
        } else {
            (&rhs.limbs, &self.limbs)
        };
        let mut out = Vec::with_capacity(long.len() + 1);
        let mut carry = 0u128;
        for (index, &limb) in long.iter().enumerate() {
            let other = short.get(index).copied().unwrap_or(0);
            let sum = u128::from(limb) + u128::from(other) + carry;
            out.push((sum & LIMB_MASK) as u64);
            carry = sum >> LIMB_BITS;
        }
        if carry != 0 {
            out.push(carry as u64);
        }
        Self::from_limbs_le(out)
    }

    /// Difference, or `None` when the result would be negative.
    ///
    /// There is no wrapping subtraction here on purpose. A magnitude that went
    /// negative is a sign error in the caller, and returning a large positive
    /// number instead of saying so is how such an error survives to a proof.
    #[must_use]
    pub fn checked_sub(&self, rhs: &Self) -> Option<Self> {
        if self.cmp_value(rhs) == Ordering::Less {
            return None;
        }
        let mut out = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i128;
        for (index, &limb) in self.limbs.iter().enumerate() {
            let other = rhs.limbs.get(index).copied().unwrap_or(0);
            let diff = i128::from(limb) - i128::from(other) + borrow;
            out.push(diff as u64);
            borrow = diff >> LIMB_BITS;
        }
        debug_assert_eq!(
            borrow, 0,
            "borrow escaped a subtraction the comparison admitted"
        );
        Some(Self::from_limbs_le(out))
    }

    /// Difference, panicking when the result would be negative.
    ///
    /// # Panics
    ///
    /// Panics when `rhs` exceeds `self`.
    #[must_use]
    pub fn sub(&self, rhs: &Self) -> Self {
        self.checked_sub(rhs)
            .expect("BigUint::sub called with a subtrahend larger than the minuend")
    }

    /// Ordering by value.
    #[must_use]
    pub fn cmp_value(&self, rhs: &Self) -> Ordering {
        match self.limbs.len().cmp(&rhs.limbs.len()) {
            Ordering::Equal => self.limbs.iter().rev().cmp(rhs.limbs.iter().rev()),
            other => other,
        }
    }

    /// The value multiplied by a single limb.
    #[must_use]
    pub fn mul_small(&self, factor: u64) -> Self {
        if factor == 0 || self.is_zero() {
            return Self::zero();
        }
        let mut out = Vec::with_capacity(self.limbs.len() + 1);
        let mut carry = 0u128;
        for &limb in &self.limbs {
            let product = u128::from(limb) * u128::from(factor) + carry;
            out.push((product & LIMB_MASK) as u64);
            carry = product >> LIMB_BITS;
        }
        if carry != 0 {
            out.push(carry as u64);
        }
        Self::from_limbs_le(out)
    }

    /// The value plus a single limb.
    #[must_use]
    pub fn add_small(&self, addend: u64) -> Self {
        self.add(&Self::from_u64(addend))
    }

    /// Quotient and remainder by a single nonzero limb.
    ///
    /// # Panics
    ///
    /// Panics when `divisor` is zero.
    #[must_use]
    pub fn divrem_small(&self, divisor: u64) -> (Self, u64) {
        assert!(divisor != 0, "BigUint::divrem_small by zero");
        let mut quotient = vec![0u64; self.limbs.len()];
        let mut remainder = 0u128;
        for index in (0..self.limbs.len()).rev() {
            let current = (remainder << LIMB_BITS) | u128::from(self.limbs[index]);
            quotient[index] = (current / u128::from(divisor)) as u64;
            remainder = current % u128::from(divisor);
        }
        (Self::from_limbs_le(quotient), remainder as u64)
    }

    /// The value shifted left by a bit count.
    #[must_use]
    pub fn shl_bits(&self, shift: u64) -> Self {
        if self.is_zero() {
            return Self::zero();
        }
        let limb_shift = (shift / u64::from(LIMB_BITS)) as usize;
        let bit_shift = (shift % u64::from(LIMB_BITS)) as u32;
        let mut out = vec![0u64; limb_shift];
        if bit_shift == 0 {
            out.extend_from_slice(&self.limbs);
        } else {
            let mut carry = 0u64;
            for &limb in &self.limbs {
                out.push((limb << bit_shift) | carry);
                carry = limb >> (LIMB_BITS - bit_shift);
            }
            out.push(carry);
        }
        Self::from_limbs_le(out)
    }

    /// The value shifted right by a bit count, discarding the shifted-out bits.
    #[must_use]
    pub fn shr_bits(&self, shift: u64) -> Self {
        let limb_shift = (shift / u64::from(LIMB_BITS)) as usize;
        if limb_shift >= self.limbs.len() {
            return Self::zero();
        }
        let bit_shift = (shift % u64::from(LIMB_BITS)) as u32;
        let kept = &self.limbs[limb_shift..];
        if bit_shift == 0 {
            return Self::from_limbs_le(kept.to_vec());
        }
        let mut out = Vec::with_capacity(kept.len());
        for (index, &limb) in kept.iter().enumerate() {
            let high = kept.get(index + 1).copied().unwrap_or(0);
            out.push((limb >> bit_shift) | (high << (LIMB_BITS - bit_shift)));
        }
        Self::from_limbs_le(out)
    }
}

impl PartialOrd for BigUint {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BigUint {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_value(other)
    }
}

impl fmt::Display for BigUint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_dec_string())
    }
}

impl fmt::Debug for BigUint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "BigUint({})", self.to_dec_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> BigUint {
        BigUint::parse_dec(text).expect("vector operand parses")
    }

    #[test]
    fn zero_has_exactly_one_encoding() {
        assert!(BigUint::zero().is_zero());
        assert!(BigUint::from_limbs_le(vec![0, 0, 0]).is_zero());
        assert_eq!(BigUint::from_limbs_le(vec![0, 0]), BigUint::zero());
        assert_eq!(BigUint::zero().limbs(), &[] as &[u64]);
        assert_eq!(BigUint::zero().to_dec_string(), "0");
    }

    #[test]
    fn addition_carries_out_of_the_top_limb() {
        let a = parse("18446744073709551615");
        let sum = a.add(&BigUint::one());
        assert_eq!(sum.to_dec_string(), "18446744073709551616");
        assert_eq!(sum.limbs(), &[0, 1]);
    }

    #[test]
    fn subtraction_to_zero_normalizes_away_every_limb() {
        let a = parse("222232244629420445529739893461909967206666939096499764990979600");
        let zero = a.sub(&a);
        assert!(zero.is_zero());
        assert_eq!(zero.limbs(), &[] as &[u64]);
    }

    #[test]
    fn subtraction_below_zero_reports_rather_than_wraps() {
        assert_eq!(
            BigUint::from_u64(5).checked_sub(&BigUint::from_u64(7)),
            None
        );
    }

    #[test]
    fn borrow_propagates_the_full_length() {
        let a = parse("340282366920938463463374607431768211456");
        assert_eq!(
            a.sub(&BigUint::one()).to_dec_string(),
            "340282366920938463463374607431768211455"
        );
    }

    #[test]
    fn ordering_is_by_value_not_by_limb_vector() {
        assert!(parse("18446744073709551616") > parse("18446744073709551615"));
        assert!(parse("0") < parse("1"));
        assert_eq!(parse("7").cmp_value(&parse("7")), Ordering::Equal);
    }

    #[test]
    fn shifts_move_whole_limbs_and_partial_limbs_alike() {
        let one = BigUint::one();
        assert_eq!(one.shl_bits(64).limbs(), &[0, 1]);
        assert_eq!(one.shl_bits(65).limbs(), &[0, 2]);
        assert_eq!(one.shl_bits(64).shr_bits(64), one);
        assert_eq!(one.shl_bits(130).shr_bits(130), one);
        assert!(one.shr_bits(1).is_zero());
        assert!(BigUint::zero().shl_bits(1000).is_zero());
        let wide = parse("340282366920938463463374607431768211455");
        assert_eq!(wide.shl_bits(3).shr_bits(3), wide);
        assert_eq!(
            wide.shr_bits(1).to_dec_string(),
            "170141183460469231731687303715884105727"
        );
    }

    #[test]
    fn bit_length_counts_significant_bits_only() {
        assert_eq!(BigUint::zero().bit_len(), 0);
        assert_eq!(BigUint::one().bit_len(), 1);
        assert_eq!(parse("18446744073709551615").bit_len(), 64);
        assert_eq!(parse("18446744073709551616").bit_len(), 65);
    }

    #[test]
    fn trailing_zeros_skip_whole_zero_limbs() {
        assert_eq!(BigUint::one().shl_bits(200).trailing_zeros(), 200);
        assert_eq!(BigUint::from_u64(12).trailing_zeros(), 2);
        assert_eq!(BigUint::zero().trailing_zeros(), 0);
    }

    #[test]
    fn small_multiplication_and_division_invert_each_other() {
        let a = parse("222232244629420445529739893461909967206666939096499764990979600");
        let scaled = a.mul_small(u64::MAX);
        let (back, remainder) = scaled.divrem_small(u64::MAX);
        assert_eq!(remainder, 0);
        assert_eq!(back, a);
        assert!(a.mul_small(0).is_zero());
    }

    #[test]
    fn machine_word_views_report_absence_rather_than_truncating() {
        assert_eq!(BigUint::zero().to_u64(), Some(0));
        assert_eq!(parse("18446744073709551615").to_u64(), Some(u64::MAX));
        assert_eq!(parse("18446744073709551616").to_u64(), None);
        assert_eq!(
            parse("340282366920938463463374607431768211455").to_u128(),
            Some(u128::MAX)
        );
        assert_eq!(
            parse("340282366920938463463374607431768211456").to_u128(),
            None
        );
        assert_eq!(
            BigUint::from_u128(u128::MAX).to_dec_string(),
            "340282366920938463463374607431768211455"
        );
        assert!(BigUint::from_u128(0).is_zero());
    }

    #[test]
    fn parity_matches_the_low_bit() {
        assert!(BigUint::zero().is_even());
        assert!(!BigUint::one().is_even());
        assert!(parse("18446744073709551616").is_even());
    }
}
