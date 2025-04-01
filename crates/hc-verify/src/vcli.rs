//! The command-line surface: what a verification is asked for, and what it
//! prints.
//!
//! The rendering lives in the library rather than in the binary for one
//! reason: there are two front doors to this crate — the standalone executable
//! and the solver command line's wrapper — and the specification requires them
//! to produce byte-for-byte identical output. Two formatters would satisfy
//! that requirement on the day they were written and stop satisfying it later.
//!
//! Every line the report prints corresponds to a check that actually ran. A
//! report that listed the transform chain and the branch cover on a
//! certificate that has neither would be describing a verification that did not
//! happen, and the whole value of the output is that a reader can take it
//! literally.
//!
//! The argument grammar is here for the same reason as the rendering. It is
//! the one piece of the executable that can be got wrong quietly — an option
//! misread as a path, a missing operand treated as a default — and putting it
//! in the binary would put it outside every harness in the project.

use core::fmt::Write as _;

use crate::vclose::Verdict;
use crate::vhcc::Claim;
use crate::vreject::Rejected;

/// The label a claim is announced under on acceptance.
#[must_use]
pub const fn headline(claim: Claim) -> &'static str {
    match claim {
        Claim::Optimal => "CERTIFIED OPTIMAL (exact arithmetic)",
        Claim::Infeasible => "CERTIFIED INFEASIBLE (exact arithmetic)",
        Claim::Unbounded => "CERTIFIED UNBOUNDED (exact arithmetic)",
        Claim::Gap => "CERTIFIED FEASIBLE WITH AN EXACT GAP",
        Claim::PointOnly => "CERTIFIED FEASIBLE, NO BOUND CLAIMED",
    }
}

/// Renders an accepted verification.
///
/// The instance and certificate names are the caller's to supply, because the
/// library never learns them: it is handed bytes.
#[must_use]
pub fn accepted(
    instance: &str,
    instance_bytes: usize,
    certificate: &str,
    verdict: &Verdict,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "instance     {instance}  {instance_bytes} bytes");
    let _ = writeln!(out, "certificate  {certificate}  {} bytes", verdict.bytes);
    let _ = writeln!(out, "binding      ok");
    let _ = writeln!(out, "parse        ok  reparsed from the original bytes");
    let _ = writeln!(
        out,
        "model        ok  {}",
        crate::vhash::to_hex(&verdict.model)
    );
    if let Some(lower) = &verdict.lower {
        let _ = writeln!(out, "bound        ok  L = {lower}");
    }
    if let Some(objective) = &verdict.objective {
        let _ = writeln!(out, "witness      ok  U = {objective}");
    }
    if let Some(gap) = &verdict.gap {
        let _ = writeln!(out, "gap          ok  {gap}");
    }
    let _ = writeln!(out, "work         {} units (upper bound)", verdict.work);
    let _ = writeln!(out, "widest       {} bits", verdict.peak_bits);
    let _ = writeln!(out, "{}", headline(verdict.claim));
    out
}

/// Renders a refusal.
///
/// One line, naming the rule and what it saw. A refusal that only said no
/// would be a refusal nobody can argue with, and being able to argue with it
/// is what turns a disagreement between the two implementations into a report
/// with an address.
#[must_use]
pub fn refused(refusal: &Rejected) -> String {
    format!("REJECTED: {refusal}\n")
}

/// What the executable was asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The certificate to check.
    pub certificate: String,
    /// The instance to check it against.
    pub instance: String,
}

/// The usage text, printed when the arguments do not parse.
pub const USAGE: &str = "\
usage: hc-verify <certificate> --instance <file>
       hc-verify --version

Rechecks a certificate against the original problem file. Exits 0 when the
proof is accepted and 1 when it is refused, naming the rule that refused.
";

/// What the argument list amounts to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Recheck a certificate.
    Recheck(Request),
    /// Print which code is doing the checking.
    Version,
    /// Print the usage text and stop.
    Usage,
}

/// The version line.
///
/// A verifier that could not say which code checked a proof would be asking to
/// be trusted on the strength of a filename. The number comes from the
/// manifest rather than from a string somebody maintains by hand.
#[must_use]
pub fn version() -> String {
    format!("hc-verify {}\n", env!("CARGO_PKG_VERSION"))
}

/// Reads the argument list.
///
/// Deliberately unforgiving. An unknown option is a mistake worth stopping
/// for rather than something to skip past: the one thing a verifier must never
/// do is appear to have checked more than it did, and an argument silently
/// ignored is the cheapest way to arrange that.
#[must_use]
pub fn request(arguments: &[String]) -> Ask {
    if arguments.iter().any(|word| word == "--version") {
        return if arguments.len() == 1 {
            Ask::Version
        } else {
            Ask::Usage
        };
    }

    let mut certificate = None;
    let mut instance = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--instance" => {
                if instance.is_some() {
                    return Ask::Usage;
                }
                let Some(next) = arguments.get(index + 1) else {
                    return Ask::Usage;
                };
                instance = Some(next.clone());
                index += 2;
            }
            other if other.starts_with('-') => return Ask::Usage,
            other if certificate.is_none() => {
                certificate = Some(other.to_owned());
                index += 1;
            }
            _ => return Ask::Usage,
        }
    }
    match (certificate, instance) {
        (Some(certificate), Some(instance)) => Ask::Recheck(Request {
            certificate,
            instance,
        }),
        _ => Ask::Usage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vclose::verify;
    use crate::vhcc::from_annotated_hex;
    use crate::vreject::Code;

    const VECTOR: &str = include_str!("../../../spec/v1/vectors/hcc/tiny-optimal.hex");
    const TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

    #[test]
    fn an_accepted_report_names_only_the_checks_that_ran() {
        let bytes = from_annotated_hex(VECTOR).expect("hexadecimal");
        let verdict = verify(&bytes, TINY).expect("accepted");
        let report = accepted("tiny.mps", TINY.len(), "tiny.hcc", &verdict);

        assert!(report.contains("binding      ok"));
        assert!(report.contains("bound        ok  L = -4"));
        assert!(report.contains("witness      ok  U = -4"));
        assert!(report.contains("widest       "));
        assert!(report.ends_with("CERTIFIED OPTIMAL (exact arithmetic)\n"));
        // No transform chain and no tree were checked, so neither is claimed.
        assert!(!report.contains("transform"));
        assert!(!report.contains("branch"));
        assert!(report.is_ascii());
    }

    #[test]
    fn a_refusal_names_the_rule_and_what_it_saw() {
        let refusal = Rejected::new(Code::DUAL_BOUND_VALUE, "the combination establishes -4");
        assert_eq!(
            refused(&refusal),
            "REJECTED: HCC-DUAL-03: the combination establishes -4\n"
        );
    }

    #[test]
    fn the_argument_grammar_accepts_the_documented_form_and_nothing_else() {
        let words = |list: &[&str]| {
            list.iter()
                .map(|word| (*word).to_owned())
                .collect::<Vec<_>>()
        };
        let expected = Ask::Recheck(Request {
            certificate: "out.hcc".to_owned(),
            instance: "model.mps".to_owned(),
        });

        assert_eq!(
            request(&words(&["out.hcc", "--instance", "model.mps"])),
            expected
        );
        // The order of the two is not significant; the presence of both is.
        assert_eq!(
            request(&words(&["--instance", "model.mps", "out.hcc"])),
            expected
        );

        assert_eq!(request(&words(&["out.hcc"])), Ask::Usage, "no instance");
        assert_eq!(
            request(&words(&["--instance", "m.mps"])),
            Ask::Usage,
            "no certificate"
        );
        assert_eq!(
            request(&words(&["out.hcc", "--instance"])),
            Ask::Usage,
            "no operand"
        );
        assert_eq!(
            request(&words(&["out.hcc", "--lenient", "--instance", "m.mps"])),
            Ask::Usage
        );
        assert_eq!(
            request(&words(&["a.hcc", "b.hcc", "--instance", "m.mps"])),
            Ask::Usage
        );
        assert_eq!(
            request(&words(&[
                "a.hcc",
                "--instance",
                "m.mps",
                "--instance",
                "n.mps"
            ])),
            Ask::Usage,
            "a second instance is a mistake, not an override"
        );
        assert_eq!(request(&[]), Ask::Usage);

        assert_eq!(request(&words(&["--version"])), Ask::Version);
        assert_eq!(request(&words(&["--version", "out.hcc"])), Ask::Usage);
        assert!(version().starts_with("hc-verify "));
        assert!(version().ends_with('\n'));
    }

    #[test]
    fn every_claim_has_a_headline_and_none_of_them_overstates() {
        for claim in [
            Claim::Optimal,
            Claim::Infeasible,
            Claim::Unbounded,
            Claim::Gap,
            Claim::PointOnly,
        ] {
            let line = headline(claim);
            assert!(line.is_ascii());
            assert!(line.starts_with("CERTIFIED"));
        }
        assert!(headline(Claim::PointOnly).contains("NO BOUND"));
    }
}
