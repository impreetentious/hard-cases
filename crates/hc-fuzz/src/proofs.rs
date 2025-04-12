//! The proof target.
//!
//! A certificate is the one input where accepting a damaged file is worse than
//! crashing on it. So this takes real certificates — assembled from generated
//! instances by the ordinary path — damages them, and checks two things.
//!
//! The verifier answers. Every mutated certificate comes back either accepted
//! or refused with an identifier, never with a panic and never with a hang.
//!
//! And the answer is stable. A certificate that verifies must verify again,
//! with the same verdict, because a verifier whose answer depends on anything
//! but its inputs is not a verifier.
//!
//! What is deliberately not asserted is that every mutation is refused. Most
//! are, but a certificate carries fields a change can leave meaning the same
//! thing — a record ordering that was already free, a byte inside a name — and
//! demanding refusal would be demanding that the verifier reject files that
//! are still correct.

use hc_cert::mip::assemble_mip;
use hc_core::Xoshiro256PlusPlus as Rng;
use hc_mip::search::{Options as MipOptions, run as search_tree};
use hc_verify::vclose::verify;

use crate::shapes::{Shape, draft, render};
use crate::{Budget, Finding, Report, without_panicking};

/// Runs the proof target over a budget.
#[must_use]
pub fn campaign(budget: Budget) -> Report {
    let mut report = Report::default();
    for seed in budget.seeds() {
        let drafted = draft(seed);
        let text = render(&drafted, Shape::Lp, seed);
        let Ok(model) = hc_io::lp::read(text.as_bytes()) else {
            // A draft this generator could not render inside the subset is not
            // a finding here; the parser targets are where that is examined.
            continue;
        };
        let search = search_tree(&model, &MipOptions::default());
        let Ok(assembled) = assemble_mip(&model, text.as_bytes(), &search) else {
            continue;
        };
        let proof = assembled.encode();

        // The unmutated certificate first. If this does not verify, nothing
        // the mutations say afterwards means anything.
        report.cases += 1;
        match without_panicking("the verifier", seed, &text, || {
            verify(&proof, text.as_bytes())
        }) {
            Err(finding) => {
                report.findings.push(finding);
                continue;
            }
            Ok(Err(refusal)) => {
                report.findings.push(Finding {
                    target: "the assembled certificate",
                    seed,
                    detail: format!(
                        "an unmutated certificate was refused as {}",
                        refusal.code.text()
                    ),
                    input: text.clone(),
                });
                continue;
            }
            Ok(Ok(first)) => {
                report.accepted += 1;
                let again = verify(&proof, text.as_bytes());
                if again.map(|second| second == first) != Ok(true) {
                    report.findings.push(Finding {
                        target: "the verifier",
                        seed,
                        detail: "two runs over the same bytes gave different verdicts".to_owned(),
                        input: text.clone(),
                    });
                }
            }
        }

        // Then the damage. Each mutation is a separate case with its own
        // derived seed, so a finding names a number that reproduces it.
        let mut rng = Rng::from_seed(seed ^ 0x00c0_ffee);
        for round in 0..12u64 {
            let mut damaged = proof.clone();
            match round % 3 {
                0 => {
                    let at = rng.next_below(damaged.len() as u64) as usize;
                    damaged[at] = damaged[at].wrapping_add(1);
                }
                1 => {
                    let at = rng.next_below(damaged.len() as u64) as usize;
                    damaged.truncate(at);
                }
                _ => {
                    let at = rng.next_below(damaged.len() as u64) as usize;
                    let byte = rng.next_below(256) as u8;
                    damaged.insert(at, byte);
                }
            }
            report.cases += 1;
            let outcome = without_panicking("the verifier", seed, &text, || {
                verify(&damaged, text.as_bytes())
            });
            match outcome {
                Err(finding) => report.findings.push(finding),
                Ok(Ok(_)) => report.accepted += 1,
                Ok(Err(_)) => report.refused += 1,
            }
        }

        // And the other half of the pair: a sound certificate against the
        // wrong file. The digest binds a claim to bytes, so this must always
        // be refused, and the identifier is the one that says so.
        let other = render(
            &draft(seed.wrapping_add(1)),
            Shape::Lp,
            seed.wrapping_add(1),
        );
        report.cases += 1;
        match without_panicking("the verifier", seed, &other, || {
            verify(&proof, other.as_bytes())
        }) {
            Err(finding) => report.findings.push(finding),
            Ok(Ok(_)) => report.findings.push(Finding {
                target: "the instance binding",
                seed,
                detail: "a certificate verified against a file it is not about".to_owned(),
                input: other,
            }),
            Ok(Err(_)) => report.refused += 1,
        }
    }
    report
}
