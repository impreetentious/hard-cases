//! What each subcommand does.
//!
//! The arrangement worth stating: `solve` never prints a certified status on
//! its own authority. It assembles a certificate, hands the bytes and the
//! original file to the verifier, and reports what the verifier said. A run
//! whose certificate the verifier rejects reports the rejection, not the
//! solver's opinion of the answer.
//!
//! That is what `--no-cert` costs. It exists for search experiments, and its
//! result is labelled a candidate: it is not a terminal status, it does not go
//! on a scoreboard, and the label says so in the output rather than in a
//! footnote somewhere else.

use hc_cert::assemble::assemble_lp;
use hc_cert::explain::tree;
use hc_cert::mip::assemble_mip;
use hc_core::{Model, Tol};
use hc_io::mps::{Form, read};
use hc_lp::{Effort, Options, Outcome, Problem, Working, solve};
use hc_mip::search::{Options as MipOptions, run as search_tree};
use hc_verify::vcli::{accepted, refused};
use hc_verify::vclose::verify;

/// How a run ended, for the process exit code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// Everything asked for happened.
    Fine,
    /// The verifier refused, or the input did not parse.
    Refused,
    /// The command line was wrong.
    Misused,
}

/// Reads and canonicalizes an instance.
///
/// # Errors
///
/// Returns the message to print when the file is not a readable instance.
pub fn parse_instance(raw: &[u8]) -> Result<Model, String> {
    read(raw, Form::Fixed).map_err(|refusal| format!("{refusal}"))
}

/// `hc check`: the canonical form and nothing else.
#[must_use]
pub fn check(raw: &[u8]) -> (String, Ending) {
    match parse_instance(raw) {
        Ok(model) => (model.canonical_dump(), Ending::Fine),
        Err(message) => (format!("REFUSED: {message}\n"), Ending::Refused),
    }
}

/// The outcome of a solve, before anything is claimed about it.
pub struct Solved {
    /// The parsed model.
    pub model: Model,
    /// What the search returned.
    pub outcome: Outcome,
    /// What the search spent.
    pub effort: Effort,
}

/// Runs the search. No claim is made here.
///
/// # Errors
///
/// Returns the message to print when the file is not a readable instance.
pub fn search(raw: &[u8]) -> Result<Solved, String> {
    let model = parse_instance(raw)?;
    let working = Working::from_model(&model);
    let problem = Problem::scaled(&working, Tol::solver());
    let (outcome, effort) = solve(&problem, &Options::default());
    Ok(Solved {
        model,
        outcome,
        effort,
    })
}

/// The effort line, in counts and never in seconds.
#[must_use]
pub fn effort_line(effort: &Effort) -> String {
    format!(
        "effort       iters={} flips={} degenerate={} refactor={} escalations={}",
        effort.iterations,
        effort.flips,
        effort.degenerate,
        effort.refactorizations,
        effort.escalations
    )
}

/// `hc solve --no-cert`: a candidate, labelled as one.
#[must_use]
pub fn candidate(solved: &Solved) -> String {
    format!(
        "{}\nstatus       candidate-{} (uncertified)\n",
        effort_line(&solved.effort),
        solved.outcome.as_str()
    )
}

/// `hc solve`: assemble, then let the verifier decide.
///
/// The certificate bytes come back alongside the report so the caller can
/// write them out; a caller that asked for no file still gets the verification,
/// because the verification is the point and the file is a convenience.
#[must_use]
pub fn certify(
    instance_name: &str,
    raw: &[u8],
    certificate_name: &str,
    solved: &Solved,
) -> (String, Option<Vec<u8>>, Ending) {
    let assembled = match assemble_lp(&solved.model, raw, &solved.outcome) {
        Ok(certificate) => certificate,
        Err(reason) => {
            return (
                format!(
                    "{}\nstatus       error(CertAssembly): {reason}\n",
                    effort_line(&solved.effort)
                ),
                None,
                Ending::Refused,
            );
        }
    };
    let bytes = assembled.encode();

    match verify(&bytes, raw) {
        Ok(verdict) => {
            let mut out = accepted(instance_name, raw.len(), certificate_name, &verdict);
            out.push_str(&effort_line(&solved.effort));
            out.push('\n');
            (out, Some(bytes), Ending::Fine)
        }
        Err(refusal) => {
            // The solver produced something its own checker refuses. That is a
            // solver defect and it is reported as one; the answer the search
            // liked is not printed, because printing it would be printing an
            // unverified claim next to the news that it failed verification.
            let mut out = refused(&refusal);
            out.push_str("status       error(VerifyReject)\n");
            (out, Some(bytes), Ending::Refused)
        }
    }
}

/// `hc explain`: the proof, drawn.
///
/// The tree is rendered from the certificate rather than from the search, and
/// it is rendered only after the verifier has accepted it. An explanation of a
/// proof nobody checked would be an explanation of a hypothesis.
#[must_use]
pub fn explain(instance_name: &str, raw: &[u8]) -> (String, Ending) {
    let model = match parse_instance(raw) {
        Ok(model) => model,
        Err(message) => return (format!("REFUSED: {message}\n"), Ending::Refused),
    };
    let search = search_tree(&model, &MipOptions::default());
    let assembled = match assemble_mip(&model, raw, &search) {
        Ok(assembled) => assembled,
        Err(reason) => {
            return (format!("no certificate: {reason}\n"), Ending::Refused);
        }
    };
    let bytes = assembled.encode();
    match verify(&bytes, raw) {
        Err(refusal) => (refused(&refusal), Ending::Refused),
        Ok(verdict) => {
            let mut out = tree(&assembled);
            let _ = core::fmt::Write::write_fmt(
                &mut out,
                format_args!(
                    "verified: {} bytes, {} work units, widest rational {} bits\n",
                    verdict.bytes, verdict.work, verdict.peak_bits
                ),
            );
            let _ =
                core::fmt::Write::write_fmt(&mut out, format_args!("instance: {instance_name}\n"));
            (out, Ending::Fine)
        }
    }
}

/// `hc verify`: the wrapper, which must print what the standalone binary
/// prints, byte for byte.
#[must_use]
pub fn recheck(
    certificate_name: &str,
    certificate: &[u8],
    instance_name: &str,
    instance: &[u8],
) -> (String, Ending) {
    match verify(certificate, instance) {
        Ok(verdict) => (
            accepted(instance_name, instance.len(), certificate_name, &verdict),
            Ending::Fine,
        ),
        Err(refusal) => (refused(&refusal), Ending::Refused),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

    #[test]
    fn checking_an_instance_prints_its_canonical_form() {
        let (out, ending) = check(TINY);
        assert_eq!(ending, Ending::Fine);
        assert!(out.starts_with("hc-canonical 1\n"));
        assert!(out.ends_with("end\n"));
    }

    #[test]
    fn an_unreadable_instance_is_refused_rather_than_guessed_at() {
        let (out, ending) = check(b"this is not an instance file\n");
        assert_eq!(ending, Ending::Refused);
        assert!(out.starts_with("REFUSED: "));
    }

    #[test]
    fn a_solve_reports_only_what_the_verifier_accepted() {
        let solved = search(TINY).expect("the frozen instance parses");
        let (out, bytes, ending) = certify("tiny.mps", TINY, "tiny.hcc", &solved);
        assert_eq!(ending, Ending::Fine);
        assert!(bytes.is_some());
        assert!(out.contains("CERTIFIED OPTIMAL (exact arithmetic)"));
        assert!(out.contains("effort       iters="));
    }

    #[test]
    fn refusing_to_certify_produces_a_candidate_that_says_it_is_one() {
        let solved = search(TINY).expect("the frozen instance parses");
        let out = candidate(&solved);
        assert!(out.contains("candidate-"));
        assert!(out.contains("(uncertified)"));
        assert!(!out.contains("CERTIFIED"));
    }

    #[test]
    fn the_wrapper_prints_exactly_what_the_library_renders() {
        let solved = search(TINY).expect("the frozen instance parses");
        let (_, bytes, _) = certify("tiny.mps", TINY, "tiny.hcc", &solved);
        let bytes = bytes.expect("a certificate");
        let (wrapper, ending) = recheck("tiny.hcc", &bytes, "tiny.mps", TINY);
        assert_eq!(ending, Ending::Fine);

        let verdict = verify(&bytes, TINY).expect("accepted");
        assert_eq!(
            wrapper,
            accepted("tiny.mps", TINY.len(), "tiny.hcc", &verdict)
        );
    }

    #[test]
    fn explaining_an_instance_draws_a_tree_the_verifier_accepted() {
        let (out, ending) = explain("tiny.mps", TINY);
        assert_eq!(ending, Ending::Fine);
        assert!(out.starts_with("node 0  "));
        assert!(out.contains("claim: optimal"));
        assert!(out.contains("verified: "));
        assert!(out.is_ascii());
    }

    #[test]
    fn explaining_something_that_is_not_an_instance_refuses() {
        let (out, ending) = explain("junk", b"not an instance\n");
        assert_eq!(ending, Ending::Refused);
        assert!(out.starts_with("REFUSED: "));
    }

    #[test]
    fn a_certificate_bound_to_another_instance_is_refused_by_the_wrapper() {
        let solved = search(TINY).expect("the frozen instance parses");
        let (_, bytes, _) = certify("tiny.mps", TINY, "tiny.hcc", &solved);
        let bytes = bytes.expect("a certificate");
        let mut other = TINY.to_vec();
        other.push(b'\n');
        let (out, ending) = recheck("tiny.hcc", &bytes, "other.mps", &other);
        assert_eq!(ending, Ending::Refused);
        assert!(out.starts_with("REJECTED: HCC-PARSE-03"));
    }
}
