#![forbid(unsafe_code)]

//! Presolve, written down.
//!
//! Presolve is where solvers usually stop being checkable. A chain of
//! reductions described as standard preprocessing produces a smaller model, and
//! every claim proved about that smaller model rests on a step nobody replayed.
//!
//! Here a reduction is a typed record with an applicability condition, an
//! action, and a pair of maps. A verifier holding the model before a record
//! recomputes the model after it and compares the hash; it never reads the
//! resulting coefficients as fact. The two maps are what make the step true
//! rather than merely reproducible, and they are stated in `spec/v1` rather
//! than in this code.
//!
//! Eight transforms, and that is the list. Dominated columns, coefficient
//! strengthening, probing, symmetry, and proportional aggregation are all
//! genuinely useful and none of them has a witness that survives being written
//! down as an exact recomputable equivalence, so they are excluded rather than
//! included with a hand-wave.

pub mod chain;
pub mod draft;
pub mod map;
pub mod rule;

pub use chain::{Chain, Contradiction, Step, reduce};
pub use draft::Draft;
pub use rule::{Inapplicable, Rule, apply, encode};
