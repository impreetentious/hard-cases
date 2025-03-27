//! The MPS reader.
//!
//! MPS is not one format. It is a family of mutually incompatible readings of
//! the same punched-card layout, and most of the disagreements are silent: the
//! file parses either way and the answer is different. Every fork is decided
//! in the normative assets, in writing, with the losing reading named — so
//! that a future reader knows it was considered rather than missed.

pub mod numeric;
pub mod reader;
pub mod record;
pub mod section;

pub use numeric::Limits;
pub use reader::{Options, Parsed, read, read_with};
pub use record::{Fields, Form, Line, Record, Shape};
pub use section::{Section, Sequence};
