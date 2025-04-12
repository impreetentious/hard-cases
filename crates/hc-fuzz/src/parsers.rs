//! The parser targets.
//!
//! Three properties, in increasing order of what they would cost to be wrong.
//!
//! No panic. A reader is handed files from wherever a model came from, and a
//! panic in one is a way to stop the tool with a file rather than a defect
//! somewhere harmless.
//!
//! The two implementations agree. Where both accept, the canonical dumps must
//! be identical bytes; where both refuse, the identifiers must match; and one
//! accepting what the other refuses is the finding this whole arrangement
//! exists to produce.
//!
//! One model, four files. A draft rendered in every accepted reading has to
//! produce one model. The digest differs because the bytes and the tag differ;
//! everything below it must not.

use hc_core::Model;

use crate::edits::{Edit, apply};
use crate::shapes::{Shape, draft, render};
use crate::{Budget, Finding, Report, without_panicking};

/// Reads with the solver's reader for a shape.
fn solver(raw: &[u8], shape: Shape) -> Result<Model, String> {
    match shape {
        Shape::MpsFixed => hc_io::mps::read(raw, hc_io::mps::Form::Fixed),
        Shape::MpsFree => hc_io::mps::read(raw, hc_io::mps::Form::Free),
        Shape::Lp => hc_io::lp::read(raw),
        Shape::Json => hc_io::json::read(raw),
    }
    .map_err(|refusal| refusal.id.as_str().to_owned())
}

/// Reads with the verifier's reader for a shape.
fn verifier(raw: &[u8], shape: Shape) -> Result<hc_verify::Instance, String> {
    match shape {
        Shape::MpsFixed => hc_verify::vparse::read(raw, hc_verify::vparse::Layout::Fixed),
        Shape::MpsFree => hc_verify::vparse::read(raw, hc_verify::vparse::Layout::Free),
        Shape::Lp => hc_verify::vlp::read(raw),
        Shape::Json => hc_verify::vjson::read(raw),
    }
    .map_err(|refusal| refusal.code.text().to_owned())
}

/// Everything the canonical dump says about the model rather than the file.
fn below(dump: &str) -> String {
    dump.lines().skip(3).collect::<Vec<_>>().join("\n")
}

/// Runs the parser targets over a budget.
#[must_use]
pub fn campaign(budget: Budget) -> Report {
    let mut report = Report::default();
    for seed in budget.seeds() {
        let drafted = draft(seed);
        let mut dumps: Vec<(Shape, String)> = Vec::new();

        for shape in Shape::ALL {
            let text = render(&drafted, shape, seed);
            for edit in Edit::ALL {
                let mutated = apply(&text, edit, seed ^ (edit as u64).wrapping_mul(31));
                let what = format!("{} {}", shape.as_str(), edit.as_str());
                report.cases += 1;
                match one(seed, shape, &what, &mutated) {
                    Ok(Some(dump)) => {
                        report.accepted += 1;
                        if edit == Edit::None {
                            dumps.push((shape, dump));
                        }
                    }
                    Ok(None) => report.refused += 1,
                    Err(finding) => report.findings.push(finding),
                }
            }
        }

        // The rendering of one draft in four readings is one model. This is
        // the property no single-format harness can see.
        if dumps.len() == Shape::ALL.len() {
            let (first, expected) = &dumps[0];
            for (shape, dump) in &dumps[1..] {
                if dump != expected {
                    report.findings.push(Finding {
                        target: "one draft, four readings",
                        seed,
                        detail: format!(
                            "{} and {} read the same draft as different models",
                            first.as_str(),
                            shape.as_str()
                        ),
                        input: format!("{}\n---\n{dump}", expected.clone()),
                    });
                }
            }
        }
    }
    report
}

/// One case: both readers, compared.
fn one(seed: u64, shape: Shape, what: &str, text: &str) -> Result<Option<String>, Finding> {
    let raw = text.as_bytes();
    let left = without_panicking("the solver's reader", seed, text, || solver(raw, shape))?;
    let right = without_panicking("the verifier's reader", seed, text, || verifier(raw, shape))?;
    let finding = |detail: String| Finding {
        target: "the two readers",
        seed,
        detail: format!("{what}: {detail}"),
        input: text.to_owned(),
    };
    match (left, right) {
        (Ok(left), Ok(right)) => {
            let left = left.canonical_dump();
            let right = right.dump();
            if left == right {
                Ok(Some(below(&left)))
            } else {
                Err(finding(format!(
                    "accepted with different canonical dumps\n{left}\n---\n{right}"
                )))
            }
        }
        (Err(left), Err(right)) => {
            if left == right {
                Ok(None)
            } else {
                Err(finding(format!("refused as {left} and as {right}")))
            }
        }
        (Ok(_), Err(right)) => Err(finding(format!(
            "the solver accepted what the verifier refused as {right}"
        ))),
        (Err(left), Ok(_)) => Err(finding(format!(
            "the verifier accepted what the solver refused as {left}"
        ))),
    }
}
