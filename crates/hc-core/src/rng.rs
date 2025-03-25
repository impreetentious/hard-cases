//! Deterministic generation.
//!
//! The solver draws random numbers in exactly two places: the bound
//! perturbation that breaks degeneracy, and the flips that break a cycling
//! feasibility pump. Neither draw may depend on anything the machine happens
//! to know. The stream is derived from the instance bytes and the user seed
//! and from nothing else, so the same file solved on two machines produces the
//! same tree, the same log, and the same certificate bytes.
//!
//! There is one stream. Not one per subsystem, not one reseeded at the root of
//! each subtree — one, consumed in event order. A per-subsystem generator
//! sounds tidier and quietly makes the search depend on the order in which
//! subsystems happen to ask, which is exactly the dependency this module
//! exists to remove.
//!
//! Both recurrences are written out rather than taken from a crate. They are
//! twenty lines each, the constants are published, and the frozen vectors in
//! the normative assets are what they are held against.

/// The odd increment SplitMix64 advances its state by.
const SPLITMIX_GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;
const SPLITMIX_MIX_A: u64 = 0xbf58_476d_1ce4_e5b9;
const SPLITMIX_MIX_B: u64 = 0x94d0_49bb_1331_11eb;

/// The seed expander.
///
/// It never produces a value the solver uses directly. Its only job is to turn
/// one word into the four the main generator needs, and it is used for that
/// because a generator initialized by copying its seed into every state word
/// has a visibly poor first few outputs.
#[derive(Clone, Copy, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A generator starting from a seed.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next output.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(SPLITMIX_GAMMA);
        let mut mixed = self.state;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(SPLITMIX_MIX_A);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(SPLITMIX_MIX_B);
        mixed ^ (mixed >> 31)
    }
}

/// The solver stream.
#[derive(Clone, Copy, Debug)]
pub struct Xoshiro256PlusPlus {
    state: [u64; 4],
}

impl Xoshiro256PlusPlus {
    /// The generator the seeding rule produces from a seed word.
    ///
    /// Four consecutive expander outputs become the state in word order.
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        let mut expander = SplitMix64::new(seed);
        let state = [
            expander.next_u64(),
            expander.next_u64(),
            expander.next_u64(),
            expander.next_u64(),
        ];
        // An all-zero state is a fixed point of the recurrence. It is
        // unreachable from the expander, so this asserts rather than defends:
        // reaching it would mean the expander is not the one specified.
        debug_assert!(state != [0; 4], "the expander produced an all-zero state");
        Self { state }
    }

    /// The generator for one instance and one user seed.
    ///
    /// The seed word is the little-endian read of the digest's first eight
    /// bytes, combined with the user seed by exclusive-or. Binding the stream
    /// to the instance digest means an identical seed on two different models
    /// does not produce correlated searches, and it puts the stream inside
    /// what the digest already covers.
    #[must_use]
    pub fn from_instance(instance_digest: &[u8; 32], user_seed: u64) -> Self {
        let mut head = [0u8; 8];
        head.copy_from_slice(&instance_digest[..8]);
        Self::from_seed(u64::from_le_bytes(head) ^ user_seed)
    }

    /// The generator with an explicit state, for reproducing a frozen vector.
    ///
    /// # Panics
    ///
    /// Panics on the all-zero state, which the recurrence cannot leave.
    #[must_use]
    pub fn from_state(state: [u64; 4]) -> Self {
        assert!(state != [0; 4], "the all-zero state is a fixed point");
        Self { state }
    }

    /// The current state, in word order.
    #[must_use]
    pub fn state(&self) -> [u64; 4] {
        self.state
    }

    /// The next output.
    ///
    /// The output is computed from the state before the update, and the update
    /// is applied in exactly the order the recurrence is written in.
    pub fn next_u64(&mut self) -> u64 {
        let result = self.state[0]
            .wrapping_add(self.state[3])
            .rotate_left(23)
            .wrapping_add(self.state[0]);

        let carried = self.state[1] << 17;
        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];
        self.state[2] ^= carried;
        self.state[3] = self.state[3].rotate_left(45);

        result
    }

    /// A uniform value strictly below `bound`.
    ///
    /// Rejection rather than a modulo, because a modulo of a full-width draw
    /// is biased toward the low values and the bias is invisible in every
    /// test that does not look for it.
    ///
    /// # Panics
    ///
    /// Panics when `bound` is zero.
    pub fn next_below(&mut self, bound: u64) -> u64 {
        assert!(bound > 0, "next_below needs a positive bound");
        // The number of draws in `0..=zone` is a multiple of `bound`.
        let remainder = ((u64::MAX % bound) + 1) % bound;
        let zone = u64::MAX - remainder;
        loop {
            let drawn = self.next_u64();
            if drawn <= zone {
                return drawn % bound;
            }
        }
    }

    /// A uniform value in the half-open unit interval.
    ///
    /// The top fifty-three bits of one draw, scaled by a power of two, so
    /// every representable value in the interval is reachable and the scaling
    /// is exact.
    pub fn next_unit_interval(&mut self) -> f64 {
        const SIGNIFICAND: u64 = 1 << 53;
        ((self.next_u64() >> 11) as f64) / (SIGNIFICAND as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_expander_reproduces_its_published_outputs() {
        let mut expander = SplitMix64::new(0);
        assert_eq!(expander.next_u64(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(expander.next_u64(), 0x6e78_9e6a_a1b9_65f4);
        let mut from_one = SplitMix64::new(1);
        assert_eq!(from_one.next_u64(), 0x910a_2dec_8902_5cc1);
    }

    #[test]
    fn seeding_fills_the_state_in_word_order() {
        let generator = Xoshiro256PlusPlus::from_seed(0);
        let mut expander = SplitMix64::new(0);
        let expected = [
            expander.next_u64(),
            expander.next_u64(),
            expander.next_u64(),
            expander.next_u64(),
        ];
        assert_eq!(generator.state(), expected);
    }

    #[test]
    fn the_same_seed_replays_the_same_stream() {
        let mut left = Xoshiro256PlusPlus::from_seed(0x0123_4567_89ab_cdef);
        let mut right = Xoshiro256PlusPlus::from_seed(0x0123_4567_89ab_cdef);
        for _ in 0..64 {
            assert_eq!(left.next_u64(), right.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge_immediately() {
        let mut left = Xoshiro256PlusPlus::from_seed(0);
        let mut right = Xoshiro256PlusPlus::from_seed(1);
        assert_ne!(left.next_u64(), right.next_u64());
    }

    #[test]
    fn a_bounded_draw_stays_inside_its_bound_and_reaches_both_ends() {
        let mut generator = Xoshiro256PlusPlus::from_seed(7);
        let mut low = false;
        let mut high = false;
        for _ in 0..4096 {
            let drawn = generator.next_below(10);
            assert!(drawn < 10);
            low |= drawn == 0;
            high |= drawn == 9;
        }
        assert!(low && high, "a bounded draw never reached an endpoint");
        let mut single = Xoshiro256PlusPlus::from_seed(7);
        assert_eq!(single.next_below(1), 0);
    }

    #[test]
    fn a_unit_draw_stays_in_the_half_open_interval() {
        let mut generator = Xoshiro256PlusPlus::from_seed(11);
        for _ in 0..4096 {
            let drawn = generator.next_unit_interval();
            assert!((0.0..1.0).contains(&drawn));
        }
    }

    #[test]
    fn an_instance_seed_reads_the_digest_little_endian() {
        let mut digest = [0u8; 32];
        digest[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let expected = Xoshiro256PlusPlus::from_seed(0x0807_0605_0403_0201);
        assert_eq!(
            Xoshiro256PlusPlus::from_instance(&digest, 0).state(),
            expected.state()
        );
        // The user seed enters by exclusive-or, so it is its own inverse.
        let combined = Xoshiro256PlusPlus::from_instance(&digest, 0x0807_0605_0403_0201);
        assert_eq!(combined.state(), Xoshiro256PlusPlus::from_seed(0).state());
    }

    #[test]
    #[should_panic(expected = "fixed point")]
    fn the_all_zero_state_is_refused() {
        let _ = Xoshiro256PlusPlus::from_state([0; 4]);
    }
}
