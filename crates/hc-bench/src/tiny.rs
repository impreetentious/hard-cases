//! Tiny integer programs, three ways.
//!
//! Every generated instance is answered by three things that share nothing but
//! the model. The search finds an answer by branching. The enumeration oracle
//! finds one by looking at every point. And the standalone verifier decides
//! whether the search's recursive proof of its answer actually holds.
//!
//! Three answers make the campaign worth more than the sum of two of them. A
//! search that is wrong and a proof that is wrong in the same way would agree
//! with each other, and the oracle would not agree with either. A search that
//! is right and a proof assembler that is wrong would produce a certificate the
//! verifier refuses. The only way through all three is to be right.
//!
//! The instances are small on purpose — a handful of variables over small
//! boxes — because enumeration is exponential and because a disagreement is
//! only useful if a person can sit down with the instance and work out which
//! side is wrong.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use hc_cert::mip::assemble_mip;
use hc_core::{Model, Xoshiro256PlusPlus};
use hc_io::mps::{Form, read};
use hc_mip::branching::Branching;
use hc_mip::search::{Ending, Options, run};
use hc_verify::vclose::verify;
use hc_verify::vhcc::Claim;

use hc_presolve::reduce;

use crate::exhaustive::{Truth, best};
use crate::proof::claim_word;

/// The largest box product the oracle is asked to walk.
pub const BOX_CAP: u64 = 1 << 12;

/// A generated instance and the text it was generated as.
#[derive(Clone, Debug)]
pub struct Instance {
    /// The instance file, in free-form MPS.
    pub source: String,
    /// The parsed model.
    pub model: Model,
}

/// One disagreement, with everything needed to reproduce it.
#[derive(Clone, Debug)]
pub struct Finding {
    /// The round that produced it.
    pub round: usize,
    /// What went wrong.
    pub detail: String,
    /// The instance file.
    pub source: String,
}

/// What a campaign found.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Instances generated.
    pub rounds: usize,
    /// Instances the oracle could answer.
    pub answerable: usize,
    /// How many certificates of each claim the verifier accepted.
    pub certified: BTreeMap<String, usize>,
    /// Why assembly declined, and how often.
    pub declined: BTreeMap<String, usize>,
    /// Everything that went wrong. Any of these fails the campaign.
    pub findings: Vec<Finding>,
    /// The largest tree the campaign built.
    pub widest_tree: usize,
}

impl Report {
    /// Whether the campaign found nothing wrong.
    #[must_use]
    pub fn clean(&self) -> bool {
        self.findings.is_empty()
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
            "{} rounds, {} answerable, {} certified, {} declined, {} findings, widest tree {}",
            self.rounds,
            self.answerable,
            self.accepted(),
            self.declined.values().sum::<usize>(),
            self.findings.len(),
            self.widest_tree
        );
        for (claim, count) in &self.certified {
            let _ = write!(out, "\n  certified {claim}: {count}");
        }
        for (reason, count) in &self.declined {
            let _ = write!(out, "\n  declined {reason}: {count}");
        }
        for finding in self.findings.iter().take(3) {
            let _ = write!(
                out,
                "\n--- round {}: {}\n{}",
                finding.round, finding.detail, finding.source
            );
        }
        out
    }
}

/// Generates one tiny integer program.
///
/// Every variable is integer and every variable is bounded, so the oracle can
/// always answer. The shape is drawn as well as the values: a campaign over one
/// shape tests one shape.
#[must_use]
pub fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> Instance {
    let mut source = String::from("NAME T\nROWS\n N obj\n");
    let senses: Vec<char> = (0..rows)
        .map(|_| match generator.next_below(5) {
            0 => 'E',
            1 => 'G',
            _ => 'L',
        })
        .collect();
    for (index, sense) in senses.iter().enumerate() {
        let _ = writeln!(source, " {sense} r{index}");
    }

    source.push_str("COLUMNS\n MARKER 'MARKER' 'INTORG'\n");
    for column in 0..columns {
        let _ = writeln!(source, " c{column} obj {}", small(generator, 9));
        for row in 0..rows {
            let value = small(generator, 5);
            if value == 0 {
                continue;
            }
            let _ = writeln!(source, " c{column} r{row} {value}");
        }
    }
    source.push_str(" MARKER 'MARKER' 'INTEND'\n");

    source.push_str("RHS\n");
    for (row, sense) in senses.iter().enumerate() {
        // Right-hand sides drawn around the middle of what the boxes can
        // reach, so the campaign sees genuine optima rather than a long run of
        // trivially infeasible or trivially unconstrained instances.
        let value = match sense {
            'L' => generator.next_below(20) as i64,
            'G' => -(generator.next_below(12) as i64),
            _ => small(generator, 6),
        };
        let _ = writeln!(source, " rhs r{row} {value}");
    }

    source.push_str("BOUNDS\n");
    for column in 0..columns {
        let high = 1 + generator.next_below(3);
        let _ = writeln!(source, " UI b c{column} {high}");
    }
    source.push_str("ENDATA\n");

    let model = read(source.as_bytes(), Form::Free).unwrap_or_else(|error| {
        panic!("the generator produced something unreadable: {error}\n{source}")
    });
    Instance { source, model }
}

/// A small signed integer in `[-limit, limit]`.
fn small(generator: &mut Xoshiro256PlusPlus, limit: u64) -> i64 {
    generator.next_below(limit * 2 + 1) as i64 - limit as i64
}

/// One combination of the settings a search can be run under.
///
/// The settings change how the search gets there and change nothing about
/// where it arrives. Sweeping them is how that stops being an assertion: every
/// instance is answered under every combination, and every answer is compared
/// against enumeration and against a verified certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setting {
    /// Whether the model is reduced first.
    pub presolve: bool,
    /// Whether branching uses the textbook rule.
    pub simple: bool,
    /// Whether cuts are separated.
    pub cuts: bool,
}

/// Every combination, in a fixed order.
pub const SETTINGS: &[Setting] = &[
    Setting {
        presolve: false,
        simple: false,
        cuts: false,
    },
    Setting {
        presolve: false,
        simple: true,
        cuts: false,
    },
    Setting {
        presolve: true,
        simple: false,
        cuts: false,
    },
    Setting {
        presolve: true,
        simple: true,
        cuts: false,
    },
    Setting {
        presolve: false,
        simple: false,
        cuts: true,
    },
    Setting {
        presolve: true,
        simple: false,
        cuts: true,
    },
];

/// Runs a campaign.
#[must_use]
pub fn run_campaign(seed: u64, rounds: usize) -> Report {
    let mut generator = Xoshiro256PlusPlus::from_seed(seed);
    let mut report = Report::default();

    for round in 0..rounds {
        let rows = 1 + (round % 3);
        let columns = 1 + (round % 4);
        let instance = generate(&mut generator, rows, columns);
        report.rounds += 1;
        one(round, &instance, &mut report);
    }

    report
}

/// Runs a campaign under every combination of the settings.
///
/// Each instance is answered four ways, and all four must agree with
/// enumeration and with their own verified certificates.
#[must_use]
pub fn run_sweep(seed: u64, rounds: usize) -> Report {
    let mut generator = Xoshiro256PlusPlus::from_seed(seed);
    let mut report = Report::default();

    for round in 0..rounds {
        let rows = 1 + (round % 3);
        let columns = 1 + (round % 4);
        let instance = generate(&mut generator, rows, columns);
        report.rounds += 1;
        for setting in SETTINGS {
            swept(round, &instance, *setting, &mut report);
        }
    }

    report
}

/// One instance under one setting.
fn swept(round: usize, instance: &Instance, setting: Setting, report: &mut Report) {
    let truth = best(&instance.model, BOX_CAP);
    if matches!(truth, Truth::Beyond { .. }) {
        return;
    }
    report.answerable += 1;

    let options = Options {
        branching: if setting.simple {
            Branching::MostFractional
        } else {
            Branching::ReliabilityPseudocost
        },
        cut_rounds: if setting.cuts { 2 } else { 0 },
        ..Options::default()
    };
    let raw = instance.source.as_bytes();
    let chain = setting.presolve.then(|| reduce(&instance.model));
    let reduced = chain
        .as_ref()
        .map_or(&instance.model, |chain| &chain.reduced);
    let search = run(reduced, &options);
    report.widest_tree = report.widest_tree.max(search.tree.len());

    let assembled = match &chain {
        Some(chain) => hc_cert::mip::assemble_presolved(&instance.model, chain, raw, &search),
        None => assemble_mip(&instance.model, raw, &search),
    };
    let assembled = match assembled {
        Ok(assembled) => assembled,
        Err(reason) => {
            *report.declined.entry(reason.kind().to_owned()).or_insert(0) += 1;
            return;
        }
    };

    match verify(&assembled.encode(), raw) {
        Err(refusal) => report.findings.push(Finding {
            round,
            detail: format!("{setting:?}: the verifier refused: {refusal}"),
            source: instance.source.clone(),
        }),
        Ok(verdict) => {
            let word = claim_word(verdict.claim);
            *report.certified.entry(word.to_owned()).or_insert(0) += 1;
            if !agrees(word, &verdict, &truth) {
                report.findings.push(Finding {
                    round,
                    detail: format!(
                        "{setting:?}: certified {word} against enumeration's {truth:?}"
                    ),
                    source: instance.source.clone(),
                });
            }
        }
    }
}

/// Whether a certified claim and an enumerated answer can both be true.
fn agrees(word: &str, verdict: &hc_verify::vclose::Verdict, truth: &Truth) -> bool {
    match (word, truth) {
        ("optimal", Truth::Optimal { value, .. }) => {
            verdict
                .objective
                .as_ref()
                .map(hc_verify::vrat::Frac::to_canonical)
                .as_deref()
                == Some(value.to_canonical_string().as_str())
        }
        ("optimal", Truth::Infeasible) | ("infeasible", Truth::Optimal { .. }) => false,
        ("infeasible", Truth::Infeasible) => true,
        _ => true,
    }
}

/// One instance, from search to oracle to verifier.
fn one(round: usize, instance: &Instance, report: &mut Report) {
    let mut fail = |detail: String| {
        report.findings.push(Finding {
            round,
            detail,
            source: instance.source.clone(),
        });
    };

    let truth = best(&instance.model, BOX_CAP);
    if matches!(truth, Truth::Beyond { .. }) {
        return;
    }
    report.answerable += 1;

    let search = run(&instance.model, &Options::default());
    report.widest_tree = report.widest_tree.max(search.tree.len());

    // First: does the search agree with looking at every point?
    match (&search.ending, &truth) {
        (Ending::Optimal, Truth::Optimal { value, .. }) => {
            let found = search
                .incumbent
                .as_ref()
                .map(|incumbent| incumbent.objective.clone());
            if found.as_ref() != Some(value) {
                fail(format!(
                    "the search says {found:?} and enumeration says {value}"
                ));
                return;
            }
        }
        (Ending::Infeasible, Truth::Infeasible) => {}
        (Ending::Optimal, Truth::Infeasible) => {
            fail(String::from("the search found a point where there is none"));
            return;
        }
        (Ending::Infeasible, Truth::Optimal { value, .. }) => {
            fail(format!(
                "the search says infeasible and enumeration finds {value}"
            ));
            return;
        }
        // A stopped or unbounded search makes no claim enumeration contradicts.
        _ => {}
    }

    // Second: does the proof of that answer survive an independent recheck?
    let raw = instance.source.as_bytes();
    let assembled = match assemble_mip(&instance.model, raw, &search) {
        Ok(assembled) => assembled,
        Err(reason) => {
            *report.declined.entry(reason.kind().to_owned()).or_insert(0) += 1;
            return;
        }
    };
    let bytes = assembled.encode();
    match verify(&bytes, raw) {
        Err(refusal) => fail(format!("the verifier refused: {refusal}")),
        Ok(verdict) => {
            let word = claim_word(verdict.claim);
            *report.certified.entry(word.to_owned()).or_insert(0) += 1;

            // Third: does the certified claim say the same thing the oracle
            // does? A verified proof of the wrong theorem is the failure this
            // whole arrangement exists to catch.
            match (&verdict.claim, &truth) {
                (Claim::Optimal, Truth::Optimal { value, .. }) => {
                    let stated = verdict
                        .objective
                        .as_ref()
                        .map(hc_verify::vrat::Frac::to_canonical);
                    if stated.as_deref() != Some(value.to_canonical_string().as_str()) {
                        fail(format!(
                            "a certified optimum of {stated:?} against enumeration's {value}"
                        ));
                    }
                }
                (Claim::Optimal, Truth::Infeasible) => {
                    fail(String::from("a certified optimum where there is no point"));
                }
                (Claim::Infeasible, Truth::Optimal { value, .. }) => {
                    fail(format!(
                        "a certified infeasibility where enumeration finds {value}"
                    ));
                }
                _ => {}
            }
        }
    }
}
