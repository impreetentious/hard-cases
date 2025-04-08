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
use hc_cert::mip::{assemble_mip, assemble_presolved};
use hc_core::{Model, Tol};
use hc_io::mps::{Form, read};
use hc_lp::dual::solve_from as solve_dual;
use hc_lp::{Basis, Effort, Options, Outcome, Problem, Working, solve};
use hc_mip::branching::Branching;
use hc_mip::search::{Options as MipOptions, run as search_tree};
use hc_presolve::{Chain, reduce};
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

/// What a solve was asked to do.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    /// Whether to try the dual method.
    pub dual: bool,
    /// Whether to reduce the model first.
    pub presolve: bool,
    /// A candidate solution over the model's variables.
    pub solution: Option<Vec<hc_exact::Rational>>,
    /// Whether to branch by the textbook rule.
    pub simple_branching: bool,
    /// Whether to separate cuts.
    pub cuts: bool,
}

/// Reads a solution file: one exact decimal value per line, in variable order.
///
/// Blank lines and lines beginning with a number sign are ignored, so a file
/// can carry a note about where it came from. Nothing here decides whether the
/// point is any good; the search screens it exactly.
///
/// # Errors
///
/// Returns the message to print when a line is not a number.
pub fn read_solution(text: &str) -> Result<Vec<hc_exact::Rational>, String> {
    let mut out = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let body = line.split('#').next().unwrap_or_default().trim();
        if body.is_empty() {
            continue;
        }
        let value = hc_exact::Rational::parse_decimal(body)
            .map_err(|error| format!("line {}: {error}", number + 1))?;
        out.push(value);
    }
    Ok(out)
}

/// The outcome of a solve, before anything is claimed about it.
pub struct Solved {
    /// The parsed model.
    pub model: Model,
    /// The reduction chain, when presolve ran.
    pub chain: Option<Chain>,
    /// The tree search over the reduced model.
    pub tree: hc_mip::search::Search,
    /// The linear outcome on the original model, for the fallback path.
    pub outcome: Outcome,
    /// What the linear search spent.
    pub effort: Effort,
    /// Which method actually ran.
    ///
    /// Reported rather than assumed. The dual method declines to start when
    /// the logical basis does not price correctly, and a run that quietly fell
    /// back to the primal while the caller believed otherwise would be a run
    /// nobody could reason about.
    pub method: &'static str,
}

/// Runs the search. No claim is made here.
///
/// # Errors
///
/// Returns the message to print when the file is not a readable instance.
pub fn search(raw: &[u8], settings: &Settings) -> Result<Solved, String> {
    let model = parse_instance(raw)?;
    let chain = settings.presolve.then(|| reduce(&model));
    let reduced = chain.as_ref().map_or(&model, |chain| &chain.reduced);

    // A supplied solution is over the reduced model's variables, because that
    // is the model the search is about. It is screened inside the search, like
    // every other candidate.
    let start = settings
        .solution
        .as_ref()
        .and_then(|values| hc_mip::extend(&hc_lp::exact::Canonical::of(reduced), values));
    let options = MipOptions {
        cut_rounds: if settings.cuts { 4 } else { 0 },
        start,
        branching: if settings.simple_branching {
            Branching::MostFractional
        } else {
            Branching::ReliabilityPseudocost
        },
        ..MipOptions::default()
    };
    let tree = search_tree(reduced, &options);
    let dual = settings.dual;

    let working = Working::from_model(&model);
    let problem = Problem::scaled(&working, Tol::solver());

    let attempt = if dual {
        solve_dual(&problem, Basis::logical(&problem), &Options::default()).ok()
    } else {
        None
    };
    let (outcome, effort, method) = match attempt {
        Some((outcome, effort)) => (outcome, effort, "dual"),
        None => {
            let (outcome, effort) = solve(&problem, &Options::default());
            (
                outcome,
                effort,
                if dual { "dual-declined" } else { "primal" },
            )
        }
    };
    Ok(Solved {
        model,
        chain,
        tree,
        outcome,
        effort,
        method,
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
        "{}\nmethod       {}\nstatus       candidate-{} (uncertified)\n",
        effort_line(&solved.effort),
        solved.method,
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
    // The tree path first, because it is the general one: a model with no
    // integer column produces a one-node tree and the same statement. The
    // linear path is the fallback, and today it is what certifies an
    // unbounded relaxation, which a mixed-integer claim cannot use.
    let assembled = match &solved.chain {
        Some(chain) => assemble_presolved(&solved.model, chain, raw, &solved.tree),
        None => assemble_mip(&solved.model, raw, &solved.tree),
    }
    .or_else(|_| assemble_lp(&solved.model, raw, &solved.outcome));

    let assembled = match assembled {
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
            out.push_str("method       ");
            out.push_str(solved.method);
            out.push('\n');
            // What the certificate carries, not what presolve did. A run may
            // reduce a model and then certify by a route that carries no
            // chain, and reporting the reduction there would describe a proof
            // the file does not contain.
            let replayed = assembled
                .records
                .iter()
                .filter(|record| {
                    matches!(
                        record,
                        hc_cert::hcc::Record::Transform { .. }
                            | hc_cert::hcc::Record::TransformContradiction { .. }
                    )
                })
                .count();
            let _ = core::fmt::Write::write_fmt(
                &mut out,
                format_args!("presolve     {replayed} transform(s) in the certificate\n"),
            );
            let cuts = assembled
                .records
                .iter()
                .filter(|record| {
                    matches!(
                        record,
                        hc_cert::hcc::Record::GmiCut { .. } | hc_cert::hcc::Record::CoverCut { .. }
                    )
                })
                .count();
            let _ = core::fmt::Write::write_fmt(
                &mut out,
                format_args!("cuts         {cuts} in the certificate\n"),
            );
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
    let chain = reduce(&model);
    let search = search_tree(&chain.reduced, &MipOptions::default());
    let assembled = match assemble_presolved(&model, &chain, raw, &search) {
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
        let solved = search(
            TINY,
            &Settings {
                presolve: true,
                ..Settings::default()
            },
        )
        .expect("the frozen instance parses");
        let (out, bytes, ending) = certify("tiny.mps", TINY, "tiny.hcc", &solved);
        assert_eq!(ending, Ending::Fine);
        assert!(bytes.is_some());
        assert!(out.contains("CERTIFIED OPTIMAL (exact arithmetic)"));
        assert!(out.contains("effort       iters="));
    }

    #[test]
    fn refusing_to_certify_produces_a_candidate_that_says_it_is_one() {
        let solved = search(
            TINY,
            &Settings {
                presolve: true,
                ..Settings::default()
            },
        )
        .expect("the frozen instance parses");
        let out = candidate(&solved);
        assert!(out.contains("candidate-"));
        assert!(out.contains("(uncertified)"));
        assert!(!out.contains("CERTIFIED"));
    }

    #[test]
    fn the_wrapper_prints_exactly_what_the_library_renders() {
        let solved = search(
            TINY,
            &Settings {
                presolve: true,
                ..Settings::default()
            },
        )
        .expect("the frozen instance parses");
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
    fn a_solution_file_is_read_as_exact_decimals_with_notes_ignored() {
        let text = "# where this came from\n4\n\n0.25 # a quarter\n-3\n";
        let values = read_solution(text).expect("the file reads");
        let rendered: Vec<String> = values
            .iter()
            .map(hc_exact::Rational::to_canonical_string)
            .collect();
        assert_eq!(rendered, vec!["4", "1/4", "-3"]);

        assert!(read_solution("4\nnot a number\n").is_err());
    }

    #[test]
    fn a_supplied_solution_is_screened_rather_than_believed() {
        // The frozen instance allows x up to four; nine is outside it.
        let settings = Settings {
            presolve: false,
            solution: Some(vec![hc_exact::Rational::from_i64(9)]),
            ..Settings::default()
        };
        let solved = search(TINY, &settings).expect("the frozen instance parses");
        let (out, _, ending) = certify("tiny.mps", TINY, "tiny.hcc", &solved);
        assert_eq!(ending, Ending::Fine);
        assert!(
            out.contains("U = -4"),
            "the search should reach its own answer"
        );
    }

    #[test]
    fn the_dual_method_reports_whether_it_actually_ran() {
        let plain = search(
            TINY,
            &Settings {
                presolve: true,
                ..Settings::default()
            },
        )
        .expect("the frozen instance parses");
        assert_eq!(plain.method, "primal");

        let asked = search(
            TINY,
            &Settings {
                dual: true,
                presolve: true,
                ..Settings::default()
            },
        )
        .expect("the frozen instance parses");
        assert!(
            asked.method == "dual" || asked.method == "dual-declined",
            "a run that asked for the dual reported {}",
            asked.method
        );
        // Whichever ran, the answer is the same one.
        assert_eq!(plain.outcome.as_str(), asked.outcome.as_str());
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
        let solved = search(
            TINY,
            &Settings {
                presolve: true,
                ..Settings::default()
            },
        )
        .expect("the frozen instance parses");
        let (_, bytes, _) = certify("tiny.mps", TINY, "tiny.hcc", &solved);
        let bytes = bytes.expect("a certificate");
        let mut other = TINY.to_vec();
        other.push(b'\n');
        let (out, ending) = recheck("tiny.hcc", &bytes, "other.mps", &other);
        assert_eq!(ending, Ending::Refused);
        assert!(out.starts_with("REJECTED: HCC-PARSE-03"));
    }
}
