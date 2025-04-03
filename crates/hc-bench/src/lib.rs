#![forbid(unsafe_code)]

//! The harness: reference implementations, campaigns, and the scoreboard.
//!
//! This crate is outside the deterministic perimeter on purpose. It reads
//! clocks, touches the filesystem, and reports in floating point, because a
//! benchmark harness has to. Nothing it computes may reach a certificate.
//!
//! Its most important contents are not the benchmarks. They are the reference
//! implementations: small, slow, obviously correct programs that exist to
//! disagree with the fast ones. An oracle that shares code with the thing it
//! is checking is not an oracle, so the reference solver here links none of
//! the production linear-algebra or search code, and a gate in continuous
//! integration fails the build if it ever does.

pub mod campaign;
pub mod corpus;
pub mod proof;
pub mod receipt;
pub mod ref_lp;
pub mod tamper;
