//! The committed mixed-integer corpus, run on every preflight.
//!
//! This is the gate that consumes those files. Its existence is the point: a
//! set of instance files that no gate reads proves only that somebody once
//! intended to test something, and until this existed the mixed-integer half
//! of the solver had been proved only against models a generator produced.
//!
//! Three checks per entry, and they are deliberately unlike each other.
//!
//! **Enumeration.** Where the product of the integer boxes is small enough,
//! every assignment in the box is walked and the best feasible one is found by
//! looking. That answer consults nothing the solver produced — no relaxation,
//! no bound, no incumbent — so a search that agreed with it by sharing a
//! mistake would have to share it with a program that has no simplex in it.
//!
//! **Certification.** Every entry assembles a certificate and the standalone
//! verifier accepts it against the original bytes. That is a different kind of
//! evidence from the first: enumeration says the answer is right, the verifier
//! says the *reasoning* is checkable.
//!
//! **Separation.** Every entry is solved twice, with cuts and without, and the
//! two must agree about the value. A cut that removes a feasible integer point
//! is the failure that comparison exists to catch, and it is the one failure
//! in this project that a correct-looking answer can hide.
//!
//! The manifest is walked rather than transcribed. An instance named there and
//! not compiled in here fails, and an instance compiled in here and not named
//! there fails too.

use hc_bench::exhaustive::{Truth, best};
use hc_bench::miplib::{cuts_agree, run_one};
use hc_core::SpecDocument;
use hc_io::mps::{Form, read};

const MANIFEST: &str = include_str!("../../../instances/integer/manifest.toml");

/// The node budget every entry is run under.
///
/// Counted in nodes rather than in seconds so that the gate means the same
/// thing on every machine, and set well above what any of these needs so that
/// a run reaching it is a regression rather than a tuning question.
const NODES: u64 = 10_000;

/// The most assignments enumeration will walk, checked before anything is
/// allocated.
const BOX_CAP: u64 = 1 << 20;

const FILES: &[(&str, &[u8])] = &[
    (
        "knap9.mps",
        include_bytes!("../../../instances/integer/knap9.mps"),
    ),
    (
        "fixcharge6.mps",
        include_bytes!("../../../instances/integer/fixcharge6.mps"),
    ),
    (
        "stock3.mps",
        include_bytes!("../../../instances/integer/stock3.mps"),
    ),
    (
        "parity5.mps",
        include_bytes!("../../../instances/integer/parity5.mps"),
    ),
];

/// One entry, as the manifest states it.
struct Entry {
    table: String,
    file: &'static str,
    raw: &'static [u8],
    expect: String,
    oracle: String,
    objective: Option<String>,
}

fn entries() -> Vec<Entry> {
    let document = SpecDocument::parse(MANIFEST).expect("the corpus manifest is inside the subset");
    let mut out = Vec::new();
    for table in document.tables_under("instance") {
        let file = table
            .text("file")
            .unwrap_or_else(|| panic!("[{}] names no file", table.name()));
        let (name, raw) = FILES
            .iter()
            .find(|(candidate, _)| *candidate == file)
            .unwrap_or_else(|| panic!("[{}] names {file}, which is not compiled in", table.name()));
        out.push(Entry {
            table: table.name().to_owned(),
            file: name,
            raw,
            expect: table
                .text("expect")
                .unwrap_or_else(|| panic!("[{}] states no expectation", table.name()))
                .to_owned(),
            oracle: table
                .text("oracle")
                .unwrap_or_else(|| panic!("[{}] states no oracle word", table.name()))
                .to_owned(),
            objective: table.text("objective").map(str::to_owned),
        });
    }
    out
}

#[test]
fn the_manifest_and_the_compiled_corpus_are_the_same_list() {
    let named: Vec<&str> = entries().iter().map(|entry| entry.file).collect();
    for (file, _) in FILES {
        assert!(
            named.contains(file),
            "{file} is compiled in and the manifest does not name it"
        );
    }
    assert_eq!(
        named.len(),
        FILES.len(),
        "the two lists are different sizes"
    );
}

#[test]
fn every_entry_reaches_its_status_and_certifies() {
    let mut findings = Vec::new();
    for entry in entries() {
        let row = run_one(entry.file, entry.raw, NODES);
        if let Some(finding) = row.finding {
            findings.push(format!("{}: {finding}", entry.table));
            continue;
        }
        if row.ending != entry.expect {
            findings.push(format!(
                "{}: the manifest expects {} and the search ended {}",
                entry.table, entry.expect, row.ending
            ));
        }
        if row.verified.is_none() {
            findings.push(format!(
                "{}: no certificate the standalone verifier accepted",
                entry.table
            ));
        }
        if let (Some(stated), Some(attained)) =
            (entry.objective.as_deref(), row.objective.as_deref())
        {
            if stated != attained {
                findings.push(format!(
                    "{}: the manifest records {stated} and the search attained {attained}",
                    entry.table
                ));
            }
        }
    }
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}

#[test]
fn the_enumerable_entries_agree_with_enumeration() {
    let mut findings = Vec::new();
    for entry in entries() {
        let model = read(entry.raw, Form::Free).expect("a corpus entry parses");
        let truth = best(&model, BOX_CAP);
        // The oracle word is checked in both directions. An entry claiming
        // enumeration that turns out to be beyond it would be an entry whose
        // strongest check silently stopped running.
        match (entry.oracle.as_str(), &truth) {
            ("exhaustive", Truth::Beyond { reason }) => {
                findings.push(format!(
                    "{}: the manifest claims enumeration and it is beyond it: {reason}",
                    entry.table
                ));
                continue;
            }
            ("beyond", Truth::Beyond { .. }) => continue,
            ("beyond", _) => {
                findings.push(format!(
                    "{}: the manifest gives up on enumeration and enumeration answered",
                    entry.table
                ));
                continue;
            }
            ("exhaustive", _) => {}
            (other, _) => {
                findings.push(format!("{}: {other} names no oracle", entry.table));
                continue;
            }
        }

        let row = run_one(entry.file, entry.raw, NODES);
        match truth {
            Truth::Optimal { value, .. } => {
                let enumerated = value.to_canonical_string();
                if row.ending != "optimal" {
                    findings.push(format!(
                        "{}: enumeration found an optimum and the search ended {}",
                        entry.table, row.ending
                    ));
                } else if row.objective.as_deref() != Some(enumerated.as_str()) {
                    findings.push(format!(
                        "{}: enumeration says {enumerated} and the search says {:?}",
                        entry.table, row.objective
                    ));
                }
            }
            Truth::Infeasible => {
                if row.ending != "infeasible" {
                    findings.push(format!(
                        "{}: enumeration found nothing feasible and the search ended {}",
                        entry.table, row.ending
                    ));
                }
            }
            Truth::Beyond { .. } => unreachable!("handled above"),
        }
    }
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}

#[test]
fn separation_does_not_change_any_answer() {
    let mut findings = Vec::new();
    for entry in entries() {
        if let Some(finding) = cuts_agree(entry.file, entry.raw, NODES) {
            findings.push(format!("{}: {finding}", entry.table));
        }
    }
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}
