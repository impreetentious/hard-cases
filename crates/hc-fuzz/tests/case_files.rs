//! The case-file pipeline.
//!
//! A case file is a claim that something was wrong and is not any more, and
//! both halves have to still be true. This reads every file in the catalogue,
//! checks that its frontmatter is complete and that what it names exists, and
//! — for the entries that name a seed — runs that seed and requires the
//! campaign to come back clean.
//!
//! That last part is what makes the catalogue a regression suite rather than a
//! set of anecdotes. A fix that came undone would fail here at the seed the
//! case file names.

use std::fs;
use std::path::PathBuf;

use hc_fuzz::{Budget, parsers, proofs};

/// The fields every case file opens with.
const FIELDS: [&str; 6] = [
    "id",
    "crate",
    "found-by",
    "reproduce",
    "fixed-in",
    "rejection",
];

/// The headings every case file carries, in order.
const HEADINGS: [&str; 5] = ["Symptom", "Hunt", "Root cause", "Fix", "Lesson"];

fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("case-files")
}

/// Every case file, as (name, text), in name order.
fn files() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = fs::read_dir(directory())
        .expect("the catalogue directory")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("HC-CASE-") {
                return None;
            }
            Some((name, fs::read_to_string(entry.path()).expect("a case file")))
        })
        .collect();
    out.sort();
    out
}

/// One frontmatter field.
fn field<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines()
        .take_while(|line| *line != "---" || text.starts_with(line))
        .find_map(|line| line.strip_prefix(&format!("{name}: ")))
        .map(str::trim)
}

#[test]
fn every_case_file_is_complete_and_says_what_it_is_about() {
    let files = files();
    assert!(!files.is_empty(), "the catalogue is empty");
    for (name, text) in &files {
        assert!(text.starts_with("---\n"), "{name} has no frontmatter");
        for wanted in FIELDS {
            assert!(
                field(text, wanted).is_some_and(|value| !value.is_empty()),
                "{name} has no {wanted}"
            );
        }
        let id = field(text, "id").expect("an id");
        assert_eq!(
            format!("{id}.md"),
            *name,
            "{name} carries an identifier that is not its own"
        );
        for heading in HEADINGS {
            assert!(
                text.contains(&format!("## {heading}")),
                "{name} has no {heading} section"
            );
        }
        // The catalogue is not padded, and nothing in it is a placeholder.
        for marker in ["TODO", "TBD", "FIXME", "placeholder"] {
            assert!(
                !text.contains(marker),
                "{name} carries an unfinished marker: {marker}"
            );
        }
    }
}

#[test]
fn every_identifier_is_used_once_and_the_numbering_has_no_holes() {
    // Identifiers are cited from outside — by a commit message, by a report,
    // by another case file — so a reused one is a citation that now points
    // somewhere else.
    let files = files();
    for (index, (name, _)) in files.iter().enumerate() {
        assert_eq!(
            *name,
            format!("HC-CASE-{:04}.md", index + 1),
            "the catalogue skips or repeats a number"
        );
    }
}

#[test]
fn every_named_rejection_identifier_exists() {
    // A case file that turns on a rule cites the rule by the identifier the
    // verifier prints, and a citation nobody can look up is decoration.
    const REJECTIONS: &str = include_str!("../../../spec/v1/rejections.toml");
    for (name, text) in files() {
        let stated = field(&text, "rejection").expect("a rejection field");
        if stated == "none" {
            continue;
        }
        assert!(
            REJECTIONS.contains(&format!("[reject.{stated}]")),
            "{name} cites {stated}, which the rejection table does not declare"
        );
    }
}

#[test]
fn every_seed_a_case_file_names_is_clean_now() {
    // The regression half. Each seed reproduces the exact case that was wrong,
    // and the campaign has to come back with nothing to say about it.
    let mut checked = 0usize;
    for (name, text) in files() {
        let line = field(&text, "reproduce").expect("a reproduce field");
        let Some(seed) = line
            .split_whitespace()
            .skip_while(|word| *word != "--from")
            .nth(1)
            .and_then(|word| word.parse::<u64>().ok())
        else {
            continue;
        };
        let budget = Budget {
            from: seed,
            cases: 1,
        };
        let mut report = parsers::campaign(budget);
        report.absorb(proofs::campaign(budget));
        assert!(
            report.clean(),
            "{name} names seed {seed}, which is a finding again:\n{}",
            report
                .findings
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
        checked += 1;
    }
    assert!(checked > 0, "no case file names a seed to replay");
}
