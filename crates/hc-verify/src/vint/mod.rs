//! The verifier's integers.
//!
//! [`Nat`] is a nonnegative magnitude; [`Int`] is a magnitude with a sign.
//! Both are built here rather than taken from anywhere, and both are built
//! differently from their counterparts on the solver side — a different limb
//! width, a different multiplication, a different division, a different
//! greatest common divisor. The reason is in the crate documentation and it is
//! the whole reason this crate exists.

pub mod int;
pub mod nat;

pub use int::{Int, IntError};
pub use nat::{Nat, NatError};
