#![forbid(unsafe_code)]

//! The linear-programming core: sparse kernels, factorization, and the revised
//! simplex.
//!
//! Everything here runs in floating point, and nothing it computes is ever a
//! conclusion. A basis is a place to look, a reduced cost is a reason to look
//! elsewhere, and a bound is true only when an exact inference says so. The
//! crate is organized around keeping that distinction visible: the search
//! returns a basis and a status, and the exact machinery above it decides what,
//! if anything, may be claimed.
//!
//! No bare threshold appears anywhere below. Every fence is a named constant
//! carried through the call that uses it, so a reader can see which policy a
//! comparison is applying without going to look it up.

pub mod basis;
pub mod inverse;
pub mod lu;
pub mod pricing;
pub mod problem;
pub mod ratio;
pub mod scale;
pub mod simplex;
pub mod sparse;
pub mod working;

pub use basis::{Basis, Status};
pub use inverse::{Inverse, Reason, Unstable};
pub use lu::{Factorization, Singular};
pub use pricing::{Entering, Pricing, Rule};
pub use problem::Problem;
pub use ratio::Step;
pub use scale::Scaling;
pub use simplex::{Effort, Options, Outcome, solve, solve_from};
pub use sparse::{SparseVec, Workspace};
pub use working::{Kind, Working};

pub mod exact;
