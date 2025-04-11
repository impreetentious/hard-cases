//! The LP-format reader.
//!
//! LP format is a readable way to write a model down, and readability is
//! exactly where it becomes ambiguous. A statement has no terminator, so a
//! reader has to decide where a wrapped constraint ends; an omitted bound is
//! defaulted; a term can be written four ways. Each of those is a place where
//! two readers build different models from one file and neither reports
//! anything.
//!
//! The accepted subset is chosen so that no such place remains, and the rule
//! that buys most of it is the label. Every objective and every constraint
//! carries one, and a line that does not begin with a label continues the
//! statement before it. That makes the continuation rule total, where the usual
//! heuristics — a leading sign, or a line containing a relational operator —
//! split a wrapped row differently from each other and do it silently.

pub mod reader;
pub mod token;

pub use reader::{Options, Parsed, read, read_with};
pub use token::{Relation, Token};
