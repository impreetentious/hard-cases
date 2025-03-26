//! The verifier's arbitrary-precision integers.
//!
//! Built to be unlike the solver's on purpose. The limb is thirty-two bits
//! wide rather than sixty-four, so every intermediate product fits a machine
//! word with room to spare and there is no widening step to get wrong.
//! Multiplication is schoolbook and only schoolbook: the recursive algorithm
//! the solver uses is faster and has a split size to be wrong about, and the
//! quantities here are bounded by the certificate caps rather than by the size
//! of a model. Division finds each quotient digit by estimating it from the
//! leading limbs and then walking the estimate down until the product fits,
//! which needs no correction step and no proof that the correction terminates
//! — the loop is the correction. The greatest common divisor is Euclid's, not
//! the binary one.
//!
//! None of those choices is an accident and none is a preference. Two
//! implementations that make the same choices tend to fail on the same inputs,
//! and this one exists precisely to fail somewhere else.

use core::cmp::Ordering;
use core::fmt;

/// Bits per limb.
const LIMB: u32 = 32;
/// The radix, as the wider type every intermediate is computed in.
const RADIX: u64 = 1 << LIMB;
/// The largest power of ten below the radix, and its exponent.
const DECIMAL_CHUNK: u32 = 1_000_000_000;
const DECIMAL_CHUNK_DIGITS: usize = 9;

/// A nonnegative arbitrary-precision integer.
///
/// Little-endian base two-to-the-thirty-second digits with no leading zero
/// digit, so zero is the empty digit list and each value has one encoding.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct Nat {
    digits: Vec<u32>,
}

impl Nat {
    /// Zero.
    #[must_use]
    pub const fn zero() -> Self {
        Self { digits: Vec::new() }
    }

    /// One.
    #[must_use]
    pub fn one() -> Self {
        Self::from_u32(1)
    }

    /// A value from one digit.
    #[must_use]
    pub fn from_u32(value: u32) -> Self {
        if value == 0 {
            Self::zero()
        } else {
            Self {
                digits: vec![value],
            }
        }
    }

    /// A value from a machine word.
    #[must_use]
    pub fn from_u64(value: u64) -> Self {
        Self::from_digits(vec![(value % RADIX) as u32, (value / RADIX) as u32])
    }

    /// A value from a double-width machine word.
    #[must_use]
    pub fn from_u128(value: u128) -> Self {
        let mut digits = Vec::new();
        let mut rest = value;
        while rest != 0 {
            digits.push((rest % u128::from(RADIX)) as u32);
            rest /= u128::from(RADIX);
        }
        Self::from_digits(digits)
    }

    /// A value from digits, least significant first, trimming leading zeros.
    #[must_use]
    pub fn from_digits(mut digits: Vec<u32>) -> Self {
        while digits.last() == Some(&0) {
            digits.pop();
        }
        Self { digits }
    }

    /// The digits, least significant first.
    #[must_use]
    pub fn digits(&self) -> &[u32] {
        &self.digits
    }

    /// Whether the value is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    /// Whether the value is one.
    #[must_use]
    pub fn is_one(&self) -> bool {
        self.digits.len() == 1 && self.digits[0] == 1
    }

    /// The value as a machine word, when it fits.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self.digits.len() {
            0 => Some(0),
            1 => Some(u64::from(self.digits[0])),
            2 => Some(u64::from(self.digits[0]) + u64::from(self.digits[1]) * RADIX),
            _ => None,
        }
    }

    /// The number of significant bits.
    #[must_use]
    pub fn bits(&self) -> u64 {
        match self.digits.last() {
            None => 0,
            Some(top) => {
                (self.digits.len() as u64 - 1) * u64::from(LIMB)
                    + u64::from(LIMB - top.leading_zeros())
            }
        }
    }

    /// Comparison by value.
    #[must_use]
    pub fn compare(&self, other: &Self) -> Ordering {
        if self.digits.len() != other.digits.len() {
            return self.digits.len().cmp(&other.digits.len());
        }
        for index in (0..self.digits.len()).rev() {
            if self.digits[index] != other.digits[index] {
                return self.digits[index].cmp(&other.digits[index]);
            }
        }
        Ordering::Equal
    }

    /// Sum.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        let width = self.digits.len().max(other.digits.len());
        let mut digits = Vec::with_capacity(width + 1);
        let mut carry = 0u64;
        for index in 0..width {
            let total = carry + u64::from(self.digit(index)) + u64::from(other.digit(index));
            digits.push((total % RADIX) as u32);
            carry = total / RADIX;
        }
        if carry != 0 {
            digits.push(carry as u32);
        }
        Self::from_digits(digits)
    }

    /// Difference, or `None` when it would be negative.
    #[must_use]
    pub fn minus(&self, other: &Self) -> Option<Self> {
        if self.compare(other) == Ordering::Less {
            return None;
        }
        let mut digits = Vec::with_capacity(self.digits.len());
        let mut borrow = 0i64;
        for index in 0..self.digits.len() {
            let mut total = i64::from(self.digit(index)) - i64::from(other.digit(index)) - borrow;
            if total < 0 {
                total += RADIX as i64;
                borrow = 1;
            } else {
                borrow = 0;
            }
            digits.push(total as u32);
        }
        Some(Self::from_digits(digits))
    }

    /// Product, computed digit by digit.
    #[must_use]
    pub fn times(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        let mut digits = vec![0u32; self.digits.len() + other.digits.len()];
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &right) in other.digits.iter().enumerate() {
                let total = u64::from(left) * u64::from(right) + u64::from(digits[i + j]) + carry;
                digits[i + j] = (total % RADIX) as u32;
                carry = total / RADIX;
            }
            let mut at = i + other.digits.len();
            while carry != 0 {
                let total = u64::from(digits[at]) + carry;
                digits[at] = (total % RADIX) as u32;
                carry = total / RADIX;
                at += 1;
            }
        }
        Self::from_digits(digits)
    }

    /// Product with one digit.
    #[must_use]
    pub fn times_digit(&self, factor: u32) -> Self {
        if factor == 0 || self.is_zero() {
            return Self::zero();
        }
        let mut digits = Vec::with_capacity(self.digits.len() + 1);
        let mut carry = 0u64;
        for &digit in &self.digits {
            let total = u64::from(digit) * u64::from(factor) + carry;
            digits.push((total % RADIX) as u32);
            carry = total / RADIX;
        }
        if carry != 0 {
            digits.push(carry as u32);
        }
        Self::from_digits(digits)
    }

    /// Quotient and remainder, or `None` when the divisor is zero.
    ///
    /// Each quotient digit is estimated from the leading digits and then
    /// lowered until the divisor times the estimate fits inside what is left.
    /// With the divisor normalized so its leading digit has its top bit set,
    /// the estimate is never more than two too large, so the loop below runs
    /// at most twice — but the code does not depend on that bound being right,
    /// which is the reason it is written as a loop and not as a correction.
    #[must_use]
    pub fn divide(&self, divisor: &Self) -> Option<(Self, Self)> {
        if divisor.is_zero() {
            return None;
        }
        if self.compare(divisor) == Ordering::Less {
            return Some((Self::zero(), self.clone()));
        }

        let shift = u64::from(divisor.digits[divisor.digits.len() - 1].leading_zeros());
        let scaled_divisor = divisor.shift_up(shift);
        let scaled_dividend = self.shift_up(shift);
        let width = scaled_divisor.digits.len();
        let top = u64::from(scaled_divisor.digits[width - 1]);

        let mut quotient = vec![0u32; scaled_dividend.digits.len()];
        let mut remainder = Self::zero();

        for position in (0..scaled_dividend.digits.len()).rev() {
            // Bring the next digit down: shift the running remainder up by one
            // digit and place the dividend's digit in the vacated position.
            let mut digits = Vec::with_capacity(remainder.digits.len() + 1);
            digits.push(scaled_dividend.digits[position]);
            digits.extend_from_slice(&remainder.digits);
            remainder = Self::from_digits(digits);

            if remainder.compare(&scaled_divisor) == Ordering::Less {
                continue;
            }

            // The running remainder is below the divisor times the radix, so
            // it has at most one digit more than the divisor and the estimate
            // below is computed from at most two digits.
            let length = remainder.digits.len();
            let leading = if length > width {
                u64::from(remainder.digits[length - 1]) * RADIX
                    + u64::from(remainder.digits[length - 2])
            } else {
                u64::from(remainder.digits[length - 1])
            };
            let mut estimate = (leading / top).min(RADIX - 1) as u32;

            let mut product = scaled_divisor.times_digit(estimate);
            while product.compare(&remainder) == Ordering::Greater {
                estimate -= 1;
                product = scaled_divisor.times_digit(estimate);
            }

            remainder = remainder
                .minus(&product)
                .expect("the product no longer exceeds the remainder");
            quotient[position] = estimate;
        }

        Some((Self::from_digits(quotient), remainder.shift_down(shift)))
    }

    /// The greatest common divisor, by Euclid's algorithm.
    #[must_use]
    pub fn gcd(&self, other: &Self) -> Self {
        let mut left = self.clone();
        let mut right = other.clone();
        while !right.is_zero() {
            let (_, remainder) = left.divide(&right).expect("the divisor is not zero here");
            left = right;
            right = remainder;
        }
        left
    }

    /// The value with `bits` zero bits appended.
    #[must_use]
    pub fn shift_up(&self, bits: u64) -> Self {
        if self.is_zero() {
            return Self::zero();
        }
        let whole = (bits / u64::from(LIMB)) as usize;
        let partial = (bits % u64::from(LIMB)) as u32;
        let mut digits = vec![0u32; whole];
        if partial == 0 {
            digits.extend_from_slice(&self.digits);
        } else {
            let mut carry = 0u32;
            for &digit in &self.digits {
                digits.push((digit << partial) | carry);
                carry = digit >> (LIMB - partial);
            }
            digits.push(carry);
        }
        Self::from_digits(digits)
    }

    /// The value with its low `bits` bits discarded.
    #[must_use]
    pub fn shift_down(&self, bits: u64) -> Self {
        let whole = (bits / u64::from(LIMB)) as usize;
        if whole >= self.digits.len() {
            return Self::zero();
        }
        let partial = (bits % u64::from(LIMB)) as u32;
        let kept = &self.digits[whole..];
        if partial == 0 {
            return Self::from_digits(kept.to_vec());
        }
        let mut digits = Vec::with_capacity(kept.len());
        for index in 0..kept.len() {
            let low = kept[index] >> partial;
            let high = if index + 1 < kept.len() {
                kept[index + 1] << (LIMB - partial)
            } else {
                0
            };
            digits.push(low | high);
        }
        Self::from_digits(digits)
    }

    /// Ten raised to an exponent.
    #[must_use]
    pub fn power_of_ten(exponent: u32) -> Self {
        let mut value = Self::one();
        let mut left = exponent;
        while left >= DECIMAL_CHUNK_DIGITS as u32 {
            value = value.times_digit(DECIMAL_CHUNK);
            left -= DECIMAL_CHUNK_DIGITS as u32;
        }
        for _ in 0..left {
            value = value.times_digit(10);
        }
        value
    }

    /// Reads a decimal magnitude of at least one digit.
    ///
    /// # Errors
    ///
    /// Returns [`NatError::NotDecimal`] for anything that is not digits.
    pub fn from_decimal(text: &str) -> Result<Self, NatError> {
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(NatError::NotDecimal);
        }
        let mut value = Self::zero();
        let bytes = text.as_bytes();
        let mut at = 0usize;
        while at < bytes.len() {
            let take = DECIMAL_CHUNK_DIGITS.min(bytes.len() - at);
            let mut chunk = 0u32;
            let mut scale = 1u32;
            for &byte in &bytes[at..at + take] {
                chunk = chunk * 10 + u32::from(byte - b'0');
                scale *= 10;
            }
            value = value.times_digit(scale).plus(&Self::from_u32(chunk));
            at += take;
        }
        Ok(value)
    }

    /// Renders the value in decimal.
    #[must_use]
    pub fn to_decimal(&self) -> String {
        if self.is_zero() {
            return String::from("0");
        }
        let mut chunks: Vec<u32> = Vec::new();
        let mut rest = self.clone();
        let divisor = Self::from_u32(DECIMAL_CHUNK);
        while !rest.is_zero() {
            let (quotient, remainder) = rest.divide(&divisor).expect("the divisor is not zero");
            chunks.push(remainder.digits.first().copied().unwrap_or(0));
            rest = quotient;
        }
        let mut out = String::new();
        for (index, chunk) in chunks.iter().rev().enumerate() {
            if index == 0 {
                out.push_str(&chunk.to_string());
            } else {
                let text = chunk.to_string();
                for _ in text.len()..DECIMAL_CHUNK_DIGITS {
                    out.push('0');
                }
                out.push_str(&text);
            }
        }
        out
    }

    /// The digit at a position, or zero past the end.
    fn digit(&self, index: usize) -> u32 {
        self.digits.get(index).copied().unwrap_or(0)
    }
}

impl fmt::Display for Nat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_decimal())
    }
}

impl fmt::Debug for Nat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Nat({})", self.to_decimal())
    }
}

/// What can go wrong reading a magnitude.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NatError {
    /// The text was not a run of decimal digits.
    NotDecimal,
}

impl fmt::Display for NatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotDecimal => formatter.write_str("value is not a decimal magnitude"),
        }
    }
}

impl core::error::Error for NatError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Nat {
        Nat::from_decimal(text).expect("a decimal magnitude")
    }

    #[test]
    fn zero_has_one_encoding_and_reads_back() {
        assert!(Nat::zero().is_zero());
        assert!(Nat::from_digits(vec![0, 0, 0]).is_zero());
        assert_eq!(Nat::zero().to_decimal(), "0");
        assert_eq!(read("0"), Nat::zero());
        assert_eq!(read("000000"), Nat::zero());
        assert_eq!(Nat::zero().bits(), 0);
    }

    #[test]
    fn addition_and_subtraction_cross_every_digit_boundary() {
        assert_eq!(
            read("4294967295").plus(&Nat::one()).to_decimal(),
            "4294967296"
        );
        assert_eq!(
            read("18446744073709551615").plus(&Nat::one()).to_decimal(),
            "18446744073709551616"
        );
        assert_eq!(
            read("340282366920938463463374607431768211456")
                .minus(&Nat::one())
                .expect("nonnegative")
                .to_decimal(),
            "340282366920938463463374607431768211455"
        );
        let big = read("222232244629420445529739893461909967206666939096499764990979600");
        assert!(big.minus(&big).expect("nonnegative").is_zero());
        assert_eq!(read("5").minus(&read("7")), None);
    }

    #[test]
    fn multiplication_reaches_the_published_products() {
        assert_eq!(
            read("18446744073709551615")
                .times(&read("18446744073709551615"))
                .to_decimal(),
            "340282366920938463426481119284349108225"
        );
        assert_eq!(
            read("2432902008176640000")
                .times(&read("15511210043330985984000000"))
                .to_decimal(),
            "37737254063669622951147441236213760000000000"
        );
        assert!(read("7").times(&Nat::zero()).is_zero());
        assert_eq!(read("7").times(&Nat::one()).to_decimal(), "7");
    }

    #[test]
    fn division_reconstructs_and_bounds_its_remainder() {
        let cases = [
            (
                "115792089237316195423570985008687907853269984665640564039457584007913129639935",
                "18446744073709551617",
            ),
            (
                "6277101735386680763835789423207666416083908700390324961280",
                "340282366920938463463374607431768211455",
            ),
            ("265252859812191058636308480000000", "1"),
            ("5", "7"),
            ("100000000000000000000000000000000", "999999999"),
        ];
        for (dividend, divisor) in cases {
            let (a, b) = (read(dividend), read(divisor));
            let (quotient, remainder) = a.divide(&b).expect("nonzero divisor");
            assert_eq!(
                remainder.compare(&b),
                Ordering::Less,
                "{dividend} / {divisor}"
            );
            assert_eq!(
                quotient.times(&b).plus(&remainder),
                a,
                "{dividend} / {divisor}"
            );
        }
        assert_eq!(
            read("115792089237316195423570985008687907853269984665640564039457584007913129639935")
                .divide(&read("18446744073709551617"))
                .expect("nonzero divisor")
                .0
                .to_decimal(),
            "6277101735386680763495507056286727952657427581105975853055"
        );
        assert_eq!(
            read("6277101735386680763835789423207666416083908700390324961280")
                .divide(&read("340282366920938463463374607431768211455"))
                .expect("nonzero divisor")
                .0
                .to_decimal(),
            "18446744073709551616"
        );
        assert_eq!(Nat::one().divide(&Nat::zero()), None);
    }

    #[test]
    fn the_divisor_of_two_values_divides_both() {
        assert_eq!(
            read("280571172992510140037611932413038677189525")
                .gcd(&read("453973694165307953197296969697410619233826"))
                .to_decimal(),
            "1"
        );
        assert_eq!(
            read("910043815000214977332758527534256632492715260325658624")
                .gcd(&read(
                    "3026129320496825594530516643625426310831441226804886503424"
                ))
                .to_decimal(),
            "867885413170065858204611327680832512371745357824"
        );
        assert_eq!(
            Nat::zero().gcd(&read("2432902008176640000")).to_decimal(),
            "2432902008176640000"
        );
        assert_eq!(
            read("2432902008176640000").gcd(&Nat::zero()).to_decimal(),
            "2432902008176640000"
        );
        assert!(Nat::zero().gcd(&Nat::zero()).is_zero());
    }

    #[test]
    fn shifts_are_inverses_where_nothing_falls_off() {
        let value = read("340282366920938463463374607431768211455");
        for bits in [0u64, 1, 31, 32, 33, 64, 100, 256] {
            assert_eq!(value.shift_up(bits).shift_down(bits), value, "by {bits}");
        }
        assert_eq!(Nat::one().shift_up(32).digits(), &[0, 1]);
        assert!(Nat::one().shift_down(1).is_zero());
        assert!(Nat::zero().shift_up(1000).is_zero());
        assert_eq!(
            value.shift_down(1).to_decimal(),
            "170141183460469231731687303715884105727"
        );
    }

    #[test]
    fn decimal_text_round_trips_across_chunk_boundaries() {
        for text in [
            "0",
            "1",
            "999999999",
            "1000000000",
            "1000000001",
            "4294967296",
            "222232244629420445529739893461909967206666939096499764990979600",
        ] {
            assert_eq!(read(text).to_decimal(), text);
        }
        assert_eq!(read("00700").to_decimal(), "700");
        assert_eq!(Nat::from_decimal(""), Err(NatError::NotDecimal));
        assert_eq!(Nat::from_decimal("12a"), Err(NatError::NotDecimal));
        assert_eq!(Nat::from_decimal("-1"), Err(NatError::NotDecimal));
    }

    #[test]
    fn an_interior_decimal_chunk_keeps_its_leading_zeros() {
        let value = Nat::power_of_ten(10).plus(&Nat::from_u32(7));
        assert_eq!(value.to_decimal(), "10000000007");
    }

    #[test]
    fn powers_of_ten_land_where_they_should() {
        assert_eq!(Nat::power_of_ten(0).to_decimal(), "1");
        assert_eq!(Nat::power_of_ten(9).to_decimal(), "1000000000");
        assert_eq!(Nat::power_of_ten(10).to_decimal(), "10000000000");
        assert_eq!(Nat::power_of_ten(30).to_decimal().len(), 31);
    }

    #[test]
    fn machine_word_conversions_report_absence_rather_than_truncating() {
        assert_eq!(Nat::from_u64(u64::MAX).to_decimal(), "18446744073709551615");
        assert_eq!(Nat::from_u64(u64::MAX).as_u64(), Some(u64::MAX));
        assert_eq!(read("18446744073709551616").as_u64(), None);
        assert_eq!(
            Nat::from_u128(u128::MAX).to_decimal(),
            "340282366920938463463374607431768211455"
        );
        assert!(Nat::from_u128(0).is_zero());
        assert!(Nat::one().is_one());
        assert!(!Nat::zero().is_one());
    }

    #[test]
    fn bit_counts_match_the_leading_digit() {
        assert_eq!(Nat::one().bits(), 1);
        assert_eq!(read("4294967295").bits(), 32);
        assert_eq!(read("4294967296").bits(), 33);
        assert_eq!(read("18446744073709551615").bits(), 64);
    }
}
