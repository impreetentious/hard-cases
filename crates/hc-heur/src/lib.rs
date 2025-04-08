#![forbid(unsafe_code)]

//! Primal heuristics: guessing well, and being honest about it.
//!
//! A heuristic is a guess at a good solution. Nothing here establishes
//! anything: every candidate leaves as a rational vector and is accepted only
//! if the model, consulted exactly, says it is feasible. That division is the
//! whole design. The guessing may be as rough as it likes, and the acceptance
//! has no tolerance in it anywhere.
//!
//! Three of them, and that is the list. Rounding takes a relaxation's point and
//! moves each integer variable to the nearest integer. Diving fixes one
//! variable at a time and re-solves, following the relaxation down. The
//! feasibility pump alternates between the relaxation and the nearest integer
//! point until the two meet or it gives up.
//!
//! Every one of them reports what it did, including when it failed. A
//! heuristic that only announced its successes would make a search look
//! luckier than it was, and the theater would show a solver that never misses.

pub mod dive;
pub mod pump;
pub mod round;

pub use dive::dive;
pub use pump::pump;
pub use round::round;

/// What a heuristic did.
///
/// Reported whether or not it worked. The misses are the interesting half:
/// they are what a trace shows, what a scoreboard counts, and what tells
/// someone reading a log that the search was working rather than lucky.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A candidate that passed exact screening.
    Found {
        /// The point, over the working shape's columns.
        point: Vec<hc_exact::Rational>,
        /// Its exact objective.
        objective: hc_exact::Rational,
    },
    /// A candidate was produced and the model refused it.
    Refused {
        /// Why.
        reason: String,
    },
    /// No candidate was produced at all.
    Abandoned {
        /// Why.
        reason: String,
    },
}

impl Outcome {
    /// Whether the heuristic found something the model accepted.
    #[must_use]
    pub const fn is_found(&self) -> bool {
        matches!(self, Self::Found { .. })
    }

    /// The word this outcome travels as in a trace.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::Found { .. } => "found",
            Self::Refused { .. } => "refused",
            Self::Abandoned { .. } => "abandoned",
        }
    }
}
