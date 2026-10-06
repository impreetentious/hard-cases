//! The reduction chain, and the order it is built in.
//!
//! The fixpoint runs the eight transforms in order, each scanning candidates
//! by ascending index, and restarts at the first one after any change. That
//! order is fixed so the chain is reproducible; the proof's correctness does
//! not depend on it, because any chain whose records satisfy the two
//! invariants is valid in any order.
//!
//! Termination needs more than it looks like it needs. Seven of the eight
//! transforms remove a row or a column, so between them they can fire at most
//! as often as the model has rows and columns. The eighth tightens a bound,
//! and requires a strict improvement — which sounds like the same argument and
//! is not one. Two rows can tighten each other by ever-smaller exact amounts
//! forever, each improvement strict, the sequence converging on a limit it
//! never reaches, and the rationals growing at every step until the arithmetic
//! is the slowest part of the program. That is not a hypothetical: it is what
//! a generated four-column model did, and it is why the tightening rule is
//! given a budget below.
//!
//! A derivation that closes a domain to nothing ends the chain. It is not a
//! failure of the transform and it is not an ad hoc witness: the record names
//! the derivation, and the verifier recomputes it on the model it holds.

use std::collections::BTreeMap;

use hc_core::{Model, Sha256};
use hc_exact::Rational;

use crate::draft::Draft;
use crate::map::reverse;
use crate::rule::{Inapplicable, Rule, apply, empty_column_value, singleton_bounds};

/// How often one side of one column's bound may be tightened.
///
/// A budget rather than a threshold, because the alternative is a threshold: a
/// smallest improvement worth taking, which is a tolerance, and a tolerance in
/// an exact reduction is the one thing this project has no room for. Counting
/// is honest instead — presolve is an optimization, a reduction it declines to
/// make costs nothing but effort, and stopping early leaves a weaker reduced
/// model that is still exactly equivalent to the original.
///
/// Eight is generous for a model that converges and immediately fatal to one
/// that does not. Every real tightening comes from a different row or a
/// different neighbour's bound, and a column whose bound has moved eight times
/// on one side is a column being chased rather than reduced.
pub const TIGHTENINGS_PER_SIDE: usize = 8;

/// One step of a chain: the rule, and the hash of what it produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The transform.
    pub rule: Rule,
    /// The canonical hash of the model it produces.
    pub result: [u8; 32],
}

/// A derivation that closed a domain to nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contradiction {
    /// The rule whose derivation emptied the domain.
    pub rule: Rule,
}

/// A reduction chain.
#[derive(Clone, Debug)]
pub struct Chain {
    /// The steps, in the order they were applied.
    pub steps: Vec<Step>,
    /// The model the chain ends at.
    pub reduced: Model,
    /// The derivation that ended the chain, when one did.
    pub contradiction: Option<Contradiction>,
}

impl Chain {
    /// Lifts a point of the reduced model back to the original.
    ///
    /// The walk is backwards through the stages, which means rebuilding each
    /// stage's predecessor. That is slower than remembering them and is what
    /// the verifier will do anyway, so doing it the same way here means the
    /// two sides exercise the same path.
    #[must_use]
    pub fn lift(&self, original: &Model, point: &[Rational]) -> Vec<Rational> {
        // Every intermediate model, in order, so a backward walk has the model
        // each transform was applied to.
        let mut stages: Vec<Model> = vec![original.clone()];
        for step in &self.steps {
            let last = stages.last().expect("the original is present");
            match apply(last, &step.rule) {
                Ok(next) => stages.push(next),
                Err(_) => break,
            }
        }

        let mut carried = point.to_vec();
        for (position, step) in self.steps.iter().enumerate().rev() {
            let Some(before) = stages.get(position) else {
                continue;
            };
            carried = reverse(before, &step.rule, &carried);
        }
        carried
    }
}

/// The canonical hash of a model: the digest of its canonical dump.
#[must_use]
pub fn model_hash(model: &Model) -> [u8; 32] {
    let mut state = Sha256::new();
    state.update(model.canonical_dump().as_bytes());
    state.finish()
}

/// Runs the fixpoint.
#[must_use]
pub fn reduce(model: &Model) -> Chain {
    let mut current = model.clone();
    let mut steps: Vec<Step> = Vec::new();
    let mut contradiction = None;
    // One count per column and side, so the tightening rule cannot be chased
    // around a cycle of ever-smaller improvements.
    let mut spent: BTreeMap<(usize, bool), usize> = BTreeMap::new();

    'fixpoint: loop {
        for rule in candidates(&current) {
            if let Rule::TightenBound { column, upper, .. } = &rule
                && spent.get(&(*column, *upper)).copied().unwrap_or(0) >= TIGHTENINGS_PER_SIDE
            {
                continue;
            }
            match apply(&current, &rule) {
                Ok(next) => {
                    if let Rule::TightenBound { column, upper, .. } = &rule {
                        *spent.entry((*column, *upper)).or_insert(0) += 1;
                    }
                    steps.push(Step {
                        rule,
                        result: model_hash(&next),
                    });
                    current = next;
                    continue 'fixpoint;
                }
                Err(Inapplicable::Contradiction) => {
                    contradiction = Some(Contradiction { rule });
                    break 'fixpoint;
                }
                Err(_) => {}
            }
        }
        break;
    }

    Chain {
        steps,
        reduced: current,
        contradiction,
    }
}

/// Every rule worth trying on a model, in the order the specification fixes.
///
/// Generating candidates and then testing them, rather than testing inside
/// eight bespoke scans, keeps the applicability conditions in exactly one
/// place: the transform itself. A scan that decided applicability would be a
/// second implementation of the condition, and the two would eventually
/// disagree.
fn candidates(model: &Model) -> Vec<Rule> {
    let draft = Draft::of(model);
    let mut out = Vec::new();

    // T1
    for (column, variable) in draft.variables.iter().enumerate() {
        if let (Some(low), Some(high)) = variable.effective_bounds()
            && low == high
        {
            out.push(Rule::FixedColumn { column, value: low });
        }
    }
    // T2
    for (column, variable) in draft.variables.iter().enumerate() {
        if draft.column_entries(column).is_empty()
            && let Some(value) = empty_column_value(variable)
        {
            out.push(Rule::EmptyColumn { column, value });
        }
    }
    // T3
    for row in 0..draft.rows.len() {
        let entries = draft.row_entries(row);
        if entries.len() == 1 {
            out.push(Rule::SingletonRow {
                row,
                column: entries[0].0,
            });
        }
    }
    // T4
    for column in 0..draft.variables.len() {
        let appearances = draft.column_entries(column);
        if appearances.len() == 1 {
            out.push(Rule::FreeSingletonColumn {
                row: appearances[0].0,
                column,
            });
        }
    }
    // T5
    for row in 0..draft.rows.len() {
        out.push(Rule::RedundantRow { row });
    }
    // T6
    for row in 0..draft.rows.len() {
        out.push(Rule::ForcingRow { row, upper: false });
        out.push(Rule::ForcingRow { row, upper: true });
    }
    // T7
    for row in 0..draft.rows.len() {
        for (column, _) in draft.row_entries(row) {
            for from_upper in [false, true] {
                if let Some((upper, value)) =
                    crate::rule::derived_bound(&draft, row, column, from_upper)
                {
                    out.push(Rule::TightenBound {
                        row,
                        column,
                        upper,
                        value,
                    });
                }
            }
        }
    }
    // T8
    for keep in 0..draft.variables.len() {
        for drop in (keep + 1)..draft.variables.len() {
            out.push(Rule::DuplicateColumn { keep, drop });
        }
    }

    out
}

/// The bounds a singleton row implies, re-exported for the fixture harness.
///
/// The rounding direction on an integer column is the part of T3 most easily
/// got backwards, and a fixture that reached it through a whole reduction
/// would be testing the fixpoint rather than the rule.
#[must_use]
pub fn singleton_derivation(
    coefficient: &Rational,
    lower: Option<&Rational>,
    upper: Option<&Rational>,
    integral: bool,
) -> (Option<Rational>, Option<Rational>) {
    singleton_bounds(coefficient, lower, upper, integral)
}
