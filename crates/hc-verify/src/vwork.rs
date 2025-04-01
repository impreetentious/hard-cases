//! Counting the verifier's work, in a currency that is not seconds.
//!
//! A caps regime that measured verification in wall time would be measuring
//! the machine, and a caps regime that measured it as a fraction of the solve
//! would invite the reading that a longer search earns a longer proof — which
//! is exactly backwards. So the verifier counts what it actually does: one
//! unit per byte it reads, and one unit per limb-word step its arithmetic
//! performs.
//!
//! The count is an upper bound, deterministic, and computed from the sizes of
//! the operands rather than accumulated inside the loops. That choice is
//! recorded as a decision and not as an accident: a counter incremented inside
//! `Nat`'s inner loops would either be a global mutable cell — ambient state
//! in the one crate that should have none — or a parameter threaded through
//! every arithmetic signature in the trusted base, making the code that most
//! needs to be readable the least. The cost of the choice is that a value like
//! a greatest common divisor, whose real step count depends on its inputs, is
//! charged its worst case rather than its actual one.
//!
//! Charging the worst case is the safe direction. A cap that stops a
//! verification too early can only refuse work; it can never accept a proof
//! that should have been refused, because the count gates nothing about
//! mathematics. Every reported figure is therefore a ceiling on the real cost,
//! and the report says so.
//!
//! Every arithmetic operation a proof rule performs goes through this type.
//! That is what stops a new rule from forgetting to pay: a rule has a meter or
//! it has no arithmetic.

use core::cmp::Ordering;

use crate::vrat::Frac;

/// The width of one limb, in bits. Matches the arithmetic this crate carries.
const LIMB_BITS: u64 = 32;

/// The number of limb words a value of a given bit length occupies.
///
/// Zero occupies one word for costing purposes rather than none, so that an
/// operation on zero is never free. Free operations make a counter easy to
/// game and hard to reason about.
const fn words(bits: u64) -> u64 {
    bits / LIMB_BITS + 1
}

/// The limb-word cost of a rational, as the numerator and denominator
/// together.
fn size(value: &Frac) -> u64 {
    words(value.top().size().bits()) + words(value.bottom().bits())
}

/// The bit width of a rational, as the wider of its two halves.
fn width(value: &Frac) -> u64 {
    value.top().size().bits().max(value.bottom().bits())
}

/// A running count of verifier work units.
///
/// Saturating throughout. A count that wrapped would report a small number for
/// an enormous verification, which is the one failure a work meter must not
/// have.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Work {
    units: u64,
    peak_bits: u64,
}

impl Work {
    /// A fresh meter reading zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            units: 0,
            peak_bits: 0,
        }
    }

    /// The units consumed so far.
    #[must_use]
    pub const fn units(&self) -> u64 {
        self.units
    }

    /// The widest rational the check has handled, in bits.
    ///
    /// Published alongside the work count because the two answer different
    /// questions. The count says how much checking cost; this says how large
    /// the numbers in the proof got, which is the figure that decides whether
    /// a certificate fits inside the published bit cap at all.
    #[must_use]
    pub const fn peak_bits(&self) -> u64 {
        self.peak_bits
    }

    /// Whether the count has reached a limit.
    #[must_use]
    pub const fn beyond(&self, limit: u64) -> bool {
        self.units > limit
    }

    /// Charges for bytes read.
    pub const fn read(&mut self, bytes: u64) {
        self.units = self.units.saturating_add(bytes);
    }

    /// Charges a raw number of limb-word steps.
    ///
    /// Used by the readers, which know how much text they consumed but do no
    /// arithmetic of their own worth naming.
    pub const fn spend(&mut self, steps: u64) {
        self.units = self.units.saturating_add(steps);
    }

    /// Notes how wide a value is, without charging for it.
    ///
    /// Every operation passes its operands and its result through here, so the
    /// published peak is over everything the check actually touched rather
    /// than over whatever happened to be measured.
    fn note(&mut self, value: &Frac) {
        self.peak_bits = self.peak_bits.max(width(value));
    }

    /// Adds two rationals, charging for the cross-multiplication, the sum, and
    /// the reduction that follows.
    pub fn add(&mut self, left: &Frac, right: &Frac) -> Frac {
        self.charge_combine(left, right);
        let total = left.plus(right);
        self.note(&total);
        total
    }

    /// Subtracts, at the cost of an addition.
    pub fn subtract(&mut self, left: &Frac, right: &Frac) -> Frac {
        self.charge_combine(left, right);
        let difference = left.minus(right);
        self.note(&difference);
        difference
    }

    /// Multiplies, charging for two schoolbook products and the reduction.
    pub fn multiply(&mut self, left: &Frac, right: &Frac) -> Frac {
        let (a, b) = (size(left), size(right));
        self.spend(a.saturating_mul(b).saturating_mul(2));
        self.charge_reduction(a, b);
        let product = left.times(right);
        self.note(&product);
        product
    }

    /// Divides, at the cost of a multiplication.
    ///
    /// Returns `None` when the divisor is zero, exactly as the arithmetic
    /// does; the work is charged either way, because it was done either way.
    pub fn divide(&mut self, left: &Frac, right: &Frac) -> Option<Frac> {
        let (a, b) = (size(left), size(right));
        self.spend(a.saturating_mul(b).saturating_mul(2));
        self.charge_reduction(a, b);
        let quotient = left.divided(right);
        if let Some(value) = &quotient {
            self.note(value);
        }
        quotient
    }

    /// Compares two rationals, charging for the cross-multiplication.
    pub fn compare(&mut self, left: &Frac, right: &Frac) -> Ordering {
        let (a, b) = (size(left), size(right));
        self.spend(a.saturating_mul(b));
        self.note(left);
        self.note(right);
        left.compare(right)
    }

    /// Charges for a comparison against a value already in hand, where no
    /// multiplication is needed.
    pub fn inspect(&mut self, value: &Frac) {
        self.spend(size(value));
        self.note(value);
    }

    /// The cost of bringing two rationals to a common denominator and adding.
    fn charge_combine(&mut self, left: &Frac, right: &Frac) {
        let (a, b) = (size(left), size(right));
        // Two cross products, then an addition over the wider of the results.
        self.spend(a.saturating_mul(b).saturating_mul(2));
        self.spend(a.saturating_add(b));
        self.charge_reduction(a, b);
    }

    /// The cost of reducing a result to lowest terms.
    ///
    /// Euclid's algorithm on operands of `n` words takes a number of steps
    /// bounded by a small multiple of `n` divisions, and each division of an
    /// `n`-word value by an `m`-word one costs about `n * m`. The bound below
    /// is that product taken at the widest the result can be. It is generous,
    /// which is the direction a bound should err in.
    fn charge_reduction(&mut self, a: u64, b: u64) {
        let widest = a.saturating_add(b);
        self.spend(widest.saturating_mul(widest));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vint::Int;

    fn ratio(top: i64, bottom: i64) -> Frac {
        Frac::ratio(&Int::from_i64(top), &Int::from_i64(bottom)).expect("a nonzero denominator")
    }

    #[test]
    fn a_fresh_meter_reads_zero_and_every_operation_costs_something() {
        let mut work = Work::new();
        assert_eq!(work.units(), 0);

        let one = work.add(&Frac::one(), &Frac::zero());
        assert_eq!(one.to_canonical(), "1");
        assert!(work.units() > 0, "an addition must cost something");

        let before = work.units();
        work.inspect(&Frac::one());
        assert!(work.units() > before, "an inspection must cost something");
    }

    #[test]
    fn the_arithmetic_the_meter_performs_is_the_arithmetic_it_wraps() {
        let mut work = Work::new();
        let third = ratio(1, 3);
        let half = ratio(1, 2);
        assert_eq!(work.add(&third, &half).to_canonical(), "5/6");
        assert_eq!(work.subtract(&third, &half).to_canonical(), "-1/6");
        assert_eq!(work.multiply(&third, &half).to_canonical(), "1/6");
        assert_eq!(
            work.divide(&third, &half).map(|value| value.to_canonical()),
            Some("2/3".to_owned())
        );
        assert_eq!(work.divide(&third, &Frac::zero()), None);
        assert_eq!(work.compare(&third, &half), Ordering::Less);
    }

    #[test]
    fn wider_operands_cost_more_and_the_ordering_is_strict() {
        let narrow = {
            let mut work = Work::new();
            work.multiply(&Frac::from_i64(3), &Frac::from_i64(5));
            work.units()
        };
        let wide = {
            let mut work = Work::new();
            let big = Frac::whole(
                Int::from_decimal(&"9".repeat(60)).expect("a long decimal is an integer"),
            );
            work.multiply(&big, &big);
            work.units()
        };
        assert!(
            wide > narrow,
            "a wider product must cost more: {wide} against {narrow}"
        );
    }

    #[test]
    fn the_peak_width_is_over_everything_the_check_touched() {
        let mut work = Work::new();
        assert_eq!(work.peak_bits(), 0);

        work.inspect(&Frac::one());
        let narrow = work.peak_bits();
        assert!(narrow > 0);

        let big =
            Frac::whole(Int::from_decimal(&"9".repeat(100)).expect("a long decimal is an integer"));
        work.multiply(&big, &big);
        assert!(
            work.peak_bits() > narrow * 10,
            "a hundred-digit product should dominate the peak"
        );
    }

    #[test]
    fn the_count_saturates_rather_than_wrapping() {
        let mut work = Work::new();
        work.read(u64::MAX);
        work.read(1000);
        assert_eq!(work.units(), u64::MAX);
        assert!(work.beyond(u64::MAX - 1));
    }

    #[test]
    fn bytes_read_are_charged_one_for_one() {
        let mut work = Work::new();
        work.read(150);
        assert_eq!(work.units(), 150);
    }

    #[test]
    fn nothing_is_free_including_operations_on_zero() {
        let mut work = Work::new();
        work.multiply(&Frac::zero(), &Frac::zero());
        assert!(work.units() > 0);
    }
}
