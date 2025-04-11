//! The JSON instance reader.
//!
//! This is the programmatic surface: the shape a caller builds a model in when
//! it is not writing a file a human will read. It is the strictest of the three
//! accepted formats, because it is the one with no history to be compatible
//! with, and because the thing JSON is most convenient for is exactly the thing
//! this project forbids.
//!
//! That thing is the JSON number. A JSON number is a decimal literal that
//! almost every reader hands back as a binary64, and a model value that has
//! passed through a binary64 is no longer the value the document wrote. So no
//! model value here is a number: coefficients, bounds, right-hand sides,
//! ranges, and the objective constant are strings, read by the same exact
//! decimal grammar the other two formats use. The accepted grammar has no
//! floating-point literal in it at all, which makes the rule structural rather
//! than a convention a reader has to remember.

pub mod reader;
pub mod value;

pub use reader::{Options, read, read_with};
pub use value::{Json, parse};
