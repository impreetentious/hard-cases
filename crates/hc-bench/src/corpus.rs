//! Running a corpus of real instance files through both solvers.
//!
//! The generated campaign covers breadth: thousands of small shapes drawn from
//! a distribution nobody designed. A corpus covers something the generator
//! cannot reach — structure. A transportation matrix, a covering matrix, a
//! flow conservation system, and a cycling example all have properties that a
//! random draw produces essentially never, and those properties are what
//! solver implementations are actually wrong about.
//!
//! Every instance is read from bytes handed in by the caller. This crate is
//! outside the deterministic perimeter and may touch the filesystem, but a
//! runner that opened files itself would be a runner whose results depend on
//! where it was run from, so the reading stays at the edge and the running
//! stays here.
//!
//! What the manifest records is evidence, not truth. At this stage the
//! objective for an instance with an optimum is the value two independent
//! solvers agree on — which is real evidence and is not a proof, and is
//! labelled as the former. When exact assembly exists, the same field carries
//! a value an independent checker accepted, and the label changes with it.

use std::collections::BTreeMap;

use hc_core::{Model, Tol};
use hc_io::mps::{Form, read};
use hc_lp::{Options, Problem, Working};

use crate::campaign::AGREEMENT_FENCE;
use crate::ref_lp;

/// What an instance is expected to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expected {
    /// A finite optimum.
    Optimal,
    /// No feasible point.
    Infeasible,
    /// Objective values without limit.
    Unbounded,
}

impl Expected {
    /// The expectation a manifest word names.
    #[must_use]
    pub fn named(text: &str) -> Option<Self> {
        match text {
            "optimal" => Some(Self::Optimal),
            "infeasible" => Some(Self::Infeasible),
            "unbounded" => Some(Self::Unbounded),
            _ => None,
        }
    }

    /// The word this expectation is written as.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Optimal => "optimal",
            Self::Infeasible => "infeasible",
            Self::Unbounded => "unbounded",
        }
    }
}

/// One row of the board.
#[derive(Clone, Debug)]
pub struct Row {
    /// The instance's file name.
    pub name: String,
    /// Rows and columns of the parsed model.
    pub shape: (usize, usize),
    /// Stored coefficients.
    pub nonzeros: usize,
    /// What the production solver concluded.
    pub solver: String,
    /// What the reference solver concluded.
    pub reference: String,
    /// The objective, when both reached one.
    pub objective: Option<f64>,
    /// Simplex iterations spent by the production solver.
    pub iterations: u64,
    /// What went wrong, when something did.
    pub finding: Option<String>,
}

impl Row {
    /// Whether this row is a finding.
    #[must_use]
    pub fn is_finding(&self) -> bool {
        self.finding.is_some()
    }
}

/// The whole board.
#[derive(Clone, Debug, Default)]
pub struct Board {
    /// One row per instance, in the order they were handed over.
    pub rows: Vec<Row>,
}

impl Board {
    /// Every row that is a finding.
    #[must_use]
    pub fn findings(&self) -> Vec<&Row> {
        self.rows.iter().filter(|row| row.is_finding()).collect()
    }

    /// How many rows reached each status.
    #[must_use]
    pub fn tally(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for row in &self.rows {
            *counts.entry(row.solver.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// A plain-text rendering, for a report or a failure message.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "instance                rows  cols     nz  solver      reference   iters  objective\n",
        );
        for row in &self.rows {
            out.push_str(&format!(
                "{:<22} {:>5} {:>5} {:>6}  {:<11} {:<11} {:>5}  {}\n",
                row.name,
                row.shape.0,
                row.shape.1,
                row.nonzeros,
                row.solver,
                row.reference,
                row.iterations,
                row.objective
                    .map_or_else(|| "-".to_owned(), |value| format!("{value:.6}"))
            ));
            if let Some(finding) = &row.finding {
                out.push_str(&format!("    finding: {finding}\n"));
            }
        }
        out
    }
}

/// Runs one instance through both solvers.
///
/// # Panics
///
/// Panics when the bytes are not an instance file. A corpus entry that does
/// not parse is a defect in the corpus, and a runner that skipped it would
/// report a board with a hole in it.
#[must_use]
pub fn run_one(name: &str, raw: &[u8], expected: Option<Expected>) -> Row {
    let model: Model = read(raw, Form::Fixed)
        .unwrap_or_else(|error| panic!("{name} is not a readable instance: {error}"));
    let working = Working::from_model(&model);
    let problem = Problem::scaled(&working, Tol::solver());

    let (outcome, effort) = hc_lp::solve(&problem, &Options::default());
    let reference = ref_lp::solve(&model);

    let solver_name = outcome.as_str().to_owned();
    let reference_name = match &reference {
        ref_lp::Outcome::Optimal { .. } => "optimal",
        ref_lp::Outcome::Infeasible => "infeasible",
        ref_lp::Outcome::Unbounded => "unbounded",
        ref_lp::Outcome::Exhausted => "exhausted",
    }
    .to_owned();

    let mut finding = None;
    let mut objective = None;

    match (&outcome, &reference) {
        (
            hc_lp::Outcome::Optimal {
                objective: ours,
                point,
                ..
            },
            ref_lp::Outcome::Optimal {
                objective: theirs, ..
            },
        ) => {
            objective = Some(*ours);
            let bound = AGREEMENT_FENCE * ours.abs().max(1.0);
            if (ours - theirs).abs() > bound {
                finding = Some(format!(
                    "objectives differ: solver {ours}, reference {theirs}"
                ));
            } else {
                let recovered = problem.recover_point(point);
                let structural = &recovered[..working.structural()];
                if !ref_lp::is_feasible(&model, structural, AGREEMENT_FENCE.sqrt()) {
                    finding = Some("the solver's optimum is not feasible in the model".to_owned());
                }
            }
        }
        (hc_lp::Outcome::Infeasible { .. }, ref_lp::Outcome::Infeasible)
        | (hc_lp::Outcome::Unbounded { .. }, ref_lp::Outcome::Unbounded) => {}
        _ => {
            finding = Some(format!(
                "statuses differ: solver {solver_name}, reference {reference_name}"
            ));
        }
    }

    if let Some(expected) = expected
        && finding.is_none()
        && solver_name != expected.as_str()
    {
        finding = Some(format!(
            "the manifest expects {}, and both solvers said {solver_name}",
            expected.as_str()
        ));
    }

    Row {
        name: name.to_owned(),
        shape: (model.rows().len(), model.variables().len()),
        nonzeros: model.matrix().nonzeros(),
        solver: solver_name,
        reference: reference_name,
        objective,
        iterations: effort.iterations,
        finding,
    }
}

/// Runs a whole corpus.
#[must_use]
pub fn run(entries: &[(&str, &[u8], Option<Expected>)]) -> Board {
    Board {
        rows: entries
            .iter()
            .map(|(name, raw, expected)| run_one(name, raw, *expected))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expectations_round_trip_through_their_words() {
        for expected in [Expected::Optimal, Expected::Infeasible, Expected::Unbounded] {
            assert_eq!(Expected::named(expected.as_str()), Some(expected));
        }
        assert_eq!(Expected::named("probably"), None);
        assert_eq!(Expected::named("no-progress"), None);
    }

    #[test]
    fn a_board_renders_its_rows_and_names_its_findings() {
        let board = Board {
            rows: vec![Row {
                name: "example.mps".to_owned(),
                shape: (3, 4),
                nonzeros: 9,
                solver: "optimal".to_owned(),
                reference: "infeasible".to_owned(),
                objective: Some(1.5),
                iterations: 7,
                finding: Some("statuses differ".to_owned()),
            }],
        };
        let text = board.render();
        assert!(text.contains("example.mps"));
        assert!(text.contains("finding: statuses differ"));
        assert_eq!(board.findings().len(), 1);
        assert_eq!(board.tally().get("optimal"), Some(&1));
    }
}
