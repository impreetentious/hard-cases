#![forbid(unsafe_code)]

//! Turning a search result into something that can be checked.
//!
//! The simplex returns a basis. A basis is not a claim: it is a guess about
//! which constraints are tight, arrived at in floating point, and the numbers
//! attached to it are approximations of quantities nobody has established. The
//! work in this crate is to take that guess and rebuild everything it implies
//! in exact arithmetic, over the original model, so that what comes out is
//! either a proof or an admission that there is not one.
//!
//! The rebuild can fail, and failing is a normal outcome rather than an error.
//! A basis the float search liked may be singular exactly; multipliers that
//! looked optimal may price a column the wrong way; a bound that seemed finite
//! may rest on a variable with no bound at all. Every one of those is a reason
//! to make a weaker true statement, and none of them is a reason to make the
//! stronger one anyway.
//!
//! Nothing here is trusted downstream either. What this crate assembles is
//! checked again by a verifier that shares none of it — different arithmetic,
//! different parser, different model, reading the original bytes. This side
//! exists to produce something worth checking, not to be believed.

pub mod assemble;
pub mod hcc;
pub mod lp;
pub mod read;
