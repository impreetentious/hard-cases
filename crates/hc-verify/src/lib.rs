#![forbid(unsafe_code)]

//! The verifier.
//!
//! This crate rechecks a certificate against the original problem file, and it
//! is written to share nothing with the solver that produced either. It has no
//! dependencies at all — not on the workspace, not on the ecosystem — and it
//! carries its own arbitrary-precision arithmetic, its own hashes, its own
//! parsers, and its own model.
//!
//! That duplication is the project. A solver and a checker written against the
//! same library agree wherever the library is wrong, and a proof rechecked by
//! its own arithmetic has not been rechecked. Where the two implementations
//! must be compared, they are compared against the inert vectors under
//! `spec/`, which neither of them generated and neither of them may edit to
//! make a disagreement go away.
//!
//! Independence is not only an absence of imports. The arithmetic here is
//! deliberately built differently from the solver's: a different limb width, a
//! different multiplication, a different division, a different greatest common
//! divisor. Two implementations that make the same choices tend to make the
//! same mistakes, and the point of the second one is that its mistakes are
//! somewhere else.
//!
//! Simplicity outranks speed here, everywhere, without argument. This code is
//! the trusted base; its size and its plainness are properties a reader has to
//! be able to check by reading it. The solver may be clever.

pub mod vaxiom;
pub mod vcli;
pub mod vclose;
pub mod vcut;
pub mod vhash;
pub mod vhcc;
pub mod vint;
pub mod vmodel;
pub mod vparse;
pub mod vpresolve;
pub mod vrat;
pub mod vreject;
pub mod vrules;
pub mod vspec;
pub mod vtext;
pub mod vtree;
pub mod vwork;

pub use vaxiom::{Axioms, Inequality, Sum};
pub use vcli::{Ask, Request, accepted, headline, refused, request, version};
pub use vclose::{Verdict, close, model_hash, verify};
pub use vhash::{Checksum, Digest, checksum, digest, instance_digest, to_hex};
pub use vhcc::{Claim, Proof, Step, from_annotated_hex};
pub use vint::{Int, IntError, Nat, NatError};
pub use vmodel::{Axiom, Cell, Column, Constraint, Direction, Face, Instance, Tag};
pub use vparse::Layout;
pub use vpresolve::Reduction;
pub use vrat::Frac;
pub use vreject::{Code, Rejected};
pub use vrules::{
    Bound, bound_is, contradiction, dual_bound, everything, objective_is, point_is_feasible,
    relative_gap,
};
pub use vspec::{Asset, AssetError};
pub use vtext::NumberError;
pub use vtree::{Replay, replay};
pub use vwork::Work;
