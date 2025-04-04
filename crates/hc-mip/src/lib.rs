#![forbid(unsafe_code)]

//! Branch and bound, arranged so the search can be written down.
//!
//! A branch-and-bound search is a proof that happens to be executed. Every node
//! either establishes that its region cannot contain anything better than what
//! is already known, or splits into two regions that together cover it. Run
//! that to completion and the tree is a proof of optimality; stop early and the
//! open nodes are exactly what is left unproved.
//!
//! Almost everything in this crate follows from taking that seriously.
//!
//! The tree is built parent before child, because that is the order a proof has
//! to be written in and rebuilding the order afterwards would mean the search
//! and the proof disagree about what happened. A node's bound is an exact
//! rational recomputed from its basis, not the floating-point number the
//! simplex reported, because a bound is what prunes and a prune is
//! irreversible. A candidate incumbent is screened exactly against the original
//! model before it is accepted, because the incumbent is what every bound is
//! compared against — a point that is feasible only within a tolerance would
//! quietly discard regions containing the answer.
//!
//! What the search declines to establish, it says. A node whose exact rebuild
//! fails is left open and named, and a tree with an open node proves a gap
//! rather than optimality. There is no status meaning "probably fine".

pub mod incumbent;
pub mod search;
pub mod tree;

pub use incumbent::{Incumbent, screen};
pub use search::{Ending, Node, Options, Search, run};
pub use tree::{Assumption, Tree};
