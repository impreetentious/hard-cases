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
//! There is no binding generator. The exports are plain functions over a
//! pointer and a length, results are UTF-8 JSON left in this module's memory,
//! and the page reads them out. That costs about forty lines of JavaScript and
//! no dependency, which is the trade this project makes everywhere else.

use core::fmt::Write as _;

use hc_cert::mip::assemble_presolved;
use hc_cert::trace::{Run, Verdict, of as trace_of, status_of};
use hc_core::trace::Trace;
use hc_core::{FormatTag, Model};
use hc_mip::search::{Options, run as search};
use hc_presolve::reduce;
use hc_verify::vclose::verify;

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
        let mut trace = Trace::about(*model.digest());
        trace.push(hc_core::trace::Event::Terminal { status: 0 });
        let written = trace.encode().expect("writable");

        let out = replay(&written, &proof, TINY, 1).expect("a replay");
        assert!(out.contains("\"provenance\": \"replay\""));
        assert!(out.contains("\"accepted\": true"));

        // A trace about another instance is refused rather than drawn.
        let stray = Trace::about([0; 32]).encode().expect("writable");
        assert!(replay(&stray, &proof, TINY, 1).is_err());
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

    #[test]
    fn an_instance_that_does_not_read_comes_back_as_a_message() {
        assert!(solve_live(b"not an instance\n", 1, 10).is_err());
        assert!(solve_live(TINY, 9, 10).is_err(), "no format has tag nine");
    }
}
