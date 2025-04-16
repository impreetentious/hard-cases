#![forbid(unsafe_code)]

//! The fuzzer.
//!
//! Deterministic and in the tree, rather than a coverage-guided engine behind a
//! nightly toolchain. Three reasons, in order of weight.
//!
//! A finding here has to be reproducible from a number. Everything else in this
//! project is: the campaigns are seeded, the search is single-threaded, the
//! double-run gate compares bytes. A fuzzer whose corpus grows out of timing
//! and coverage feedback produces findings nobody can replay exactly, and a
//! case file that cannot be replayed is a story rather than a regression.
//!
//! The interesting inputs here are not random bytes. They are files that are
//! almost right — a statement continued one line too far, a bound whose sides
//! cross, a token one character over its cap — and reaching those by mutating
//! bytes is enormously slower than emitting them on purpose. The generators
//! below know the grammars, so the corpus starts inside the languages and the
//! mutations walk it just outside.
//!
//! And it costs no dependency. The verifier carries none by construction and
//! the workspace carries none at all; an engine that needed a nightly compiler
//! and a linker plugin to run the tests would be the largest thing in the
//! build by a distance.
//!
//! What is given up is real coverage feedback. This is stated in
//! `docs/LIMITATIONS.md` rather than papered over: the corpus reaches what its
//! generators were written to reach, and a region of a reader that nobody
//! thought to emit for is a region nobody is fuzzing.

pub mod edits;
pub mod parsers;
pub mod proofs;
pub mod shapes;
pub mod solver;

use core::fmt;

/// How much work a campaign may do.
///
/// Counted in cases rather than in seconds, like every other budget in this
/// project. A wall-clock budget makes a run mean something different on every
/// machine, and makes a nightly finding impossible to reproduce on a laptop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// The first seed of the run.
    pub from: u64,
    /// How many seeds to walk.
    pub cases: u64,
}

impl Budget {
    /// The budget a change is expected to pay before it lands.
    pub const PER_CHANGE: Self = Self {
        from: 1,
        cases: 200,
    };

    /// The budget the nightly run pays.
    pub const NIGHTLY: Self = Self {
        from: 1,
        cases: 20_000,
    };

    /// The seeds this budget covers.
    ///
    /// The range itself, unscrambled. A finding reports the seed it came from
    /// and a case file writes that number down, so the number has to be one
    /// that walks straight back into the campaign: a budget that mixed its
    /// seeds would report values nobody could hand back to it. The mixing that
    /// makes consecutive numbers behave like unrelated ones happens inside the
    /// generator, where the frozen seeding rule already does it.
    pub fn seeds(self) -> impl Iterator<Item = u64> {
        self.from..self.from.saturating_add(self.cases)
    }
}

/// Something a target found.
///
/// A finding carries the seed that produced it and the bytes it was looking
/// at, because the first thing anyone will want is to run exactly that again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// Which target found it.
    pub target: &'static str,
    /// The seed the case came from.
    pub seed: u64,
    /// What went wrong.
    pub detail: String,
    /// The input, as text where it is text.
    pub input: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "{} at seed {}: {}",
            self.target, self.seed, self.detail
        )?;
        for line in self.input.lines().take(40) {
            writeln!(formatter, "  | {line}")?;
        }
        Ok(())
    }
}

/// What a campaign did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// How many cases were examined.
    pub cases: u64,
    /// How many inputs were accepted by both readers.
    pub accepted: u64,
    /// How many were refused by both.
    pub refused: u64,
    /// Everything that went wrong.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Folds one report into another.
    pub fn absorb(&mut self, other: Self) {
        self.cases += other.cases;
        self.accepted += other.accepted;
        self.refused += other.refused;
        self.findings.extend(other.findings);
    }

    /// Whether the campaign found nothing, which is the expected result.
    #[must_use]
    pub fn clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// The one-line summary a runner prints.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{} case(s): {} accepted, {} refused, {} finding(s)",
            self.cases,
            self.accepted,
            self.refused,
            self.findings.len()
        )
    }
}

/// Runs a closure and turns a panic into a finding rather than a crash.
///
/// A panic in a parser is a denial of service in a tool that reads files from
/// wherever a model came from, so it is a finding here and not merely a bug
/// somewhere else. Catching it is also what lets a campaign report every case
/// rather than stopping at the first.
pub(crate) fn without_panicking<T>(
    what: &'static str,
    seed: u64,
    input: &str,
    body: impl FnOnce() -> T,
) -> Result<T, Finding> {
    let hushed = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    std::panic::set_hook(hushed);
    outcome.map_err(|payload| Finding {
        target: what,
        seed,
        detail: format!("panicked: {}", explain(&payload)),
        input: input.to_owned(),
    })
}

/// Whatever a panic payload can be made to say.
fn explain(payload: &Box<dyn core::any::Any + Send>) -> String {
    payload.downcast_ref::<&str>().map_or_else(
        || {
            payload
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_else(|| "no message".to_owned())
        },
        |text| (*text).to_owned(),
    )
}
