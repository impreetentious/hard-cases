//! The solver target.
//!
//! The parser targets ask whether two readers agree about a file. This asks
//! something narrower and harder: whether the search, handed a file it was
//! never written for, ends in a state it is allowed to be in — and whether
//! anything it claims survives being checked by the other implementation.
//!
//! Four properties.
//!
//! **No panic.** A solver that stops on an unusual model stops on a file
//! somebody handed it, and the file is easier to produce than the fix.
//!
//! **The ending means what it says.** Optimal requires an incumbent;
//! infeasible forbids one; a frontier requires an open node carrying an exact
//! bound; no-progress requires one that carries none. An ending is a claim
//! about the tree, and a tree that does not support it is a claim nobody
//! checked.
//!
//! **Nothing is claimed that the verifier does not accept.** Every certificate
//! the assembler produces goes straight to the standalone verifier. A refusal
//! here is the most serious finding this crate can produce, because it means
//! the two sides disagree about a proof rather than about a file.
//!
//! **The run is reproducible.** The same instance solved twice produces the
//! same certificate and the same trace, byte for byte, in the same process and
//! from the same seed.

use hc_cert::mip::{assemble_mip, assemble_presolved};
use hc_cert::trace::{Run, of as trace_of, status_of};
use hc_core::Model;
use hc_mip::search::{Ending, Node, Options, Search, run as search};
use hc_presolve::reduce;
use hc_verify::vclose::verify;

use crate::edits::{Edit, apply};
use crate::shapes::{Shape, draft, render};
use crate::{Budget, Finding, Report, without_panicking};

/// The effort every case is solved under.
///
/// Tiny on purpose. The point is to reach many shapes, not to solve any of
/// them, and an instance that runs out of nodes exercises the frontier and the
/// no-progress endings that a generous budget never reaches.
const NODES: u64 = 40;

/// Runs the solver target over a budget.
#[must_use]
pub fn campaign(budget: Budget) -> Report {
    let mut report = Report::default();
    for seed in budget.seeds() {
        let drafted = draft(seed);
        for shape in Shape::ALL {
            let text = render(&drafted, shape, seed);
            for edit in [Edit::None, Edit::Byte, Edit::DropLine, Edit::SwapLines] {
                let mutated = apply(&text, edit, seed ^ (edit as u64).wrapping_mul(97));
                let Some(model) = readable(&mutated, shape) else {
                    continue;
                };
                report.cases += 1;
                let what = format!("{} {}", shape.as_str(), edit.as_str());
                match one(seed, &what, &mutated, &model) {
                    Ok(()) => report.accepted += 1,
                    Err(finding) => report.findings.push(finding),
                }
            }
        }
    }
    report
}

/// Reads a rendering, or reports that there is nothing to solve.
fn readable(text: &str, shape: Shape) -> Option<Model> {
    let raw = text.as_bytes();
    match shape {
        Shape::MpsFixed => hc_io::mps::read(raw, hc_io::mps::Form::Fixed),
        Shape::MpsFree => hc_io::mps::read(raw, hc_io::mps::Form::Free),
        Shape::Lp => hc_io::lp::read(raw),
        Shape::Json => hc_io::json::read(raw),
    }
    .ok()
}

/// One case: solve it, check what it says, and have the claim rechecked.
fn one(seed: u64, what: &str, text: &str, model: &Model) -> Result<(), Finding> {
    let finding = |detail: String| Finding {
        target: "the search",
        seed,
        detail: format!("{what}: {detail}"),
        input: text.to_owned(),
    };
    let options = Options {
        node_limit: NODES,
        cut_rounds: 2,
        ..Options::default()
    };

    let found = without_panicking("the search", seed, text, || search(model, &options))?;
    if let Some(wrong) = ending_holds(&found) {
        return Err(finding(wrong));
    }

    // The certificate, through the assembler the command line uses, and then
    // straight to the other implementation.
    let raw = text.as_bytes();
    let assembled = without_panicking("the assembler", seed, text, || {
        assemble_mip(model, raw, &found)
    })?;
    if let Ok(certificate) = &assembled {
        let bytes = certificate.encode();
        let verdict = without_panicking("the verifier", seed, text, || verify(&bytes, raw))?;
        if let Err(refusal) = verdict {
            return Err(finding(format!(
                "the verifier refused a certificate this search produced: {refusal}"
            )));
        }
        // The same solve again, and the same bytes. A certificate that moved
        // between two runs could not be compared between two versions.
        let again = search(model, &options);
        let repeated = assemble_mip(model, raw, &again)
            .map(|certificate| certificate.encode())
            .unwrap_or_default();
        if repeated != bytes {
            return Err(finding(
                "two runs of one solve produced different certificates".to_owned(),
            ));
        }

        let trace = trace_of(&Run {
            chain: None,
            search: &found,
            certificate: Some(certificate),
            bytes: Some(bytes.len() as u64),
            verdict: None,
            status: status_of(found.ending, found.incumbent.is_some()),
        });
        let written = trace.encode().map_err(|error| {
            finding(format!("the trace of this run cannot be written: {error}"))
        })?;
        let read = hc_core::trace::Trace::decode(&written)
            .map_err(|error| finding(format!("the trace of this run cannot be read: {error}")))?;
        if read != trace {
            return Err(finding(
                "a trace did not survive its own encoding".to_owned(),
            ));
        }
    }

    // And the same run with the model reduced first, which is the path the
    // command line takes by default. A reduction that changes the answer is a
    // reduction that is wrong.
    let chain = without_panicking("presolve", seed, text, || reduce(model))?;
    let reduced = without_panicking("the search under presolve", seed, text, || {
        search(&chain.reduced, &options)
    })?;
    if let Some(wrong) = ending_holds(&reduced) {
        return Err(finding(format!("under presolve, {wrong}")));
    }
    if let Ok(certificate) = assemble_presolved(model, &chain, raw, &reduced) {
        let bytes = certificate.encode();
        let verdict = without_panicking("the verifier", seed, text, || verify(&bytes, raw))?;
        if let Err(refusal) = verdict {
            return Err(finding(format!(
                "the verifier refused a presolved certificate: {refusal}"
            )));
        }
    }

    // The two paths must agree about the answer where both closed. A presolve
    // that changed a value would be the worst defect this campaign can find,
    // and it is checked here rather than left to the certificate — a wrong
    // answer that certifies is still a wrong answer.
    let value = |found: &Search| {
        found
            .incumbent
            .as_ref()
            .map(|incumbent| incumbent.objective.to_canonical_string())
    };
    if closed(found.ending) && closed(reduced.ending) {
        if found.ending != reduced.ending {
            return Err(finding(format!(
                "presolve off ended {:?} and presolve on ended {:?}",
                found.ending, reduced.ending
            )));
        }
        if value(&found) != value(&reduced) {
            return Err(finding(format!(
                "presolve off attained {:?} and presolve on attained {:?}",
                value(&found),
                value(&reduced)
            )));
        }
    }
    Ok(())
}

/// Whether an ending is one the search reached rather than ran out of.
fn closed(ending: Ending) -> bool {
    matches!(ending, Ending::Optimal | Ending::Infeasible)
}

/// Whether the tree supports the ending the search reported.
fn ending_holds(found: &Search) -> Option<String> {
    let open = |wanted: fn(&Node) -> bool| found.nodes.iter().flatten().any(wanted);
    match found.ending {
        Ending::Optimal if found.incumbent.is_none() => {
            Some("optimal with no incumbent".to_owned())
        }
        Ending::Infeasible if found.incumbent.is_some() => {
            Some("infeasible with an incumbent".to_owned())
        }
        Ending::Frontier if !open(|node| matches!(node, Node::Frontier { .. })) => {
            Some("a frontier with no open node carrying a bound".to_owned())
        }
        Ending::NoProgress
            if !open(|node| matches!(node, Node::Unproved { .. } | Node::Suspect { .. })) =>
        {
            Some("no progress with nothing unproved".to_owned())
        }
        _ => None,
    }
}
