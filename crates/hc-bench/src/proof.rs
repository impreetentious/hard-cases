//! The certification campaign: does every claim survive being rechecked?
//!
//! The status campaign beside this one asks whether the fast solver and the
//! slow reference agree. This one asks a different and harder question: of the
//! answers the fast solver is willing to certify, how many survive a checker
//! that shares none of its code and reparses the original file?
//!
//! Three outcomes are counted separately and none of them is hidden.
//!
//! A certificate that the verifier accepts is a claim the project is entitled
//! to make. A certificate the verifier refuses is a solver defect, recorded
//! with the instance and the rule that refused, and it fails the campaign. An
//! outcome that produced no certificate at all is neither — the exact rebuild
//! declined to make a claim it could not support, which is the designed
//! behaviour — and it is counted by reason so that a run where nothing could
//! be certified is visible as such rather than as a clean sheet.
//!
//! That last distinction is the one worth being careful about. A campaign that
//! reported only failures would show a perfect record for a solver that had
//! quietly stopped certifying anything.

use std::collections::BTreeMap;

use hc_cert::assemble::assemble_lp;
use hc_core::{Model, Tol, Xoshiro256PlusPlus};
use hc_lp::{Options, Problem, Working, solve};
use hc_verify::vclose::verify;

use crate::campaign::generate;
use crate::ref_lp;

/// A certificate the verifier refused, with everything needed to reproduce it.
#[derive(Clone, Debug)]
pub struct Refusal {
    /// The round that produced it.
    pub round: usize,
    /// The rejection identifier the verifier answered with.
    pub rule: String,
    /// What the verifier said.
    pub detail: String,
    /// The instance file.
    pub source: String,
}

/// A certified claim that disagrees with the reference solver.
#[derive(Clone, Debug)]
pub struct Discord {
    /// The round that produced it.
    pub round: usize,
    /// What the certificate proved.
    pub certified: String,
    /// What the reference solver found.
    pub reference: String,
    /// The instance file.
    pub source: String,
}

/// What a certification campaign found.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Instances generated.
    pub rounds: usize,
    /// How many certificates of each claim the verifier accepted.
    pub certified: BTreeMap<String, usize>,
    /// Why assembly declined, and how often.
    pub declined: BTreeMap<String, usize>,
    /// Certificates the verifier refused. Any of these fails the campaign.
    pub refusals: Vec<Refusal>,
    /// Accepted claims that contradict the reference solver. Also failures.
    pub discords: Vec<Discord>,
    /// The largest certificate produced, in bytes.
    pub widest_certificate: usize,
    /// The largest rational any verification handled, in bits.
    pub widest_rational: u64,
    /// Verifier work units spent across the whole campaign.
    pub work: u64,
}

impl Report {
    /// Whether the campaign found nothing wrong.
    #[must_use]
    pub fn clean(&self) -> bool {
        self.refusals.is_empty() && self.discords.is_empty()
    }

    /// How many certificates were accepted in total.
    #[must_use]
    pub fn accepted(&self) -> usize {
        self.certified.values().sum()
    }

    /// A one-paragraph account, for a failure message worth reading.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut out = format!(
            "{} rounds, {} certified, {} declined, {} refused, {} discordant",
            self.rounds,
            self.accepted(),
            self.declined.values().sum::<usize>(),
            self.refusals.len(),
            self.discords.len()
        );
        for (claim, count) in &self.certified {
            out.push_str(&format!("\n  certified {claim}: {count}"));
        }
        for (reason, count) in &self.declined {
            out.push_str(&format!("\n  declined {reason}: {count}"));
        }
        out.push_str(&format!(
            "\n  widest certificate {} bytes, widest rational {} bits, {} work units",
            self.widest_certificate, self.widest_rational, self.work
        ));
        for finding in self.refusals.iter().take(3) {
            out.push_str(&format!(
                "\n--- round {} refused by {}: {}\n{}",
                finding.round, finding.rule, finding.detail, finding.source
            ));
        }
        for finding in self.discords.iter().take(3) {
            out.push_str(&format!(
                "\n--- round {}: certified {} against reference {}\n{}",
                finding.round, finding.certified, finding.reference, finding.source
            ));
        }
        out
    }
}

/// The word the reference solver's outcome travels as.
fn reference_word(outcome: &ref_lp::Outcome) -> &'static str {
    match outcome {
        ref_lp::Outcome::Optimal { .. } => "optimal",
        ref_lp::Outcome::Infeasible => "infeasible",
        ref_lp::Outcome::Unbounded => "unbounded",
        ref_lp::Outcome::Exhausted => "exhausted",
    }
}

/// Whether a certified claim and a reference outcome can both be true.
///
/// Only the three definite claims are compared. A certified gap or a point
/// with no bound says nothing the reference solver's status contradicts, and
/// an exhausted reference says nothing at all.
fn compatible(certified: &str, reference: &str) -> bool {
    match (certified, reference) {
        (_, "exhausted") => true,
        ("optimal", "optimal") | ("infeasible", "infeasible") | ("unbounded", "unbounded") => true,
        ("optimal" | "infeasible" | "unbounded", _) => false,
        _ => true,
    }
}

/// The word a claim travels as in the report.
fn claim_word(claim: hc_verify::vhcc::Claim) -> &'static str {
    use hc_verify::vhcc::Claim;
    match claim {
        Claim::Optimal => "optimal",
        Claim::Infeasible => "infeasible",
        Claim::Unbounded => "unbounded",
        Claim::Gap => "gap",
        Claim::PointOnly => "point",
    }
}

/// Runs a certification campaign.
#[must_use]
pub fn run(seed: u64, rounds: usize) -> Report {
    let mut generator = Xoshiro256PlusPlus::from_seed(seed);
    let mut report = Report::default();

    for round in 0..rounds {
        let rows = 1 + (round % 7);
        let columns = 1 + (round % 5);
        let instance = generate(&mut generator, rows, columns);
        report.rounds += 1;
        certify_one(round, &instance.source, &instance.model, &mut report);
    }

    report
}

/// One instance, from search to independent verification.
fn certify_one(round: usize, source: &str, model: &Model, report: &mut Report) {
    let raw = source.as_bytes();
    let working = Working::from_model(model);
    let problem = Problem::scaled(&working, Tol::solver());
    let (outcome, _) = solve(&problem, &Options::default());

    let certificate = match assemble_lp(model, raw, &outcome) {
        Ok(certificate) => certificate,
        Err(reason) => {
            // Declining is a designed outcome, not a failure. It is counted by
            // reason so that a run in which the rebuild never succeeds is
            // visible rather than silent.
            *report.declined.entry(reason.kind().to_owned()).or_insert(0) += 1;
            return;
        }
    };
    let bytes = certificate.encode();
    report.widest_certificate = report.widest_certificate.max(bytes.len());

    match verify(&bytes, raw) {
        Err(refusal) => report.refusals.push(Refusal {
            round,
            rule: refusal.code.text().to_owned(),
            detail: refusal.to_string(),
            source: source.to_owned(),
        }),
        Ok(verdict) => {
            let word = claim_word(verdict.claim);
            *report.certified.entry(word.to_owned()).or_insert(0) += 1;
            report.work = report.work.saturating_add(verdict.work);
            report.widest_rational = report.widest_rational.max(verdict.peak_bits);

            let reference = reference_word(&ref_lp::solve(model));
            if !compatible(word, reference) {
                report.discords.push(Discord {
                    round,
                    certified: word.to_owned(),
                    reference: reference.to_owned(),
                    source: source.to_owned(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_certified_claim_may_only_contradict_a_definite_reference_outcome() {
        assert!(compatible("optimal", "optimal"));
        assert!(!compatible("optimal", "infeasible"));
        assert!(!compatible("infeasible", "optimal"));
        assert!(!compatible("unbounded", "optimal"));
        // The reference giving up says nothing, and a partial claim contradicts
        // nothing.
        assert!(compatible("optimal", "exhausted"));
        assert!(compatible("gap", "optimal"));
        assert!(compatible("point", "infeasible"));
    }

    #[test]
    fn a_report_with_nothing_in_it_is_clean_and_says_so_plainly() {
        let report = Report::default();
        assert!(report.clean());
        assert_eq!(report.accepted(), 0);
        assert!(report.describe().starts_with("0 rounds, 0 certified"));
    }
}
