//! The browser surface.
//!
//! Three things, and the arrangement between them is the point.
//!
//! **A live solve, bounded.** The page hands over the bytes of an instance and
//! a node budget, and gets back what a run of this solver did: the trace, the
//! certificate, and what the verifier said about it. Bounded because a page
//! that hangs is a page nobody can use, and the budget is in nodes rather than
//! seconds so that the same instance behaves the same way on every machine.
//!
//! **A replay.** The same result shape, assembled from a trace and a
//! certificate recorded natively rather than from a solve. The two paths
//! return the same fields, and the result says which one produced it, because
//! a panel that could not tell a live solve from a recording would eventually
//! label one as the other.
//!
//! **The verifier, alone.** It is compiled into this module and reached
//! through its public interface exactly as the command line reaches it. Being
//! in one wasm module with the solver is not the same as sharing anything with
//! it: the crate has no dependencies, its own arithmetic, its own parsers, and
//! a gate that fails the build if that stops being true.
//!
//! **Two ways of looking.** The glass runs the root relaxation under the
//! textbook pricing rule and reports every pivot it made, and the outline
//! reads a certificate through the verifier's own reader and reports the
//! structure beside the verdict. Neither adds anything: the glass is the
//! ordinary search with a trail attached, and the outline is what the
//! independent checker saw, which is the only reading of a certificate worth
//! drawing.
//!
//! There is no binding generator. The exports are plain functions over a
//! pointer and a length, results are UTF-8 JSON left in this module's memory,
//! and the page reads them out. That costs about forty lines of JavaScript and
//! no dependency, which is the trade this project makes everywhere else.

use core::fmt::Write as _;

use hc_cert::mip::assemble_presolved;
use hc_cert::trace::{Run, Verdict, of as trace_of, status_of};
use hc_core::trace::Trace;
use hc_core::{FormatTag, Model, Tol};
use hc_lp::basis::Basis;
use hc_lp::pricing::Rule;
use hc_lp::problem::Problem;
use hc_lp::simplex::{self, Trail};
use hc_lp::working::{Kind, Working};
use hc_mip::search::{CUT_ROUNDS, Options, run as search};
use hc_presolve::reduce;
use hc_tamper::{CASES, forge, instance_for};
use hc_verify::vclose::verify;
use hc_verify::vhcc::{Step, read as read_proof};

/// How a result was produced.
///
/// Carried in every result and rendered by the page at all times. A panel that
/// could not tell a live solve from a recording would label one as the other
/// on the first day somebody was in a hurry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// This module solved it, here, now.
    Live,
    /// A trace and a certificate recorded elsewhere, replayed.
    Replay,
}

impl Provenance {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Replay => "replay",
        }
    }
}

/// Reads an instance under the rules a format tag names.
fn read(raw: &[u8], format: u8) -> Result<Model, String> {
    match FormatTag::from_byte(format) {
        Some(FormatTag::MpsFixed) => hc_io::mps::read(raw, hc_io::mps::Form::Fixed),
        Some(FormatTag::MpsFree) => hc_io::mps::read(raw, hc_io::mps::Form::Free),
        Some(FormatTag::Lp) => hc_io::lp::read(raw),
        Some(FormatTag::Json) => hc_io::json::read(raw),
        None => return Err(format!("format tag {format} names no format")),
    }
    .map_err(|refusal| refusal.to_string())
}

/// Solves an instance and reports everything the run produced.
///
/// # Errors
///
/// Returns the message to show when the instance cannot be read.
pub fn solve_live(raw: &[u8], format: u8, nodes: u64) -> Result<String, String> {
    let model = read(raw, format)?;
    let chain = reduce(&model);
    let found = search(
        &chain.reduced,
        &Options {
            node_limit: nodes,
            // The same separation the command line runs. A browser surface
            // quietly solving with its cuts off would be showing a different
            // program from the one this repository ships.
            cut_rounds: CUT_ROUNDS,
            ..Options::default()
        },
    );

    let assembled = assemble_presolved(&model, &chain, raw, &found).ok();
    let proof = assembled.as_ref().map(hc_cert::hcc::Certificate::encode);
    let verdict = proof.as_ref().map(|bytes| match verify(bytes, raw) {
        Ok(verdict) => Verdict {
            accepted: true,
            rejection: None,
            work: verdict.work,
        },
        Err(refusal) => Verdict {
            accepted: false,
            rejection: Some(refusal.code.text().to_owned()),
            work: 0,
        },
    });

    let trace = trace_of(&Run {
        instance: *model.digest(),
        chain: Some(&chain),
        search: &found,
        certificate: assembled.as_ref(),
        proof: proof.as_deref(),
        verdict: verdict.clone(),
        status: status_of(found.ending, found.incumbent.is_some()),
    });
    Ok(report(
        Provenance::Live,
        &model,
        Some(&chain.reduced),
        &trace,
        proof.as_deref(),
        verdict.as_ref(),
    ))
}

/// Replays a recorded trace and certificate against the instance they name.
///
/// The certificate is rechecked here rather than believed. A replay that
/// displayed a recorded verdict would be showing a claim about a run nobody in
/// the room performed, and the difference between that and a proof is the
/// whole subject.
///
/// # Errors
///
/// Returns the message to show when any of the three does not read, or when
/// they are not about each other.
pub fn replay(trace: &[u8], proof: &[u8], raw: &[u8], format: u8) -> Result<String, String> {
    let model = read(raw, format)?;
    let recorded = Trace::decode(trace).map_err(|error| error.to_string())?;
    if recorded.instance != *model.digest() {
        return Err("the trace is about a different instance from the one supplied".to_owned());
    }
    // And about this certificate, not merely about this instance. A trace
    // records the length and the trailing checksum of the proof its run
    // assembled, so a recording drawn beside an unrelated certificate — two
    // genuine artifacts of two different runs of the same model, which is the
    // easiest facade to build here by accident — is refused rather than
    // rendered. A trace that never assembled a proof has nothing to bind and
    // is only allowed alongside no certificate.
    let stated = recorded.events.iter().find_map(|event| match event {
        hc_core::trace::Event::ProofAssembled { bytes, checksum } => Some((*bytes, *checksum)),
        _ => None,
    });
    let carried = (
        proof.len() as u64,
        u64::from(u32::from_le_bytes(
            proof
                .get(proof.len().saturating_sub(4)..)
                .and_then(|tail| tail.try_into().ok())
                .unwrap_or([0; 4]),
        )),
    );
    match stated {
        Some(bound) if bound != carried => {
            return Err(
                "the trace records a different certificate from the one supplied".to_owned(),
            );
        }
        None if !proof.is_empty() => {
            return Err("the trace records no certificate and one was supplied".to_owned());
        }
        _ => {}
    }
    let verdict = match verify(proof, raw) {
        Ok(verdict) => Verdict {
            accepted: true,
            rejection: None,
            work: verdict.work,
        },
        Err(refusal) => Verdict {
            accepted: false,
            rejection: Some(refusal.code.text().to_owned()),
            work: 0,
        },
    };
    Ok(report(
        Provenance::Replay,
        &model,
        // A replay has a trace and a certificate and no reduction chain, so
        // there is nothing to name the reduced model's columns from and the
        // page is told so rather than handed the file's labels.
        None,
        &recorded,
        Some(proof),
        Some(&verdict),
    ))
}

/// Rechecks a certificate against an instance and reports only what the
/// verifier said.
///
/// The thin wrapper the specification asks for: no solver state reaches it,
/// nothing is assembled, and the answer is the verifier's own.
///
/// # Errors
///
/// Returns the message to show when the instance cannot be read.
pub fn recheck(proof: &[u8], raw: &[u8]) -> String {
    let mut out = String::from("{\n");
    match verify(proof, raw) {
        Ok(verdict) => {
            let _ = write!(
                out,
                "  \"accepted\": true,\n  \"work\": {},\n  \"bytes\": {},\n  \"widest_bits\": {}\n",
                verdict.work, verdict.bytes, verdict.peak_bits
            );
        }
        Err(refusal) => {
            let _ = write!(
                out,
                "  \"accepted\": false,\n  \"rejection\": \"{}\",\n  \"detail\": {}\n",
                refusal.code.text(),
                quoted(&refusal.to_string())
            );
        }
    }
    out.push('}');
    out
}

/// The result both paths return.
fn report(
    how: Provenance,
    model: &Model,
    reduced: Option<&Model>,
    trace: &Trace,
    proof: Option<&[u8]>,
    verdict: Option<&Verdict>,
) -> String {
    let mut out = String::from("{\n");
    let _ = writeln!(out, "  \"provenance\": \"{}\",", how.as_str());
    let _ = writeln!(
        out,
        "  \"instance\": {{\"digest\": \"{}\", \"format\": {}, \"rows\": {}, \"columns\": {}, \"nonzeros\": {}}},",
        hc_core::hex(model.digest()),
        model.format().byte(),
        model.rows().len(),
        model.variables().len(),
        model.matrix().nonzeros()
    );
    // The reduced model's column labels. A branch event and a branch record
    // both cite a column of *that* model, not of the file, because that is the
    // model the search was about — so a page naming those columns has to be
    // given these rather than the file's, which presolve may have removed
    // columns from. A replay has no chain to hand and gets none, and the page
    // falls back to the index rather than to a name that might be wrong.
    let _ = writeln!(
        out,
        "  \"reduced\": {},",
        reduced.map_or_else(
            || "null".to_owned(),
            |reduced| {
                let mut names = String::from("{\"columns\": [");
                for (at, variable) in reduced.variables().iter().enumerate() {
                    if at > 0 {
                        names.push_str(", ");
                    }
                    names.push_str(&quoted(&variable.name));
                }
                names.push_str("]}");
                names
            }
        )
    );
    let _ = writeln!(
        out,
        "  \"certificate\": {},",
        proof.map_or_else(
            || "null".to_owned(),
            |bytes| format!(
                "{{\"bytes\": {}, \"hex\": \"{}\"}}",
                bytes.len(),
                hex(bytes)
            )
        )
    );
    let _ = writeln!(
        out,
        "  \"verdict\": {},",
        verdict.map_or_else(
            || "null".to_owned(),
            |verdict| format!(
                "{{\"accepted\": {}, \"rejection\": {}, \"work\": {}}}",
                verdict.accepted,
                verdict
                    .rejection
                    .as_deref()
                    .map_or_else(|| "null".to_owned(), quoted),
                verdict.work
            )
        )
    );
    let rendered = trace
        .render()
        .unwrap_or_else(|error| format!("{{\"error\": {}}}", quoted(&error.to_string())));
    let _ = write!(out, "  \"trace\": {}", indent(&rendered));
    out.push_str("\n}");
    out
}

/// A string as a JSON string.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Bytes as lower-case hexadecimal.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Indents a nested document so the result reads as one.
fn indent(text: &str) -> String {
    text.trim_end().lines().collect::<Vec<_>>().join("\n  ")
}

/// The most iterations the glass keeps.
///
/// A display with room for a few hundred rows asking for a million records
/// would be a page that stops responding while it fills an array nobody
/// scrolls. The count of what happened is reported separately, so a run that
/// outran this says so rather than looking short.
pub const GLASS_CAP: usize = 512;

/// Runs the model's own linear relaxation under the textbook rule and reports
/// every pivot.
///
/// The rule is Dantzig because this is the one panel about *how a simplex
/// moves*, and the reference-weighted rule the solver runs by default moves in
/// a way that is only explicable by reference to weights nobody can see. The
/// choice is stated in the answer rather than assumed by the page.
///
/// The model is the file's, not the reduced one the tree search works on, and
/// the answer says so. Presolve would rename and remove the columns this panel
/// exists to name, and a relaxation of a model a reader cannot see is a window
/// onto nothing. It is therefore the file's relaxation rather than the search's
/// root node, and the two coincide only when presolve found nothing to do.
///
/// The problem is scaled, exactly as every solve in this project is, so the
/// step lengths are in the search's own coordinates rather than the file's.
/// Objective values are unaffected: scaling divides a column and multiplies its
/// cost by the same amount.
///
/// Nothing here is a claim and the answer says so by construction: every
/// number in it is a float out of the search, there is no certificate, and no
/// exact value appears anywhere in the document.
///
/// # Errors
///
/// Returns the message to show when the instance cannot be read.
pub fn pivots(raw: &[u8], format: u8, limit: usize) -> Result<String, String> {
    let model = read(raw, format)?;
    let working = Working::from_model(&model);
    let problem = Problem::scaled(&working, Tol::solver());
    let mut trail = Trail::of(limit.min(GLASS_CAP));
    let (outcome, effort) = simplex::solve_watched(
        &problem,
        Basis::logical(&problem),
        &simplex::Options {
            rule: Rule::Dantzig,
            ..simplex::Options::default()
        },
        &mut trail,
    );

    let mut out = String::from(
        "{\n  \"rule\": \"dantzig\",\n  \"model\": \"the file's, before any reduction\",\n  \"scaled\": true,\n",
    );
    let _ = writeln!(out, "  \"outcome\": \"{}\",", outcome.as_str());
    let _ = writeln!(
        out,
        "  \"iterations\": {},\n  \"kept\": {},\n  \"truncated\": {},",
        trail.seen(),
        trail.pivots().len(),
        trail.truncated()
    );
    let _ = writeln!(
        out,
        "  \"flips\": {},\n  \"degenerate\": {},\n  \"refactorizations\": {},\n  \"escalations\": {},",
        effort.flips, effort.degenerate, effort.refactorizations, effort.escalations
    );
    out.push_str("  \"columns\": [");
    for index in 0..problem.columns() {
        if index > 0 {
            out.push(',');
        }
        let (name, kind, integral) = match working.kind(index) {
            Kind::Structural(variable) => {
                let column = &model.variables()[variable.index()];
                (column.name.as_str(), "structural", column.integral)
            }
            Kind::Logical(row) => (model.rows()[row.index()].name.as_str(), "logical", false),
        };
        let _ = write!(
            out,
            "\n    {{\"name\": {}, \"kind\": \"{kind}\", \"integral\": {integral}}}",
            quoted(name)
        );
    }
    out.push_str("\n  ],\n  \"pivots\": [");
    for (at, pivot) in trail.pivots().iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "\n    {{\"iteration\": {}, \"phase\": {}, \"entering\": {}, \"direction\": {}, \"leaving\": {}, \"theta\": {}, \"objective\": {}, \"infeasibility\": {}, \"rung\": \"{}\"}}",
            pivot.iteration,
            pivot.phase,
            pivot.entering,
            pivot.direction,
            pivot
                .leaving
                .map_or_else(|| "null".to_owned(), |column| column.to_string()),
            number(pivot.theta),
            number(pivot.objective),
            number(pivot.infeasibility),
            pivot.rung.as_str()
        );
    }
    out.push_str("\n  ]\n}");
    Ok(out)
}

/// Reads a certificate the way the verifier reads it, and reports the
/// structure alongside the verdict.
///
/// The structure comes from the verifier's own reader rather than from the
/// assembler, which is the difference between a viewer and a rendering of what
/// the solver meant to say. A certificate that does not read at all has no
/// structure to show and comes back as a refusal with the rule that stopped
/// it; one that reads and then fails a rule comes back with both, because the
/// tamper lab exists to show exactly that pair.
pub fn outline(proof: &[u8], raw: &[u8]) -> String {
    let mut out = String::from("{\n");
    match read_proof(proof, raw) {
        Err(refusal) => {
            let _ = write!(
                out,
                "  \"read\": false,\n  \"accepted\": false,\n  \"rejection\": \"{}\",\n  \"detail\": {}\n",
                refusal.code.text(),
                quoted(&refusal.to_string())
            );
        }
        Ok(document) => {
            let _ = writeln!(
                out,
                "  \"read\": true,\n  \"claim\": \"{}\",\n  \"tag\": {},\n  \"bytes\": {},",
                document.claim.word(),
                document.tag,
                document.bytes
            );
            match verify(proof, raw) {
                Ok(verdict) => {
                    let _ = writeln!(
                        out,
                        "  \"accepted\": true,\n  \"rejection\": null,\n  \"work\": {},\n  \"widest_bits\": {},",
                        verdict.work, verdict.peak_bits
                    );
                    let _ = writeln!(
                        out,
                        "  \"objective\": {},\n  \"lower\": {},\n  \"gap\": {},",
                        exact(verdict.objective.as_ref()),
                        exact(verdict.lower.as_ref()),
                        exact(verdict.gap.as_ref())
                    );
                }
                Err(refusal) => {
                    let _ = writeln!(
                        out,
                        "  \"accepted\": false,\n  \"rejection\": \"{}\",\n  \"detail\": {},",
                        refusal.code.text(),
                        quoted(&refusal.to_string())
                    );
                }
            }
            out.push_str("  \"steps\": [");
            for (id, step) in document.steps.iter().enumerate() {
                if id > 0 {
                    out.push(',');
                }
                let span = document.spans.get(id).copied().unwrap_or((0, 0));
                let _ = write!(out, "\n    {}", step_of(id, step, span));
            }
            out.push_str("\n  ]\n");
        }
    }
    out.push('}');
    out
}

/// One record, as the page draws it.
fn step_of(id: usize, step: &Step, span: (usize, usize)) -> String {
    let mut out = format!(
        "{{\"id\": {id}, \"code\": {}, \"kind\": \"{}\", \"at\": {}, \"length\": {}",
        step.code(),
        kind_of(step),
        span.0,
        span.1.saturating_sub(span.0)
    );
    match step {
        Step::Transform { rule, .. } | Step::TransformContradiction { rule, .. } => {
            let _ = write!(out, ", \"rule\": {rule}");
        }
        Step::Root { node, .. } => {
            let _ = write!(out, ", \"node\": {node}");
        }
        Step::DualBound { node, terms } | Step::Contradiction { node, terms } => {
            let _ = write!(out, ", \"node\": {node}, \"terms\": {}", terms.len());
        }
        Step::GmiCut {
            scope,
            stated_coefficients,
            ..
        } => {
            let _ = write!(
                out,
                ", \"scope\": {scope}, \"family\": \"gomory\", \"width\": {}",
                stated_coefficients.len()
            );
        }
        Step::CoverCut {
            scope,
            cover,
            stated_coefficients,
            ..
        } => {
            let _ = write!(
                out,
                ", \"scope\": {scope}, \"family\": \"cover\", \"cover\": {}, \"width\": {}",
                cover.len(),
                stated_coefficients.len()
            );
        }
        Step::Branch {
            parent,
            variable,
            split,
            left,
            right,
        } => {
            let _ = write!(
                out,
                ", \"node\": {parent}, \"variable\": {variable}, \"split\": \"{split}\", \"left\": {left}, \"right\": {right}"
            );
        }
        Step::LeafBound { node, bound } | Step::LeafFrontier { node, bound } => {
            let _ = write!(out, ", \"node\": {node}, \"cites\": {bound}");
        }
        Step::LeafInfeas {
            node,
            contradiction,
        } => {
            let _ = write!(out, ", \"node\": {node}, \"cites\": {contradiction}");
        }
        Step::LeafIntegral { node, bound, point } => {
            let _ = write!(
                out,
                ", \"node\": {node}, \"cites\": {bound}, \"width\": {}",
                point.len()
            );
        }
        Step::Incumbent {
            stated_objective,
            original,
            ..
        } => {
            let _ = write!(
                out,
                ", \"stated\": \"{stated_objective}\", \"width\": {}",
                original.len()
            );
        }
        Step::FeasiblePoint {
            stated_objective,
            point,
        } => {
            let _ = write!(
                out,
                ", \"stated\": \"{stated_objective}\", \"width\": {}",
                point.len()
            );
        }
        Step::Ray {
            direction, step, ..
        } => {
            let _ = write!(
                out,
                ", \"width\": {}, \"step\": {}",
                direction.len(),
                step.as_ref()
                    .map_or_else(|| "null".to_owned(), |value| format!("\"{value}\""))
            );
        }
        Step::Assertion {
            kind,
            stated_objective,
            stated_lower,
            stated_gap,
        } => {
            let _ = write!(
                out,
                ", \"claim\": \"{}\", \"stated\": {}, \"lower\": {}, \"gap\": {}",
                kind.word(),
                exact(stated_objective.as_ref()),
                exact(stated_lower.as_ref()),
                exact(stated_gap.as_ref())
            );
        }
        Step::Terminator { counts } => {
            let _ = write!(out, ", \"kinds\": {}", counts.len());
        }
    }
    out.push('}');
    out
}

/// The word the page shows for a record kind.
const fn kind_of(step: &Step) -> &'static str {
    match step {
        Step::Transform { .. } => "transform",
        Step::TransformContradiction { .. } => "transform_contradiction",
        Step::Root { .. } => "root",
        Step::DualBound { .. } => "dual_bound",
        Step::Contradiction { .. } => "contradiction",
        Step::GmiCut { .. } | Step::CoverCut { .. } => "cut",
        Step::Branch { .. } => "branch",
        Step::LeafBound { .. } => "leaf_bound",
        Step::LeafInfeas { .. } => "leaf_infeasible",
        Step::LeafIntegral { .. } => "leaf_integral",
        Step::LeafFrontier { .. } => "leaf_frontier",
        Step::Incumbent { .. } => "incumbent",
        Step::FeasiblePoint { .. } => "feasible_point",
        Step::Ray { .. } => "ray",
        Step::Assertion { .. } => "assertion",
        Step::Terminator { .. } => "terminator",
    }
}

/// An exact quantity as the page receives it: a string, or nothing.
///
/// A string rather than a number, everywhere, for the reason the trace schema
/// gives — a consumer that renders an exact value as a float has to do it on
/// purpose.
fn exact(value: Option<&hc_verify::vrat::Frac>) -> String {
    value.map_or_else(|| "null".to_owned(), |exact| format!("\"{exact}\""))
}

/// A float as JSON, with the values JSON has no spelling for written as
/// nothing rather than as text a parser would refuse.
fn number(value: f64) -> String {
    if value.is_finite() {
        let mut text = format!("{value}");
        if !text.contains(['.', 'e', 'E']) {
            text.push_str(".0");
        }
        text
    } else {
        "null".to_owned()
    }
}

/// The tamper corpus, as a page can offer it.
///
/// Names and expected rejections only. What each entry does is stated in the
/// corpus manifest, which is a document rather than a string table, and a page
/// that reproduced those sentences would have a second copy of them to keep
/// current.
#[must_use]
pub fn tamper_cases() -> String {
    let mut out = String::from("[");
    for (at, case) in CASES.iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "\n  {{\"name\": \"{}\", \"expect\": \"{}\", \"resealed\": {}, \"reattaches\": {}}}",
            case.name,
            case.expect,
            case.resealed,
            case.name == "reattached_certificate"
        );
    }
    out.push_str("\n]");
    out
}

/// Applies a named mutation to a certificate and reports what the verifier
/// then says about it.
///
/// The forged bytes come back too. A lab that only reported a verdict would be
/// asking to be believed about the edit, which is the posture this whole
/// project is against: the page hands the same bytes to the verifier wrapper
/// itself if it likes.
///
/// The reattachment case is the one entry that changes the instance rather
/// than the certificate, so a second file is taken and used where that case
/// asks for it.
#[must_use]
pub fn tampered(name: &str, proof: &[u8], raw: &[u8], other: &[u8]) -> String {
    let Some(case) = CASES.iter().find(|case| case.name == name) else {
        return format!(
            "{{\n  \"applied\": false,\n  \"why\": {}\n}}",
            quoted(&format!("no mutation is named {name}"))
        );
    };
    if case.name == "reattached_certificate" && other.is_empty() {
        return format!(
            "{{\n  \"case\": \"{}\",\n  \"expect\": \"{}\",\n  \"applied\": false,\n  \"why\": \"this mutation needs a second instance to reattach the certificate to\"\n}}",
            case.name, case.expect
        );
    }
    let Ok(forged) = forge(case.name, proof, raw) else {
        return format!(
            "{{\n  \"case\": \"{}\",\n  \"expect\": \"{}\",\n  \"applied\": false,\n  \"why\": \"this certificate has nothing of the shape the mutation edits\"\n}}",
            case.name, case.expect
        );
    };
    let against = instance_for(case.name, raw, other);
    let mut out = format!(
        "{{\n  \"case\": \"{}\",\n  \"expect\": \"{}\",\n  \"resealed\": {},\n  \"applied\": true,\n  \"forged\": \"{}\",\n  \"outline\": ",
        case.name,
        case.expect,
        case.resealed,
        hex(&forged)
    );
    out.push_str(&indent(&outline(&forged, against)));
    out.push_str("\n}");
    out
}

pub mod exports;

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

    #[test]
    fn a_live_solve_reports_the_run_it_performed() {
        let out = solve_live(TINY, 1, 100).expect("a readable instance");
        assert!(out.contains("\"provenance\": \"live\""));
        assert!(out.contains("\"accepted\": true"));
        assert!(out.contains("\"kind\": \"terminal\""));
        // The float and the exact halves of the trace survive into the page's
        // document as a number and a string, which is the distinction every
        // panel depends on.
        assert!(out.contains("\"objective\": -4.0"));
        assert!(out.contains("\"objective\": \"-4\""));
    }

    #[test]
    fn a_live_report_names_the_reduced_model_columns_and_a_replay_does_not() {
        let live = solve_live(TINY, 1, 100).expect("a readable instance");
        assert!(live.contains("\"reduced\": {\"columns\": ["));

        // A replay has no reduction chain, so it says so rather than handing
        // the page the file's labels for a coordinate system that may not be
        // the file's.
        let model = read(TINY, 1).expect("readable");
        let mut trace = Trace::about(*model.digest());
        trace.push(hc_core::trace::Event::Terminal { status: 0 });
        let written = trace.encode().expect("writable");
        let replayed = replay(&written, &[], TINY, 1).expect("a replay");
        assert!(replayed.contains("\"reduced\": null"));
    }

    #[test]
    fn a_replay_says_it_is_one_and_still_rechecks_the_proof() {
        let live = solve_live(TINY, 1, 100).expect("a readable instance");
        let hex = live
            .split("\"hex\": \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("the certificate came back");
        let proof: Vec<u8> = (0..hex.len() / 2)
            .map(|at| u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16).expect("hex"))
            .collect();

        let model = read(TINY, 1).expect("readable");
        let bound = |bytes: u64, checksum: u64| {
            let mut trace = Trace::about(*model.digest());
            trace.push(hc_core::trace::Event::ProofAssembled { bytes, checksum });
            trace.push(hc_core::trace::Event::Terminal { status: 0 });
            trace.encode().expect("writable")
        };
        let checksum = u64::from(u32::from_le_bytes(
            proof[proof.len() - 4..].try_into().expect("four bytes"),
        ));
        let written = bound(proof.len() as u64, checksum);

        let out = replay(&written, &proof, TINY, 1).expect("a replay");
        assert!(out.contains("\"provenance\": \"replay\""));
        assert!(out.contains("\"accepted\": true"));

        // A trace about another instance is refused rather than drawn.
        let stray = Trace::about([0; 32]).encode().expect("writable");
        assert!(replay(&stray, &proof, TINY, 1).is_err());

        // And so is a trace about this instance but another certificate: two
        // genuine artifacts of two different runs is the easiest facade to
        // assemble here by accident.
        let mismatched = bound(proof.len() as u64, checksum ^ 1);
        assert!(replay(&mismatched, &proof, TINY, 1).is_err());
        let shorter = bound(proof.len() as u64 - 1, checksum);
        assert!(replay(&shorter, &proof, TINY, 1).is_err());
    }

    #[test]
    fn the_verifier_wrapper_answers_on_its_own() {
        let live = solve_live(TINY, 1, 100).expect("a readable instance");
        let hex = live
            .split("\"hex\": \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("the certificate came back");
        let proof: Vec<u8> = (0..hex.len() / 2)
            .map(|at| u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16).expect("hex"))
            .collect();
        assert!(recheck(&proof, TINY).contains("\"accepted\": true"));

        // And a damaged one is refused, with the identifier the page shows.
        let mut damaged = proof.clone();
        let at = damaged.len() / 2;
        damaged[at] = damaged[at].wrapping_add(1);
        let refused = recheck(&damaged, TINY);
        assert!(refused.contains("\"accepted\": false"));
        assert!(refused.contains("\"rejection\": \"HCC-"));
    }

    /// Every instance this surface is ever pointed at, with its format tag.
    ///
    /// The committed corpora and the frozen vectors of all three readers. The
    /// page offers all of them, so all of them are driven through every entry
    /// point here rather than through the one that happened to be tried.
    const EVERY: &[(&str, &[u8], u8)] = &[
        (
            "transport33.mps",
            include_bytes!("../../../instances/local/transport33.mps"),
            1,
        ),
        (
            "blend4.mps",
            include_bytes!("../../../instances/local/blend4.mps"),
            1,
        ),
        (
            "cover6.mps",
            include_bytes!("../../../instances/local/cover6.mps"),
            1,
        ),
        (
            "flow8.mps",
            include_bytes!("../../../instances/local/flow8.mps"),
            1,
        ),
        (
            "freevars5.mps",
            include_bytes!("../../../instances/local/freevars5.mps"),
            1,
        ),
        (
            "cycle7.mps",
            include_bytes!("../../../instances/local/cycle7.mps"),
            1,
        ),
        (
            "infeasible4.mps",
            include_bytes!("../../../instances/local/infeasible4.mps"),
            1,
        ),
        (
            "unbounded3.mps",
            include_bytes!("../../../instances/local/unbounded3.mps"),
            1,
        ),
        (
            "knap9.mps",
            include_bytes!("../../../instances/integer/knap9.mps"),
            2,
        ),
        (
            "fixcharge6.mps",
            include_bytes!("../../../instances/integer/fixcharge6.mps"),
            2,
        ),
        (
            "stock3.mps",
            include_bytes!("../../../instances/integer/stock3.mps"),
            2,
        ),
        (
            "parity5.mps",
            include_bytes!("../../../instances/integer/parity5.mps"),
            2,
        ),
        (
            "tiny.mps",
            include_bytes!("../../../spec/v1/vectors/mps/tiny.mps"),
            1,
        ),
        (
            "ranges.mps",
            include_bytes!("../../../spec/v1/vectors/mps/ranges.mps"),
            1,
        ),
        (
            "exact.mps",
            include_bytes!("../../../spec/v1/vectors/mps/exact.mps"),
            1,
        ),
        (
            "tiny.lp",
            include_bytes!("../../../spec/v1/vectors/lp/tiny.lp"),
            3,
        ),
        (
            "ranges.lp",
            include_bytes!("../../../spec/v1/vectors/lp/ranges.lp"),
            3,
        ),
        (
            "exact.lp",
            include_bytes!("../../../spec/v1/vectors/lp/exact.lp"),
            3,
        ),
        (
            "tiny.json",
            include_bytes!("../../../spec/v1/vectors/json/tiny.json"),
            4,
        ),
        (
            "ranges.json",
            include_bytes!("../../../spec/v1/vectors/json/ranges.json"),
            4,
        ),
        (
            "exact.json",
            include_bytes!("../../../spec/v1/vectors/json/exact.json"),
            4,
        ),
    ];

    /// The hexadecimal certificate a report carries, when it carries one.
    fn proof_of(report: &str) -> Option<Vec<u8>> {
        let hex = report.split("\"hex\": \"").nth(1)?.split('"').next()?;
        Some(
            (0..hex.len() / 2)
                .map(|at| u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16).expect("hex"))
                .collect(),
        )
    }

    #[test]
    fn every_instance_the_page_offers_goes_through_every_entry_point() {
        // Coverage of the surface rather than of one instance. Three of these
        // reach a claim kind the others do not — an infeasibility, a ray, a
        // model with ranged rows — and the panels behind these calls are
        // written against all of them.
        for (name, raw, format) in EVERY {
            let report = solve_live(raw, *format, 500)
                .unwrap_or_else(|error| panic!("{name} did not solve: {error}"));
            assert!(report.contains("\"provenance\": \"live\""), "{name}");
            assert!(report.contains("\"kind\": \"terminal\""), "{name}");
            assert!(report.contains("\"reduced\": {\"columns\":"), "{name}");

            let glass = pivots(raw, *format, 64)
                .unwrap_or_else(|error| panic!("{name} had no glass: {error}"));
            assert!(glass.contains("\"rule\": \"dantzig\""), "{name}");
            assert!(glass.contains("\"pivots\": ["), "{name}");
            // Nothing in the glass is a claim, and that is checkable: an exact
            // quantity is a string everywhere in this project, and the only
            // strings here are the words the schema fixes.
            assert!(!glass.contains("\"stated\""), "{name}");

            if let Some(proof) = proof_of(&report) {
                let drawn = outline(&proof, raw);
                assert!(drawn.contains("\"read\": true"), "{name}");
                assert!(drawn.contains("\"accepted\": true"), "{name}");
                assert!(drawn.contains("\"kind\": \"terminator\""), "{name}");

                // Every record says where its payload sits, and the page lights
                // those bytes. The spans have to be inside the certificate, in
                // order, and not overlap — on every claim kind, not only on the
                // frozen vector the reader's own test uses.
                let mut previous = 0usize;
                for record in drawn.split("{\"id\": ").skip(1) {
                    let field = |key: &str| -> usize {
                        record
                            .split(&format!("\"{key}\": "))
                            .nth(1)
                            .and_then(|rest| rest.split([',', '}']).next())
                            .and_then(|text| text.trim().parse::<usize>().ok())
                            .unwrap_or_else(|| panic!("{name}: a record states no {key}"))
                    };
                    let at = field("at");
                    let length = field("length");
                    assert!(at >= previous, "{name}: record payloads overlap");
                    assert!(
                        at + length <= proof.len(),
                        "{name}: a payload runs past the end"
                    );
                    previous = at + length;
                }

                // And every mutation the corpus knows is either declined or
                // refused. A forgery accepted here is the one result this
                // whole surface exists to make impossible.
                for case in hc_tamper::CASES {
                    let answered = tampered(case.name, &proof, raw, &[]);
                    if answered.contains("\"applied\": true") {
                        assert!(
                            answered.contains("\"accepted\": false"),
                            "{name}: {} was accepted",
                            case.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn an_instance_that_does_not_read_comes_back_as_a_message() {
        assert!(solve_live(b"not an instance\n", 1, 10).is_err());
        assert!(solve_live(TINY, 9, 10).is_err(), "no format has tag nine");
    }
}
