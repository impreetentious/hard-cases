#![forbid(unsafe_code)]

//! Cutting planes, and the discipline that makes them checkable.
//!
//! A cut is an inequality added to a relaxation on the promise that it removes
//! no integer-feasible point. When that promise is broken the solver does not
//! crash and does not slow down: it reports a smaller optimal value than the
//! truth, confidently, with a complete-looking tree behind it. That is the
//! worst failure this project has, and it is why nothing here trusts the
//! routine that found the cut.
//!
//! Separation may search however it likes. Validity is established by
//! recomputing the derivation exactly from the model, and the recomputation is
//! written so that it needs nothing from the search — no basis, no tableau, no
//! bound state. What a certificate carries is the combination that produced the
//! cut; everything else is derived from the model on both sides.
//!
//! Two families, and that is the list.

pub mod cover;
pub mod gmi;
pub mod pool;
pub mod separate;

pub use cover::{Knapsack, knapsack_from, lift};
pub use gmi::{Gmi, derive};
pub use pool::{Derivation, Entry, Family, Pool};
pub use separate::violated;
