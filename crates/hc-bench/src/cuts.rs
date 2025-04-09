//! Whether a cut ever removes an answer.
//!
//! A cut is a row the model does not have, added because it is believed to
//! exclude no feasible integer point of the region it is scoped to. That belief
//! is the whole of a cut's licence, and it is the one thing about cutting that
//! can silently destroy an answer: a cut that removes the optimum turns a
//! correct search into a confident wrong number, and every proof built on top
//! of it is a valid proof of a false statement.
//!
//! So the belief is not taken. For instances small enough to enumerate, this
//! campaign looks at every feasible integer point the model has, works out
//! which of them lie inside the region a cut is scoped to, and requires the cut
//! to hold at every one of those. Nothing derived is consulted: not the
//! derivation, not the tableau it came from, not the verifier's replay of it.
//! The model's own points are the judge.
//!
//! The scope half matters as much as the validity half. A cut derived at a node
//! is true of that node's region and of nothing outside it, so a point outside
//! the scope is *allowed* to violate the cut — and a campaign that demanded
//! otherwise would be demanding that every local cut be globally valid, which
//! is a different and much weaker thing to have built.

use core::fmt::Write as _;

use hc_core::{Model, Xoshiro256PlusPlus};
use hc_cuts::pool::Entry;
use hc_exact::Rational;
use hc_mip::search::{Options, Search, run as search};
use hc_mip::tree::Assumption;

use crate::exhaustive::for_each_feasible;
use crate::tiny::{BOX_CAP, generate};

/// How many rounds of cut separation the campaign runs at a node.
const ROUNDS: usize = 3;

/// One feasible integer point that a cut excluded.
#[derive(Clone, Debug)]
pub struct Violation {
    /// The round that produced it.
    pub round: usize,
    /// The node the cut is scoped to.
    pub scope: u64,
    /// The point the cut removed, over the model's variables.
    pub point: Vec<Rational>,
    /// The cut's activity at that point, and the side it had to reach.
    pub detail: String,
    /// The instance file.
    pub source: String,
}

/// What a cut-validity campaign found.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Rounds attempted.
    pub rounds: usize,
    /// Rounds whose instance the enumerator could answer.
    pub answerable: usize,
    /// Instances that produced at least one cut.
    pub cutting: usize,
    /// Cuts examined.
    pub cuts: usize,
    /// Cut-and-point pairs checked.
    pub checks: u64,
    /// Feasible integer points removed by a cut scoped to a region holding
    /// them. Every one is a severity-zero defect.
    pub violations: Vec<Violation>,
}

impl Report {
    /// Whether the campaign found nothing.
    #[must_use]
    pub fn clean(&self) -> bool {
        self.violations.is_empty()
    }

    /// The campaign, in a form that fits in a failure message.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut out = format!(
            "{} rounds, {} answerable, {} cutting, {} cuts, {} point checks, {} violations",
            self.rounds,
            self.answerable,
            self.cutting,
            self.cuts,
            self.checks,
            self.violations.len()
        );
        for found in self.violations.iter().take(3) {
            let _ = write!(
                out,
                "\n--- round {}: a cut scoped to node {} removed a feasible integer point\n\
                 {}\npoint: {}\n{}",
                found.round,
                found.scope,
                found.detail,
                found
                    .point
                    .iter()
                    .map(Rational::to_canonical_string)
                    .collect::<Vec<_>>()
                    .join(" "),
                found.source
            );
        }
        out
    }
}

/// Whether a point satisfies every assumption on the path to a node.
///
/// This is what decides that a point is inside a cut's scope, and it is
/// deliberately made from the tree's own assumptions rather than from anything
/// the cut carries. A cut that named a scope it was not derived at would be
/// caught by this rather than excused by it.
fn inside(point: &[Rational], path: &[&Assumption]) -> bool {
    path.iter().all(|assumption| {
        let Some(value) = point.get(assumption.column) else {
            return false;
        };
        if assumption.at_most {
            value.cmp_value(&assumption.value).is_le()
        } else {
            value.cmp_value(&assumption.value).is_ge()
        }
    })
}

/// The cut's activity at a point: the left side of `a . x >= b`.
fn activity(entry: &Entry, point: &[Rational]) -> Rational {
    entry
        .coefficients
        .iter()
        .fold(Rational::zero(), |total, (column, value)| {
            match point.get(*column) {
                Some(at) => total.add(&value.mul(at)),
                None => total,
            }
        })
}

/// Checks every cut one search pooled against every feasible integer point.
fn examine(round: usize, model: &Model, source: &str, found: &Search, report: &mut Report) {
    let entries: Vec<&Entry> = found.pool.entries().iter().collect();
    if entries.is_empty() {
        return;
    }
    report.cutting += 1;
    report.cuts += entries.len();

    // The path to each cut's scope, resolved once rather than per point.
    let paths: Vec<Vec<&Assumption>> = entries
        .iter()
        .map(|entry| found.tree.path(entry.scope))
        .collect();

    let walked = for_each_feasible(model, BOX_CAP, |point| {
        for (entry, path) in entries.iter().zip(&paths) {
            if !inside(point, path) {
                continue;
            }
            report.checks += 1;
            let reached = activity(entry, point);
            if reached.cmp_value(&entry.right).is_lt() {
                report.violations.push(Violation {
                    round,
                    scope: entry.scope,
                    point: point.to_vec(),
                    detail: format!(
                        "the cut reaches {reached} and its right-hand side is {}",
                        entry.right
                    ),
                    source: source.to_owned(),
                });
            }
        }
    });
    if walked.is_ok() {
        report.answerable += 1;
    }
}

/// Runs the campaign.
///
/// Each round generates a tiny all-integer model, solves it with cuts on, and
/// holds every cut the search pooled against every feasible integer point of
/// the region that cut claims to be about.
#[must_use]
pub fn run(seed: u64, rounds: usize) -> Report {
    let mut generator = Xoshiro256PlusPlus::from_seed(seed);
    let mut report = Report::default();

    for round in 0..rounds {
        let instance = generate(&mut generator, 1 + (round % 3), 2 + (round % 3));
        report.rounds += 1;
        let found = search(
            &instance.model,
            &Options {
                cut_rounds: ROUNDS,
                ..Options::default()
            },
        );
        examine(
            round,
            &instance.model,
            &instance.source,
            &found,
            &mut report,
        );
    }
    report
}
