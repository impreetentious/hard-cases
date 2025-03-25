//! The one place a rational becomes a floating-point number.
//!
//! The search runs in binary64 and needs the model's coefficients in that
//! form. Nothing it computes there is ever a conclusion, so this conversion is
//! not part of any proof — but it still has to be *correct*, because the
//! numeric acceptance fence is written in terms of it: a model value that does
//! not round to a nonzero finite binary64 is rejected, on the grounds that the
//! search would otherwise be working on a different model than the one being
//! proved about, with a structural zero where the proof has a coefficient.
//!
//! The conversion is correctly rounded to nearest with ties to even, computed
//! by exact integer division rather than by any floating-point arithmetic. The
//! result is assembled from its bit pattern, so the subnormal range and the
//! overflow boundary are handled by the same code path as everything else
//! instead of by a sequence of multiplications that could round twice.

use core::cmp::Ordering;

use crate::int::BigInt;
use crate::rational::Rational;
use crate::uint::BigUint;

/// Significand width, including the implicit leading bit.
const SIGNIFICAND_BITS: i64 = 53;
/// The exponent of the smallest positive subnormal.
const MIN_SUBNORMAL_EXPONENT: i64 = -1074;
/// The exponent bias, and the width of the stored fraction field.
const EXPONENT_BIAS: i64 = 1023;
const FRACTION_BITS: u32 = 52;
/// The biased exponent that means infinity or not-a-number.
const BIASED_INFINITY: i64 = 2047;

impl Rational {
    /// The value rounded to binary64, to nearest with ties to even.
    ///
    /// Overflows to an infinity and underflows to a zero exactly as the
    /// floating-point format requires. Callers that need to know whether
    /// either happened ask
    /// [`Rational::rounds_to_nonzero_finite_binary64`] rather than inspecting
    /// the result, so the question is asked in the words the fence uses.
    #[must_use]
    pub fn to_binary64(&self) -> f64 {
        if self.is_zero() {
            return 0.0;
        }
        let negative = self.is_negative();
        let numerator = self.numerator().magnitude();
        let denominator = self.denominator();

        // Start where the bit lengths say the quotient has a significand's
        // worth of bits, then step until it does. The estimate is off by at
        // most one in either direction.
        let mut exponent =
            numerator.bit_len() as i64 - denominator.bit_len() as i64 - SIGNIFICAND_BITS;
        exponent = exponent.max(MIN_SUBNORMAL_EXPONENT);
        let mut scaled = scaled_divrem(numerator, denominator, exponent);
        while scaled.quotient.bit_len() as i64 > SIGNIFICAND_BITS {
            exponent += 1;
            scaled = scaled_divrem(numerator, denominator, exponent);
        }
        while (scaled.quotient.bit_len() as i64) < SIGNIFICAND_BITS
            && exponent > MIN_SUBNORMAL_EXPONENT
        {
            exponent -= 1;
            scaled = scaled_divrem(numerator, denominator, exponent);
        }

        // Round to nearest, ties to even. Doubling the remainder and comparing
        // against the divisor is the tie test without a division.
        let doubled = scaled.remainder.shl_bits(1);
        let round_up = match doubled.cmp_value(&scaled.divisor) {
            Ordering::Greater => true,
            Ordering::Equal => !scaled.quotient.is_even(),
            Ordering::Less => false,
        };
        let mut significand = scaled.quotient;
        if round_up {
            significand = significand.add_small(1);
            if significand.bit_len() as i64 > SIGNIFICAND_BITS {
                // Rounding carried out of the significand, which is exact:
                // the value is now a power of two one exponent higher.
                significand = significand.shr_bits(1);
                exponent += 1;
            }
        }

        let Some(mantissa) = significand.to_u64() else {
            unreachable!("a significand of at most 53 bits fits in a limb")
        };
        if mantissa == 0 {
            return if negative { -0.0 } else { 0.0 };
        }

        let bits = if significand.bit_len() as i64 == SIGNIFICAND_BITS {
            let biased = exponent + i64::from(FRACTION_BITS) + EXPONENT_BIAS;
            if biased >= BIASED_INFINITY {
                return if negative {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                };
            }
            debug_assert!(
                biased >= 1,
                "a full significand cannot land in the subnormal range"
            );
            ((biased as u64) << FRACTION_BITS) | (mantissa - (1u64 << FRACTION_BITS))
        } else {
            // Fewer bits than a full significand only happens at the subnormal
            // floor, where the stored bits are the value itself.
            debug_assert_eq!(exponent, MIN_SUBNORMAL_EXPONENT);
            mantissa
        };

        f64::from_bits(if negative { bits | (1u64 << 63) } else { bits })
    }

    /// Whether the value is inside the numeric acceptance fence.
    ///
    /// An exact zero passes: it is representable and it means what it says.
    /// Every other value must round to a binary64 that is both finite and
    /// nonzero, so that the search and the proof are working on the same
    /// model.
    #[must_use]
    pub fn rounds_to_nonzero_finite_binary64(&self) -> bool {
        if self.is_zero() {
            return true;
        }
        let rounded = self.to_binary64();
        rounded.is_finite() && rounded != 0.0
    }

    /// The exact rational a finite binary64 names, or `None` for an infinity
    /// or a not-a-number.
    ///
    /// Every finite binary64 is a dyadic rational, so this direction never
    /// rounds. It exists for tests and for reports that must state exactly
    /// which number the search was holding, and no proof rule reads it.
    #[must_use]
    pub fn from_binary64(value: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        if value == 0.0 {
            return Some(Self::zero());
        }
        let bits = value.to_bits();
        let negative = bits >> 63 == 1;
        let biased = ((bits >> FRACTION_BITS) & 0x7ff) as i64;
        let fraction = bits & ((1u64 << FRACTION_BITS) - 1);

        let (mantissa, exponent) = if biased == 0 {
            (fraction, MIN_SUBNORMAL_EXPONENT)
        } else {
            (
                fraction | (1u64 << FRACTION_BITS),
                biased - EXPONENT_BIAS - i64::from(FRACTION_BITS),
            )
        };

        let magnitude = BigUint::from_u64(mantissa);
        let value = if exponent >= 0 {
            Self::from_int(BigInt::from_sign_magnitude(
                negative,
                magnitude.shl_bits(exponent.unsigned_abs()),
            ))
        } else {
            Self::new(
                BigInt::from_sign_magnitude(negative, magnitude),
                BigInt::from_magnitude(BigUint::one().shl_bits(exponent.unsigned_abs())),
            )
            .expect("a power of two is never zero")
        };
        Some(value)
    }
}

/// A division of the numerator by the denominator scaled by a power of two.
struct Scaled {
    quotient: BigUint,
    remainder: BigUint,
    divisor: BigUint,
}

/// Divides `numerator` by `denominator * 2^exponent`, exactly.
fn scaled_divrem(numerator: &BigUint, denominator: &BigUint, exponent: i64) -> Scaled {
    if exponent >= 0 {
        let divisor = denominator.shl_bits(exponent.unsigned_abs());
        let (quotient, remainder) = numerator.divrem(&divisor);
        Scaled {
            quotient,
            remainder,
            divisor,
        }
    } else {
        let shifted = numerator.shl_bits(exponent.unsigned_abs());
        let (quotient, remainder) = shifted.divrem(denominator);
        Scaled {
            quotient,
            remainder,
            divisor: denominator.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(text: &str) -> Rational {
        Rational::parse_decimal(text).expect("token parses")
    }

    fn ratio(numerator: i64, denominator: i64) -> Rational {
        Rational::new(BigInt::from_i64(numerator), BigInt::from_i64(denominator))
            .expect("nonzero denominator")
    }

    #[test]
    fn exactly_representable_values_convert_without_rounding() {
        assert_eq!(decimal("0").to_binary64(), 0.0);
        assert_eq!(decimal("1").to_binary64(), 1.0);
        assert_eq!(decimal("-1").to_binary64(), -1.0);
        assert_eq!(decimal("12.375").to_binary64(), 12.375);
        assert_eq!(decimal("150").to_binary64(), 150.0);
        assert_eq!(ratio(1, 2).to_binary64(), 0.5);
        assert_eq!(ratio(-3, 4).to_binary64(), -0.75);
    }

    #[test]
    fn inexact_values_land_on_the_nearest_binary64() {
        assert_eq!(decimal("0.1").to_binary64(), 0.1);
        assert_eq!(decimal("-0.1").to_binary64(), -0.1);
        assert_eq!(ratio(1, 3).to_binary64(), 1.0 / 3.0);
        assert_eq!(ratio(2, 3).to_binary64(), 2.0 / 3.0);
        assert_eq!(
            decimal("0.3333333333333333").to_binary64(),
            0.333_333_333_333_333_3
        );
    }

    #[test]
    fn ties_round_to_even() {
        // Halfway between two adjacent representable values at the top of the
        // significand: 2^53 and 2^53 + 2 are representable, 2^53 + 1 is the
        // tie, and the even neighbour is 2^53.
        let tie = Rational::from_int(BigInt::from_u64((1u64 << 53) + 1));
        assert_eq!(tie.to_binary64(), 9_007_199_254_740_992.0);
        let above = Rational::from_int(BigInt::from_u64((1u64 << 53) + 3));
        assert_eq!(above.to_binary64(), 9_007_199_254_740_996.0);
        // And the same tie one exponent down, where the even neighbour is the
        // upper one instead.
        let other = Rational::from_int(BigInt::from_u64((1u64 << 53) + 5));
        assert_eq!(other.to_binary64(), 9_007_199_254_740_996.0);
    }

    #[test]
    fn every_finite_binary64_round_trips_through_the_exact_form() {
        let samples = [
            0.0,
            1.0,
            -1.0,
            0.1,
            -0.1,
            12.375,
            1.0 / 3.0,
            f64::MAX,
            f64::MIN,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::from_bits(0x000f_ffff_ffff_ffff),
            9_007_199_254_740_993.0,
            f64::EPSILON,
        ];
        for value in samples {
            let exact = Rational::from_binary64(value).expect("finite");
            assert_eq!(exact.to_binary64(), value, "for {value}");
        }
        assert_eq!(Rational::from_binary64(f64::INFINITY), None);
        assert_eq!(Rational::from_binary64(f64::NAN), None);
    }

    #[test]
    fn the_smallest_subnormal_and_its_neighbours_are_exact() {
        let smallest = f64::from_bits(1);
        let exact = Rational::from_binary64(smallest).expect("finite");
        assert_eq!(exact.to_binary64(), smallest);
        // Half of the smallest subnormal is a tie against zero, and zero is
        // the even neighbour.
        let half = exact.mul(&ratio(1, 2));
        assert_eq!(half.to_binary64(), 0.0);
        assert!(!half.rounds_to_nonzero_finite_binary64());
        // Just above the tie rounds up to the smallest subnormal.
        let above = half.mul(&ratio(3, 2));
        assert_eq!(above.to_binary64(), smallest);
        assert!(above.rounds_to_nonzero_finite_binary64());
    }

    #[test]
    fn overflow_and_underflow_are_outside_the_fence() {
        let huge = Rational::from_int(BigInt::from_magnitude(BigUint::pow10(400)));
        assert!(huge.to_binary64().is_infinite());
        assert!(!huge.rounds_to_nonzero_finite_binary64());
        assert!(huge.neg().to_binary64().is_infinite());
        assert!(huge.neg().to_binary64().is_sign_negative());

        let tiny = huge.recip().expect("nonzero");
        assert_eq!(tiny.to_binary64(), 0.0);
        assert!(!tiny.rounds_to_nonzero_finite_binary64());
    }

    #[test]
    fn the_overflow_boundary_is_where_the_format_says_it_is() {
        let max = Rational::from_binary64(f64::MAX).expect("finite");
        assert_eq!(max.to_binary64(), f64::MAX);
        assert!(max.rounds_to_nonzero_finite_binary64());
        // The midpoint between the largest finite value and the next power of
        // two is a tie, and the even neighbour is the infinity.
        let ulp = Rational::from_binary64(f64::MAX).expect("finite").sub(
            &Rational::from_binary64({
                let __binary64_neighbor_value: f64 = f64::MAX;
                if __binary64_neighbor_value.is_nan()
                    || __binary64_neighbor_value == f64::NEG_INFINITY
                {
                    __binary64_neighbor_value
                } else if __binary64_neighbor_value == 0.0 {
                    f64::from_bits((1u64 << 63) | 1)
                } else {
                    f64::from_bits(if __binary64_neighbor_value > 0.0 {
                        __binary64_neighbor_value.to_bits() - 1
                    } else {
                        __binary64_neighbor_value.to_bits() + 1
                    })
                }
            })
            .expect("finite"),
        );
        let midpoint = max.add(&ulp.mul(&ratio(1, 2)));
        assert!(midpoint.to_binary64().is_infinite());
        let below = max.add(&ulp.mul(&ratio(1, 4)));
        assert_eq!(below.to_binary64(), f64::MAX);
    }

    #[test]
    fn zero_is_exempt_from_the_fence() {
        assert!(Rational::zero().rounds_to_nonzero_finite_binary64());
        assert_eq!(Rational::zero().to_binary64(), 0.0);
    }

    #[test]
    fn a_value_a_float_cannot_distinguish_still_converts_to_the_nearer_side() {
        // One plus two to the minus fifty-third is the tie against one, and
        // one is the even neighbour. One plus two to the minus fifty-second is
        // representable and must not collapse.
        let tie = Rational::one().add(&ratio(1, 1).mul(&two_to_the_minus(53)));
        assert_eq!(tie.to_binary64(), 1.0);
        let representable = Rational::one().add(&two_to_the_minus(52));
        assert!(representable.to_binary64() > 1.0);
    }

    fn two_to_the_minus(bits: u64) -> Rational {
        Rational::new(
            BigInt::one(),
            BigInt::from_magnitude(BigUint::one().shl_bits(bits)),
        )
        .expect("a power of two is never zero")
    }
}
