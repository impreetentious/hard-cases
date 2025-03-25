//! The binary greatest common divisor.
//!
//! Every rational in this crate is reduced on construction, so this runs on
//! the path of essentially every exact operation the solver performs. The
//! binary algorithm is used rather than the Euclidean one because its inner
//! loop is a subtraction and a shift instead of a division, and division is by
//! a wide margin the most expensive primitive here.
//!
//! The two cases worth naming are the ones the vectors pin: an operand of
//! zero, which a trailing-zero loop spins on forever unless it is handled
//! before the loop, and a pair sharing a large power of two, which is the only
//! way to reach the common-shift path with a nontrivial shift.

use crate::uint::BigUint;

impl BigUint {
    /// The greatest common divisor. `gcd(0, b)` is `b` and `gcd(0, 0)` is zero.
    #[must_use]
    pub fn gcd(&self, other: &Self) -> Self {
        if self.is_zero() {
            return other.clone();
        }
        if other.is_zero() {
            return self.clone();
        }

        let common = core::cmp::min(self.trailing_zeros(), other.trailing_zeros());
        let mut left = self.shr_bits(self.trailing_zeros());
        let mut right = other.clone();

        loop {
            right = right.shr_bits(right.trailing_zeros());
            if left.cmp_value(&right) == core::cmp::Ordering::Greater {
                core::mem::swap(&mut left, &mut right);
            }
            right = right.sub(&left);
            if right.is_zero() {
                break;
            }
        }

        left.shl_bits(common)
    }

    /// The least common multiple. `lcm(0, b)` is zero.
    #[must_use]
    pub fn lcm(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        // Dividing before multiplying keeps the intermediate no larger than
        // the answer, which matters when both operands are already near a cap.
        self.div(&self.gcd(other)).mul(other)
    }
}

#[cfg(test)]
mod tests {
    use crate::uint::BigUint;

    fn parse(text: &str) -> BigUint {
        BigUint::parse_dec(text).expect("vector operand parses")
    }

    #[test]
    fn zero_is_the_identity_and_does_not_spin() {
        let a = parse("2432902008176640000");
        assert_eq!(BigUint::zero().gcd(&a), a);
        assert_eq!(a.gcd(&BigUint::zero()), a);
        assert!(BigUint::zero().gcd(&BigUint::zero()).is_zero());
    }

    #[test]
    fn consecutive_fibonacci_numbers_are_coprime() {
        let a = parse("280571172992510140037611932413038677189525");
        let b = parse("453973694165307953197296969697410619233826");
        assert_eq!(a.gcd(&b).to_dec_string(), "1");
        assert_eq!(b.gcd(&a).to_dec_string(), "1");
    }

    #[test]
    fn a_shared_power_of_two_survives_the_common_shift() {
        let a = parse("910043815000214977332758527534256632492715260325658624");
        let b = parse("3026129320496825594530516643625426310831441226804886503424");
        let expected = "867885413170065858204611327680832512371745357824";
        assert_eq!(a.gcd(&b).to_dec_string(), expected);
        assert_eq!(b.gcd(&a).to_dec_string(), expected);
    }

    #[test]
    fn the_divisor_actually_divides_and_is_greatest() {
        let a = parse("910043815000214977332758527534256632492715260325658624");
        let b = parse("3026129320496825594530516643625426310831441226804886503424");
        let divisor = a.gcd(&b);
        assert!(a.divides_evenly(&divisor));
        assert!(b.divides_evenly(&divisor));
        // Dividing out leaves a coprime pair, which is the property reduction
        // depends on.
        assert_eq!(a.div(&divisor).gcd(&b.div(&divisor)).to_dec_string(), "1");
    }

    #[test]
    fn a_value_divides_itself_and_one_divides_everything() {
        let a = parse("265252859812191058636308480000000");
        assert_eq!(a.gcd(&a), a);
        assert_eq!(a.gcd(&BigUint::one()).to_dec_string(), "1");
    }

    #[test]
    fn a_planted_divisor_is_recovered_across_the_recursive_threshold() {
        let divisor = BigUint::pow10(700).add_small(1);
        let a = divisor.mul(&BigUint::pow10(11).add_small(3));
        let b = divisor.mul(&BigUint::pow10(13).add_small(7));
        assert_eq!(a.gcd(&b), divisor);
    }

    #[test]
    fn the_least_common_multiple_is_the_product_over_the_divisor() {
        let a = parse("910043815000214977332758527534256632492715260325658624");
        let b = parse("3026129320496825594530516643625426310831441226804886503424");
        assert_eq!(a.lcm(&b).mul(&a.gcd(&b)), a.mul(&b));
        assert!(BigUint::zero().lcm(&a).is_zero());
        assert!(a.lcm(&BigUint::zero()).is_zero());
        assert_eq!(
            BigUint::from_u64(4)
                .lcm(&BigUint::from_u64(6))
                .to_dec_string(),
            "12"
        );
    }
}
