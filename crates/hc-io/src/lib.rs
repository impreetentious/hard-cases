#![forbid(unsafe_code)]

//! Reading instance files into the canonical exact model.
//!
//! This crate takes bytes and returns a model, and it never opens anything.
//! The caller has already decided which bytes are being read and under which
//! format's rules, because the format tag goes into the instance digest and a
//! digest that depended on a heuristic would be a digest nobody could
//! reproduce. Sniffing the variant from the content would make the identity of
//! a model depend on a guess.
//!
//! Strictness is the default and is not an inconvenience to be worked around.
//! A file that is not unambiguously inside the accepted subset is refused with
//! an identifier naming the rule that refused it, never repaired by inference.
//! Forward compatibility by ignoring what is not understood is a reasonable
//! policy for a configuration file and a disastrous one for a model somebody
//! is about to prove a theorem about.
//!
//! The verifier has its own reader for the same formats, written separately
//! and sharing nothing with this one. Where the two disagree about a file,
//! that disagreement is the finding.

pub mod mps;
