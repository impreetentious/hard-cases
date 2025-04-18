//! Running the fetched mixed-integer corpus.
//!
//! The generated campaigns cover breadth and the local corpus covers a handful
//! of structures a generator will not draw. Neither reaches what this does:
//! instances written by other people, for other purposes, at sizes nobody here
//! chose. A solver that has only ever been run on its own generator has been
//! measured against its author's imagination.
//!
//! Two things make this a gate rather than a demonstration. Every terminal
//! status a run reaches must certify and be accepted by the standalone
//! verifier — an answer that cannot be checked is not an answer here, whatever
//! it says. And on the development split, the search is run twice, with cuts
//! and without, and the two must agree about the value: a cut that changes the
//! answer is the catastrophe the whole cut apparatus is built to make
//! impossible, and this is the only place it meets instances it was not
//! written against.
//!
//! What this does not do is tune anything. The held-out half is looked at and
//! never adjusted against, and several of its instances are far beyond a
//! single-threaded search with no probing and no restarts. Those come back as
//! no-progress, which is an honest answer and the one the board prints.

use hc_cert::mip::{assemble_mip, assemble_presolved};
use hc_core::Model;
use hc_io::mps::{Form, read};
use hc_mip::search::{CUT_ROUNDS, Ending, Options, run};
use hc_presolve::reduce;
use hc_verify::vclose::verify;

/// What one instance did.
#[derive(Clone, Debug)]
pub struct Row {
    /// The instance's name.
    pub name: String,
    /// Rows and columns of the parsed model.
    pub shape: (usize, usize),
    /// Stored coefficients.
    pub nonzeros: usize,
    /// Integer columns.
    pub integers: usize,
    /// How the search ended.
    pub ending: String,
    /// Nodes opened.
    pub nodes: u64,
    /// What the verifier said, when a certificate was assembled at all.
    pub verified: Option<String>,
    /// The objective the incumbent attains, as exact text.
    pub objective: Option<String>,
    /// What went wrong, when something did.
    pub finding: Option<String>,
}

impl Row {
    /// Whether this row is a finding.
    #[must_use]
    pub fn is_finding(&self) -> bool {
        self.finding.is_some()
    }

    /// The line the board prints for it.
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{:<16} {:>6} {:>6} {:>7} {:>6}  {:<12} {:>7}  {:<10} {}",
            self.name,
            self.shape.0,
            self.shape.1,
            self.nonzeros,
            self.integers,
            self.ending,
            self.nodes,
            self.verified.clone().unwrap_or_else(|| "-".to_owned()),
            self.objective.clone().unwrap_or_else(|| "-".to_owned())
        )
    }
}

/// The header the board prints above its rows.
pub const HEADING: &str =
    "instance             rows   cols      nz   ints  ending          nodes  verified    objective";

/// Solves one instance and has the standalone verifier recheck the result.
///
/// `nodes` is the effort cap, counted in nodes rather than in seconds so that
/// a run means the same thing on every machine.
#[must_use]
pub fn run_one(name: &str, raw: &[u8], nodes: u64) -> Row {
    run_with(name, raw, nodes, CUT_ROUNDS)
}

/// Solves one instance under a chosen number of separation rounds.
///
/// The board uses this to report what cuts are worth: the same instance, the
/// same everything else, once with separation and once without. Nothing else
/// should reach for it — a harness that ran the product's search under
/// settings the product does not use would be reporting on another program,
/// which is what [`run_one`] exists to prevent.
#[must_use]
pub fn run_with(name: &str, raw: &[u8], nodes: u64, cut_rounds: usize) -> Row {
    let model: Model = match read(raw, Form::Free) {
        Ok(model) => model,
        Err(error) => {
            return Row {
                name: name.to_owned(),
                shape: (0, 0),
                nonzeros: 0,
                integers: 0,
                ending: "unreadable".to_owned(),
                nodes: 0,
                verified: None,
                objective: None,
                // An instance the readers refuse is a finding about the
                // readers, not a reason to leave the instance out.
                finding: Some(format!("did not parse: {error}")),
            };
        }
    };

    let integers = model
        .variables()
        .iter()
        .filter(|variable| variable.integral)
        .count();
    let shape = (model.rows().len(), model.variables().len());
    let nonzeros = model.matrix().nonzeros();

    let chain = reduce(&model);
    let options = Options {
        node_limit: nodes,
        cut_rounds,
        ..Options::default()
    };
    let search = run(&chain.reduced, &options);
    let ending = format!("{:?}", search.ending).to_lowercase();
    let objective = search
        .incumbent
        .as_ref()
        .map(|incumbent| incumbent.objective.to_canonical_string());

    // Every terminal status has to certify. A status nobody can check is not a
    // status this project reports.
    let assembled = assemble_presolved(&model, &chain, raw, &search);
    let (verified, finding) = match assembled {
        Ok(assembled) => {
            let bytes = assembled.encode();
            match verify(&bytes, raw) {
                Ok(verdict) => (Some(format!("{verdict:?}").to_lowercase()), None),
                Err(refusal) => (
                    None,
                    Some(format!("the verifier refused the certificate: {refusal}")),
                ),
            }
        }
        Err(reason) => {
            let terminal = matches!(
                search.ending,
                Ending::Optimal | Ending::Infeasible | Ending::Frontier
            );
            (
                None,
                terminal.then(|| format!("a terminal search assembled nothing: {reason}")),
            )
        }
    };

    Row {
        name: name.to_owned(),
        shape,
        nonzeros,
        integers,
        ending,
        nodes: search.opened,
        verified,
        objective,
        finding,
    }
}

/// Solves one instance twice, with cuts and without, and compares the answers.
///
/// The comparison is on the value and never on the effort. A cut that removes
/// a feasible integer point changes the value, and that is the failure this
/// exists to catch; a cut that costs more nodes than it saves on some instance
/// is a performance question and is not a correctness one.
///
/// # Panics
///
/// Panics when the bytes are not a readable instance, which a fetched and
/// pinned corpus entry has already been shown not to be.
#[must_use]
pub fn cuts_agree(name: &str, raw: &[u8], nodes: u64) -> Option<String> {
    let model = read(raw, Form::Free).unwrap_or_else(|error| panic!("{name}: {error}"));
    let with = run(
        &model,
        &Options {
            node_limit: nodes,
            cut_rounds: CUT_ROUNDS,
            ..Options::default()
        },
    );
    let without = run(
        &model,
        &Options {
            node_limit: nodes,
            cut_rounds: 0,
            ..Options::default()
        },
    );

    // Only a search that closed says anything about the value. Two runs that
    // both ran out of nodes may honestly disagree about what they found so
    // far, and demanding otherwise would be demanding that a partial search be
    // deterministic in something it never claimed.
    let closed = |search: &hc_mip::search::Search| {
        matches!(search.ending, Ending::Optimal | Ending::Infeasible)
    };
    if !closed(&with) || !closed(&without) {
        return None;
    }
    if with.ending != without.ending {
        return Some(format!(
            "{name}: cuts on ended {:?} and cuts off ended {:?}",
            with.ending, without.ending
        ));
    }
    let value = |search: &hc_mip::search::Search| {
        search
            .incumbent
            .as_ref()
            .map(|incumbent| incumbent.objective.to_canonical_string())
    };
    if value(&with) != value(&without) {
        return Some(format!(
            "{name}: cuts on attained {:?} and cuts off attained {:?}",
            value(&with),
            value(&without)
        ));
    }
    None
}

/// Cross-checks a solved value against a published one.
///
/// The published file is optional and is never written by this project. A
/// value this solver produced and then agreed with is not a cross-check.
#[must_use]
pub fn against_published(row: &Row, published: Option<&str>) -> Option<String> {
    let (Some(published), Some(attained)) = (published, row.objective.as_deref()) else {
        return None;
    };
    if published == attained {
        return None;
    }
    Some(format!(
        "{}: attained {attained} and the published value is {published}",
        row.name
    ))
}

/// The certificate assembled for a model with no reductions, for callers that
/// want the unreduced path.
///
/// # Errors
///
/// Returns the reason nothing could be assembled.
pub fn certify_plain(model: &Model, raw: &[u8], nodes: u64) -> Result<Vec<u8>, String> {
    let search = run(
        model,
        &Options {
            node_limit: nodes,
            ..Options::default()
        },
    );
    assemble_mip(model, raw, &search)
        .map(|assembled| assembled.encode())
        .map_err(|reason| reason.to_string())
}
