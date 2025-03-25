#![forbid(unsafe_code)]

//! The solver's exact arithmetic: arbitrary-precision unsigned and signed
//! integers, and the rationals every claim is ultimately made of.
//!
//! There is a second, unrelated implementation of the same arithmetic inside
//! the verifier. That duplication is the point of the project rather than an
//! accident of it. A solver and a verifier that share a big-integer library
//! share its bugs, and a proof checked by the code that produced it is not
//! checked at all. The two stacks are written separately, may not call one
//! another, and are held against the same inert vectors under
//! `spec/v1/vectors/arith/` — vectors neither of them generated.
//!
//! Everything here is exact. Nothing in this crate rounds, and no operation
//! has a tolerance. The floating-point search decides where to look; this
//! decides what is true.

pub mod dec;
pub mod div;
pub mod float;
pub mod gcd;
pub mod int;
pub mod mul;
pub mod rational;
pub mod text;
pub mod uint;

pub use dec::ParseUintError;
pub use int::{BigInt, ParseIntError};
pub use mul::KARATSUBA_LIMBS;
pub use rational::{Rational, RationalError};
pub use text::TextError;
pub use uint::BigUint;
