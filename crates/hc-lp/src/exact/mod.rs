//! The exact side of the linear-programming core.
//!
//! The search runs in binary64 and returns a basis: a guess about which
//! constraints are tight. This module turns such a guess back into exact
//! quantities over the model's own rationals — the vertex the basis names, the
//! row multipliers it implies — so that everything downstream reasons about
//! the model rather than about a rounding of it.
//!
//! Two things live here rather than higher up, and the reason is that two
//! different callers need them. The certificate assembler rebuilds a vertex to
//! write it into a proof; the branch-and-bound search rebuilds one to decide
//! whether a candidate incumbent is genuinely feasible before it is allowed to
//! prune anything. Those callers sit on opposite sides of the crate graph, and
//! duplicating a rational Gaussian elimination between them would be
//! duplicating the one thing in the project that most needs to be written once.
//!
//! Nothing here is fast and nothing here needs to be. It runs once per claim
//! and once per candidate incumbent, not once per pivot, and it is the code
//! whose being right matters most.

pub mod canonical;
pub mod rebuild;
pub mod system;

pub use canonical::Canonical;
pub use rebuild::{Gap, Wanted, exact_multipliers, exact_point, resting_value};
pub use system::{Failure, System};
