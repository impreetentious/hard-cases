#![forbid(unsafe_code)]

//! The vocabulary every solver-side crate shares: the instance model, sparse
//! matrix storage, stable identifiers, the terminal status and error
//! taxonomies, the deterministic generator, and the named tolerances.
//!
//! Nothing above this crate may write a bare `1e-*` literal, iterate an
//! unordered container, read a clock, or invent a status. Those four rules are
//! why this crate exists, and the forbidden-API scan enforces them at the
//! workspace boundary.
//!
//! The verifier depends on none of this. It carries its own arithmetic, its
//! own parsers, and its own model, and the isolation check fails the build if
//! an edge ever appears between the two. Where the two implementations are
//! compared, they are compared against the same inert vectors under `spec/` —
//! neither is generated from the other, and neither is the other's oracle.

pub mod claim;
pub mod dump;
pub mod hash;
pub mod ids;
pub mod model;
pub mod reject;
pub mod rng;
pub mod spec;
pub mod tol;
pub mod trace;

pub use claim::{Claim, NoProgressReason};
pub use hash::{Crc32c, Sha256, crc32c, hex, instance_digest, sha256};
pub use ids::{FormatTag, IneqId, RowId, Sense, Side, VarId};
pub use model::{IneqSource, Inequality, Matrix, Model, ModelParts, Row, RowMajor, Variable};
pub use reject::{Refusal, Rejection};
pub use rng::{SplitMix64, Xoshiro256PlusPlus};
pub use spec::{SpecDocument, SpecError, SpecReason, SpecTable, SpecValue};
pub use tol::Tol;
