//! The verifier's rationals.
//!
//! Reduced on construction, so one value has one form and equality is
//! structural. Canonical text is read and written here, and so is the decimal
//! token grammar an instance file may use — which the verifier needs because
//! it reparses the original bytes rather than trusting anything the solver
//! says about them.
//!
//! The numeric acceptance fence is decided differently here than on the solver
//! side, and deliberately so. The solver rounds the value to a binary64 and
//! looks at the result. This compares the value against the two exact
//! boundaries where rounding changes the answer — the halfway point below the
//! smallest subnormal, and the halfway point above the largest finite value —
//! using only integer comparisons. Both routes should agree on every input;
//! they are different routes so that a mistake on one of them is visible.

use core::cmp::Ordering;
use core::fmt;

use crate::vint::{Int, Nat};

/// An exact rational.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct Frac {
    /// Carries the sign.
    top: Int,
    /// Strictly positive and coprime with the top's magnitude.
    bottom: Nat,
}

impl Frac {
    /// Zero.
    #[must_use]
    pub fn zero() -> Self {
        Self {
            top: Int::zero(),
            bottom: Nat::one(),
        }
    }

    /// One.
    #[must_use]
    pub fn one() -> Self {
        Self {
            top: Int::one(),
            bottom: Nat::one(),
        }
    }

    /// A rational from an integer.
    #[must_use]
    pub fn whole(value: Int) -> Self {
        Self {
            top: value,
            bottom: Nat::one(),
        }
    }

    /// A rational from a machine word.
    #[must_use]
    pub fn from_i64(value: i64) -> Self {
        Self::whole(Int::from_i64(value))
    }

    /// A rational from a ratio, reduced, or `None` for a zero denominator.
    #[must_use]
    pub fn ratio(top: &Int, bottom: &Int) -> Option<Self> {
        if bottom.is_zero() {
            return None;
        }
        let minus = top.is_minus() != bottom.is_minus();
        let divisor = top.size().gcd(bottom.size());
        let reduced_top = top
            .size()
            .divide(&divisor)
            .expect("a divisor is never zero")
            .0;
        let reduced_bottom = bottom
            .size()
            .divide(&divisor)
            .expect("a divisor is never zero")
            .0;
        if reduced_top.is_zero() {
            return Some(Self::zero());
        }
        Some(Self {
            top: Int::signed(minus, reduced_top),
            bottom: reduced_bottom,
        })
    }

    /// The reduced numerator, carrying the sign.
    #[must_use]
    pub fn top(&self) -> &Int {
        &self.top
    }

    /// The reduced denominator, strictly positive.
    #[must_use]
    pub fn bottom(&self) -> &Nat {
        &self.bottom
    }

    /// Whether the value is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.top.is_zero()
    }

    /// Whether the value is strictly below zero.
    #[must_use]
    pub fn is_minus(&self) -> bool {
        self.top.is_minus()
    }

    /// Whether the value is strictly above zero.
    #[must_use]
    pub fn is_plus(&self) -> bool {
        self.top.is_plus()
    }

    /// Minus one, zero, or one.
    #[must_use]
    pub fn sign(&self) -> i32 {
        self.top.sign()
    }

    /// Whether the value is an integer.
    #[must_use]
    pub fn is_whole(&self) -> bool {
        self.bottom.is_one()
    }

    /// The value as an integer, when it is one.
    #[must_use]
    pub fn as_whole(&self) -> Option<Int> {
        if self.is_whole() {
            Some(self.top.clone())
        } else {
            None
        }
    }

    /// The wider of the two halves, in bits.
    ///
    /// This is what the certificate bit cap is measured against, so a value
    /// counts as large when either half is.
    #[must_use]
    pub fn bits(&self) -> u64 {
        self.top.size().bits().max(self.bottom.bits())
    }

    /// The additive inverse.
    #[must_use]
    pub fn negated(&self) -> Self {
        Self {
            top: self.top.negated(),
            bottom: self.bottom.clone(),
        }
    }

    /// The absolute value.
    #[must_use]
    pub fn magnitude(&self) -> Self {
        Self {
            top: self.top.magnitude(),
            bottom: self.bottom.clone(),
        }
    }

    /// The multiplicative inverse, or `None` for zero.
    #[must_use]
    pub fn inverted(&self) -> Option<Self> {
        if self.is_zero() {
            return None;
        }
        Some(Self {
            top: Int::signed(self.top.is_minus(), self.bottom.clone()),
            bottom: self.top.size().clone(),
        })
    }

    /// Sum.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        let top = self
            .top
            .times(&Int::positive(other.bottom.clone()))
            .plus(&other.top.times(&Int::positive(self.bottom.clone())));
        let bottom = Int::positive(self.bottom.times(&other.bottom));
        Self::ratio(&top, &bottom).expect("a positive denominator is never zero")
    }

    /// Difference.
    #[must_use]
    pub fn minus(&self, other: &Self) -> Self {
        self.plus(&other.negated())
    }

    /// Product.
    #[must_use]
    pub fn times(&self, other: &Self) -> Self {
        let top = self.top.times(&other.top);
        let bottom = Int::positive(self.bottom.times(&other.bottom));
        Self::ratio(&top, &bottom).expect("a positive denominator is never zero")
    }

    /// Quotient, or `None` when the divisor is zero.
    #[must_use]
    pub fn divided(&self, other: &Self) -> Option<Self> {
        Some(self.times(&other.inverted()?))
    }

    /// Comparison by value.
    #[must_use]
    pub fn compare(&self, other: &Self) -> Ordering {
        let left = self.top.times(&Int::positive(other.bottom.clone()));
        let right = other.top.times(&Int::positive(self.bottom.clone()));
        left.compare(&right)
    }

    /// The greatest integer no greater than the value.
    #[must_use]
    pub fn floor(&self) -> Int {
        self.top
            .divide_floor(&Int::positive(self.bottom.clone()))
            .expect("a positive denominator is never zero")
            .0
    }

    /// The least integer no less than the value.
    #[must_use]
    pub fn ceiling(&self) -> Int {
        self.negated().floor().negated()
    }

    /// The fractional part, in the half-open interval from zero to one.
    #[must_use]
    pub fn fractional(&self) -> Self {
        self.minus(&Self::whole(self.floor()))
    }

    /// Whether the value is inside the numeric acceptance fence.
    ///
    /// Zero is exempt, being exactly representable. Every other value must
    /// round to a binary64 that is finite and nonzero. The two boundaries are
    /// compared exactly rather than reached by rounding: a magnitude at or
    /// below two to the minus one thousand and seventy-fifth rounds to zero,
    /// since that point is the halfway mark below the smallest subnormal and
    /// the tie goes to the even neighbour, which is zero; a magnitude at or
    /// above two to the one thousand and twenty-fourth minus two to the nine
    /// hundred and seventieth rounds to an infinity, by the same tie rule at
    /// the other end.
    #[must_use]
    pub fn inside_binary64(&self) -> bool {
        if self.is_zero() {
            return true;
        }
        let top = self.top.size();
        // Underflow: top * 2^1075 <= bottom.
        if top.shift_up(1075).compare(&self.bottom) != Ordering::Greater {
            return false;
        }
        // Overflow: top >= bottom * (2^54 - 1) * 2^970.
        let scale = Nat::one()
            .shift_up(54)
            .minus(&Nat::one())
            .expect("two to the power exceeds one");
        let boundary = self.bottom.times(&scale).shift_up(970);
        if top.compare(&boundary) != Ordering::Less {
            return false;
        }
        true
    }
}

impl fmt::Display for Frac {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_whole() { formatter.write_str(&self.top().to_decimal()) } else { write!(formatter, "{}/{}", self.top().to_decimal(), self.bottom().to_decimal()) }
    }
}

impl fmt::Debug for Frac {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Frac({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ratio(top: i64, bottom: i64) -> Frac {
        Frac::ratio(&Int::from_i64(top), &Int::from_i64(bottom)).expect("nonzero denominator")
    }

    #[test]
    fn construction_reduces_and_puts_the_sign_on_top() {
        assert_eq!(ratio(6, 4), ratio(3, 2));
        assert_eq!(ratio(6, -4), ratio(-6, 4));
        assert!(ratio(6, -4).is_minus());
        assert_eq!(ratio(12, 4), Frac::from_i64(3));
        assert!(ratio(12, 4).is_whole());
        assert_eq!(Frac::ratio(&Int::one(), &Int::zero()), None);
    }

    #[test]
    fn zero_normalizes_its_denominator() {
        let zero = ratio(0, 5);
        assert!(zero.is_zero());
        assert!(zero.is_whole());
        assert!(!ratio(0, -5).is_minus());
        assert_eq!(zero.bottom().to_decimal(), "1");
    }

    #[test]
    fn arithmetic_reduces_where_it_should() {
        assert_eq!(ratio(1, 3).plus(&ratio(1, 6)), ratio(1, 2));
        assert!(ratio(1, 3).plus(&ratio(-1, 3)).is_zero());
        assert_eq!(ratio(1, 2).minus(&ratio(1, 3)), ratio(1, 6));
        assert_eq!(ratio(2, 3).times(&ratio(3, 2)), Frac::one());
        assert_eq!(ratio(-2, 3).times(&ratio(3, 4)), ratio(-1, 2));
        assert_eq!(
            ratio(1, 2).divided(&ratio(1, 3)).expect("nonzero"),
            ratio(3, 2)
        );
        assert_eq!(Frac::one().divided(&Frac::zero()), None);
        assert_eq!(Frac::zero().inverted(), None);
    }

    #[test]
    fn comparison_is_exact_where_a_float_would_collapse() {
        assert_eq!(ratio(355, 113).compare(&ratio(22, 7)), Ordering::Less);
        assert_eq!(ratio(22, 7).compare(&ratio(355, 113)), Ordering::Greater);
        assert_eq!(ratio(1, 3).compare(&ratio(2, 6)), Ordering::Equal);
        assert_eq!(
            ratio(9_007_199_254_740_993, 9_007_199_254_740_992).compare(&Frac::one()),
            Ordering::Greater
        );
    }

    #[test]
    fn the_floor_is_the_floor_on_both_sides_of_zero() {
        assert_eq!(ratio(7, 2).floor().to_decimal(), "3");
        assert_eq!(ratio(-7, 2).floor().to_decimal(), "-4");
        assert_eq!(ratio(7, 2).ceiling().to_decimal(), "4");
        assert_eq!(ratio(-7, 2).ceiling().to_decimal(), "-3");
        assert_eq!(ratio(4, 1).floor().to_decimal(), "4");
        assert_eq!(ratio(4, 1).ceiling().to_decimal(), "4");
    }

    #[test]
    fn the_fractional_part_is_never_negative() {
        assert_eq!(ratio(7, 2).fractional(), ratio(1, 2));
        assert_eq!(ratio(-7, 2).fractional(), ratio(1, 2));
        assert!(ratio(4, 1).fractional().is_zero());
        for value in [ratio(7, 2), ratio(-7, 2), ratio(-1, 3)] {
            let part = value.fractional();
            assert!(!part.is_minus());
            assert_eq!(part.compare(&Frac::one()), Ordering::Less);
            assert_eq!(Frac::whole(value.floor()).plus(&part), value);
        }
    }

    #[test]
    fn the_acceptance_fence_sits_exactly_on_the_rounding_boundaries() {
        assert!(Frac::zero().inside_binary64());
        assert!(Frac::one().inside_binary64());
        assert!(ratio(1, 10).inside_binary64());

        // The smallest subnormal is inside; half of it is the tie against zero
        // and is outside; anything above that tie is inside again.
        let smallest = Frac::ratio(&Int::one(), &Int::positive(Nat::one().shift_up(1074)))
            .expect("a power of two is never zero");
        assert!(smallest.inside_binary64());
        let tie = smallest.times(&ratio(1, 2));
        assert!(!tie.inside_binary64());
        assert!(tie.times(&ratio(3, 2)).inside_binary64());
        assert!(tie.negated().inside_binary64() == tie.inside_binary64());

        // The largest finite value is inside; the tie above it is not.
        let largest = Frac::whole(Int::positive(
            Nat::one()
                .shift_up(53)
                .minus(&Nat::one())
                .expect("above one")
                .shift_up(971),
        ));
        assert!(largest.inside_binary64());
        let over = Frac::whole(Int::positive(
            Nat::one()
                .shift_up(54)
                .minus(&Nat::one())
                .expect("above one")
                .shift_up(970),
        ));
        assert!(!over.inside_binary64());
        assert!(!over.negated().inside_binary64());
        // Just below that tie is still finite.
        let under = over.minus(&Frac::one());
        assert!(under.inside_binary64());
    }

    #[test]
    fn the_bit_width_is_the_wider_half() {
        let wide = Frac::ratio(&Int::positive(Nat::one().shift_up(200)), &Int::from_i64(3))
            .expect("nonzero denominator");
        assert_eq!(wide.bits(), 201);
        assert_eq!(Frac::zero().bits(), 1);
    }
}
