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
use hc_cert::explain::{records, tree};
use hc_cert::mip::{assemble_mip, assemble_presolved};
use hc_cert::trace::{Run as TraceRun, Verdict as TraceVerdict, of as trace_of, status_of};
use hc_core::{Model, Tol};
use hc_io::mps::{Form, read};
use hc_io::{json, lp};
use hc_lp::dual::solve_from as solve_dual;
use hc_lp::{Basis, Effort, Options, Outcome, Problem, Working, solve};
use hc_mip::branching::Branching;
use hc_mip::search::{CUT_ROUNDS, Options as MipOptions, run as search_tree};
use hc_presolve::{Chain, reduce};
use hc_verify::vcli::{accepted, refused};

use crate::args::Shape;
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

/// Reads and canonicalizes an instance under the named format's rules.
///
/// # Errors
///
/// Returns the message to print when the file is not a readable instance.
pub fn parse_instance(raw: &[u8], shape: Shape) -> Result<Model, String> {
    let parsed = match shape {
        Shape::MpsFixed => read(raw, Form::Fixed),
        Shape::MpsFree => read(raw, Form::Free),
        Shape::Lp => lp::read(raw),
        Shape::Json => json::read(raw),
    };
    // The format is named in the refusal, because the commonest way to be
    // refused is to have read a perfectly good file under the wrong rules.
    parsed.map_err(|refusal| format!("{refusal} (read as {})", shape.as_str()))
}

/// `hc check`: the canonical form and nothing else.
#[must_use]
pub fn check(raw: &[u8], shape: Shape) -> (String, Ending) {
    match parse_instance(raw, shape) {
        Ok(model) => (model.canonical_dump(), Ending::Fine),
        Err(message) => (format!("REFUSED: {message}\n"), Ending::Refused),
    }
}

/// What a solve was asked to do.
#[derive(Clone, Debug)]
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
    /// Which format's rules the instance is read under.
    pub shape: Shape,
}

impl Default for Settings {
    /// What the command line does when it is told nothing.
    ///
    /// Written out rather than derived, and that is the whole point of it.
    /// Deriving gives every field its zero, so the derived default was a
    /// solver with presolve and separation switched off — which is not what
    /// this program does when a person runs it, and every caller that reached
    /// for the default was quietly testing a different solver from the one
    /// that ships. The flags below turn things off; this is what they turn off
    /// from.
    fn default() -> Self {
        Self {
            dual: false,
            presolve: true,
            solution: None,
            simple_branching: false,
            cuts: true,
            shape: Shape::default(),
        }
    }
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
    let model = parse_instance(raw, settings.shape)?;
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
        cut_rounds: if settings.cuts { CUT_ROUNDS } else { 0 },
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

/// What `hc explain` was asked to print.
#[derive(Clone, Debug, Default)]
pub struct Explanation {
    /// Which format's rules the instance is read under.
    pub shape: Shape,
    /// Whether to draw the proof tree.
    pub tree: bool,
    /// Whether to list the records the proof is made of.
    pub proof: bool,
    /// Whether the caller asked for the trace of the run.
    pub trace: bool,
}

/// What an explanation produced.
pub struct Explained {
    /// What to print.
    pub report: String,
    /// The trace of the run, when one was asked for.
    pub trace: Option<Vec<u8>>,
    /// How it ended.
    pub ending: Ending,
}

/// `hc explain`: the proof, drawn.
///
/// The tree is rendered from the certificate rather than from the search, and
/// it is rendered only after the verifier has accepted it. An explanation of a
/// proof nobody checked would be an explanation of a hypothesis.
///
/// The trace is the other half and is deliberately not mixed in with it. A
/// proof is what was established; a trace is what happened on the way, most of
/// which established nothing. Printing them together would invite a reader to
/// treat a node the search opened as a step in the argument.
#[must_use]
pub fn explain(instance_name: &str, raw: &[u8], asked: &Explanation) -> Explained {
    let refuse = |report: String| Explained {
        report,
        trace: None,
        ending: Ending::Refused,
    };
    let model = match parse_instance(raw, asked.shape) {
        Ok(model) => model,
        Err(message) => return refuse(format!("REFUSED: {message}\n")),
    };
    let chain = reduce(&model);
    // The same search `solve` performs. An explanation of a different run from
    // the one this program does when asked to solve would be an explanation of
    // nothing, and it was exactly that until a build step replayed the trace
    // beside the certificate and found the two were about different runs.
    let search = search_tree(
        &chain.reduced,
        &MipOptions {
            cut_rounds: CUT_ROUNDS,
            ..MipOptions::default()
        },
    );
    let assembled = match assemble_presolved(&model, &chain, raw, &search) {
        Ok(assembled) => assembled,
        Err(reason) => return refuse(format!("no certificate: {reason}\n")),
    };
    let bytes = assembled.encode();
    let verdict = match verify(&bytes, raw) {
        Err(refusal) => return refuse(refused(&refusal)),
        Ok(verdict) => verdict,
    };

    let mut report = String::new();
    if asked.tree {
        // The reduced model's labels, because a branch record cites one of
        // its columns rather than one of the original file's.
        let names: Vec<String> = chain
            .reduced
            .variables()
            .iter()
            .map(|variable| variable.name.clone())
            .collect();
        report.push_str(&tree(&assembled, &names));
    }
    if asked.proof {
        report.push_str(&records(&assembled));
    }
    let _ = core::fmt::Write::write_fmt(
        &mut report,
        format_args!(
            "verified: {} bytes, {} work units, widest rational {} bits\n",
            verdict.bytes, verdict.work, verdict.peak_bits
        ),
    );
    let _ = core::fmt::Write::write_fmt(&mut report, format_args!("instance: {instance_name}\n"));

    let trace = asked.trace.then(|| {
        let assembled_trace = trace_of(&TraceRun {
            instance: *model.digest(),
            chain: Some(&chain),
            search: &search,
            certificate: Some(&assembled),
            proof: Some(&bytes),
            verdict: Some(TraceVerdict {
                accepted: true,
                rejection: None,
                work: verdict.work,
            }),
            status: status_of(search.ending, search.incumbent.is_some()),
        });
        let encoded = assembled_trace.encode().unwrap_or_default();
        let _ = core::fmt::Write::write_fmt(
            &mut report,
            format_args!(
                "trace: {} event(s), {} bytes\n",
                assembled_trace.events.len(),
                encoded.len()
            ),
        );
        encoded
    });

    Explained {
        report,
        trace,
        ending: Ending::Fine,
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
        let (out, ending) = check(TINY, Shape::MpsFixed);
        assert_eq!(ending, Ending::Fine);
        assert!(out.starts_with("hc-canonical 1\n"));
        assert!(out.ends_with("end\n"));
    }

    #[test]
    fn an_unreadable_instance_is_refused_rather_than_guessed_at() {
        let (out, ending) = check(b"this is not an instance file\n", Shape::MpsFixed);
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

    /// A knapsack whose relaxation is fractional, so the search branches and
    /// the separator has something to find. The frozen tiny instance closes at
    /// the root and would agree about a certificate under any settings at all.
    const KNAPSACK: &[u8] = include_bytes!("../../../instances/integer/knap9.mps");

    #[test]
    fn solving_and_explaining_are_the_same_run() {
        // They were not, and nothing said so: `explain` took the bare defaults
        // and `solve` asked for separation, so the two subcommands produced
        // different certificates for the same file and each was internally
        // consistent. It surfaced when a trace was replayed beside a
        // certificate and the pair turned out to be about different runs.
        let solved = search(
            KNAPSACK,
            &Settings {
                shape: Shape::MpsFree,
                ..Settings::default()
            },
        )
        .expect("the corpus entry parses");
        let (_, bytes, ending) = certify("knap9.mps", KNAPSACK, "knap9.hcc", &solved);
        assert_eq!(ending, Ending::Fine);
        let proof = bytes.expect("a certificate");

        let explained = explain(
            "knap9.mps",
            KNAPSACK,
            &Explanation {
                shape: Shape::MpsFree,
                trace: true,
                ..Explanation::default()
            },
        );
        assert_eq!(explained.ending, Ending::Fine);
        let trace = explained.trace.expect("a trace was asked for");

        // The trace records the length and the trailing checksum of the
        // certificate its own run assembled. If the two subcommands run the
        // same search, those are this certificate's.
        let decoded = hc_core::trace::Trace::decode(&trace).expect("the trace reads");
        let stated = decoded
            .events
            .iter()
            .find_map(|event| match event {
                hc_core::trace::Event::ProofAssembled { bytes, checksum } => {
                    Some((*bytes, *checksum))
                }
                _ => None,
            })
            .expect("the explanation assembled a certificate");
        let carried = (
            proof.len() as u64,
            u64::from(u32::from_le_bytes(
                proof[proof.len() - 4..].try_into().expect("four bytes"),
            )),
        );
        assert_eq!(
            stated, carried,
            "the explanation is of a different run from the solve"
        );
    }

    #[test]
    fn explaining_an_instance_draws_a_tree_the_verifier_accepted() {
        let both = Explanation {
            tree: true,
            proof: true,
            ..Explanation::default()
        };
        let explained = explain("tiny.mps", TINY, &both);
        assert_eq!(explained.ending, Ending::Fine);
        assert!(explained.report.starts_with("node 0  "));
        assert!(explained.report.contains("claim: optimal"));
        assert!(explained.report.contains("verified: "));
        assert!(explained.report.contains("records\n"));
        assert!(explained.report.is_ascii());
        assert!(explained.trace.is_none(), "no trace was asked for");
    }

    #[test]
    fn each_half_of_an_explanation_can_be_asked_for_on_its_own() {
        let only_tree = explain(
            "tiny.mps",
            TINY,
            &Explanation {
                tree: true,
                ..Explanation::default()
            },
        );
        assert!(only_tree.report.starts_with("node 0  "));
        assert!(!only_tree.report.contains("records\n"));

        let only_proof = explain(
            "tiny.mps",
            TINY,
            &Explanation {
                proof: true,
                ..Explanation::default()
            },
        );
        assert!(only_proof.report.starts_with("records\n"));
        assert!(!only_proof.report.contains("node 0  "));
        // The record listing names what the tree drew: a root, a bound, the
        // incumbent that attains it, and the claim.
        assert!(only_proof.report.contains("root               node 0"));
        assert!(only_proof.report.contains("dual-bound"));
        assert!(only_proof.report.contains("claim              kind 1"));
    }

    #[test]
    fn a_traced_explanation_writes_the_trace_of_that_run() {
        let explained = explain(
            "tiny.mps",
            TINY,
            &Explanation {
                tree: true,
                trace: true,
                ..Explanation::default()
            },
        );
        let written = explained.trace.expect("a trace was asked for");
        let read = hc_core::trace::Trace::decode(&written).expect("a readable trace");
        assert!(!read.events.is_empty());
        assert!(explained.report.contains("trace: "));
        // The last event says how the run ended, and the report says the
        // verifier accepted it. Those are the same run.
        assert!(matches!(
            read.events.last(),
            Some(hc_core::trace::Event::Terminal { status: 0 })
        ));
        assert!(explained.report.contains("verified: "));
    }

    #[test]
    fn explaining_something_that_is_not_an_instance_refuses() {
        let explained = explain("junk", b"not an instance\n", &Explanation::default());
        let (out, ending) = (explained.report, explained.ending);
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
