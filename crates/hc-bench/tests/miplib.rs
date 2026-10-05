//! The fetched mixed-integer subset, and an honest account of what is not there.
//!
//! These instances are not committed, so on most machines this test finds
//! nothing. That is the interesting case and the one this file is written
//! around: a harness that quietly passes when its corpus is absent has proved
//! that somebody once intended to test something.
//!
//! So the harness itself is checked unconditionally — the manifest parses, the
//! split names are disjoint, the fetch script names exactly the instances the
//! manifest does, and the run function works on an instance that *is*
//! committed — and whatever is present is run and must not be a finding.

use std::fs;
use std::path::PathBuf;

use hc_bench::miplib::{HEADING, against_published, cuts_agree, run_one};
use hc_core::{SpecDocument, SpecValue};

const MANIFEST: &str = include_str!("../../../instances/miplib/manifest.toml");
const FETCH_SCRIPT: &str = include_str!("../../../scripts/fetch-miplib.sh");
/// A committed instance with an integer column, so the runner itself is
/// exercised on every machine rather than only where the corpus was fetched.
/// It is read under the same free-form rules the corpus is.
const LOCAL: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn manifest() -> SpecDocument {
    SpecDocument::parse(MANIFEST).expect("the manifest is inside the subset")
}

fn listed(table: &str, key: &str) -> Vec<String> {
    manifest()
        .table(table)
        .and_then(|found| found.get(key))
        .and_then(SpecValue::as_list)
        .unwrap_or_else(|| panic!("[{table}] has no {key}"))
        .to_vec()
}

fn names() -> Vec<String> {
    let mut all = listed("split", "dev");
    all.extend(listed("split", "held"));
    all
}

fn budget() -> u64 {
    manifest()
        .table("budget")
        .and_then(|table| table.integer("nodes"))
        .and_then(|nodes| u64::try_from(nodes).ok())
        .expect("the manifest states an effort cap")
}

#[test]
fn the_manifest_states_what_it_covers_and_what_pinning_means() {
    let document = manifest();
    let meta = document.table("meta").expect("a meta table");
    assert_eq!(meta.integer("version"), Some(1));
    assert!(
        meta.text("source").is_some(),
        "the manifest does not say where the files come from"
    );
    assert_eq!(
        meta.get("committed").and_then(SpecValue::as_boolean),
        Some(false),
        "the manifest must say plainly that these files are not committed"
    );

    let pin = document.table("pin").expect("a pin table");
    assert!(
        pin.text("rule")
            .is_some_and(|text| text.contains("verified"))
    );
    assert!(
        pin.text("absent")
            .is_some_and(|text| text.contains("says so"))
    );

    // The published values are optional, and the manifest has to say where
    // they may not come from. A value this solver produced and then agreed
    // with is a mirror, not a cross-check.
    let published = document.table("published").expect("a published table");
    assert!(
        published
            .text("forbidden")
            .is_some_and(|text| text.contains("from memory")),
        "the manifest no longer forbids writing published values from memory"
    );

    assert!(!names().is_empty(), "the manifest names no instances");
    assert!(budget() > 0);
}

#[test]
fn the_two_halves_of_the_split_are_disjoint() {
    // The whole value of a held-out half is that nothing was tuned against it.
    // An instance in both halves is an instance that was.
    let dev = listed("split", "dev");
    let held = listed("split", "held");
    for name in &dev {
        assert!(
            !held.contains(name),
            "{name} is in both halves of the split"
        );
    }
    assert!(!dev.is_empty() && !held.is_empty());
}

#[test]
fn the_fetch_script_and_the_manifest_name_the_same_instances() {
    // Two lists that must agree, in a place where drifting apart is silent:
    // the script would fetch what nothing reads, or the harness would look for
    // what nothing fetches.
    let line = FETCH_SCRIPT
        .lines()
        .find(|line| line.starts_with("names=("))
        .expect("the script lists its instances in one place");
    let in_script: Vec<&str> = line
        .trim_start_matches("names=(")
        .trim_end_matches(')')
        .split_whitespace()
        .collect();
    assert_eq!(in_script.len(), names().len(), "the lists differ in length");
    for name in names() {
        assert!(
            in_script.contains(&name.as_str()),
            "the script omits {name}"
        );
    }
    assert!(
        FETCH_SCRIPT.contains("RECORDED, not VERIFIED"),
        "the fetch script no longer distinguishes recording a pin from verifying one"
    );
    assert!(
        FETCH_SCRIPT.contains("does not match its pin"),
        "the fetch script no longer fails on a mismatch"
    );
}

#[test]
fn the_runner_certifies_an_instance_that_is_committed() {
    // The corpus is usually absent; the runner is not allowed to be untested
    // because of that. This is a committed instance, run through exactly the
    // path a fetched one takes.
    let row = run_one("tiny", LOCAL, 2000);
    assert!(row.finding.is_none(), "{:?}", row.finding);
    assert_eq!(row.ending, "optimal");
    assert!(row.integers > 0, "the instance has no integer columns");
    assert!(
        row.verified.is_some(),
        "a terminal status reached the board without a verifier verdict"
    );
    assert!(row.render().contains("tiny"));
    assert!(HEADING.contains("verified"));

    // The value cross-check reports a disagreement and stays quiet when there
    // is nothing to compare against.
    assert!(against_published(&row, None).is_none());
    assert!(against_published(&row, row.objective.as_deref()).is_none());
    assert_eq!(row.objective.as_deref(), Some("-4"));
    assert!(
        against_published(&row, Some("999999")).is_some(),
        "a disagreement with a published value must be a finding"
    );

    // And the cuts comparison, on the same committed instance.
    assert!(cuts_agree("tiny", LOCAL, 2000).is_none());
}

#[test]
fn whatever_has_been_fetched_is_run_and_what_is_missing_is_named() {
    let directory = root().join("instances").join("miplib");
    let nodes = budget();
    let dev = listed("split", "dev");
    let mut ran = Vec::new();
    let mut missing = Vec::new();
    let mut findings = Vec::new();

    for name in names() {
        let path = directory.join(format!("{name}.mps"));
        let Ok(raw) = fs::read(&path) else {
            missing.push(name);
            continue;
        };
        let row = run_one(&name, &raw, nodes);
        if let Some(finding) = &row.finding {
            findings.push(format!("{name}: {finding}"));
        }
        // The development half is where a cut family may be looked at, so it
        // is where the cuts are made to answer for themselves.
        if dev.contains(&name)
            && let Some(finding) = cuts_agree(&name, &raw, nodes)
        {
            findings.push(finding);
        }
        ran.push(row);
    }

    if ran.is_empty() {
        assert_eq!(missing.len(), names().len());
        println!("miplib: no instance has been fetched here; run scripts/fetch-miplib.sh");
        println!("miplib: not present: {}", missing.join(", "));
        return;
    }

    println!("{HEADING}");
    for row in &ran {
        println!("{}", row.render());
    }
    println!("miplib: {} present, {} missing", ran.len(), missing.len());
    if !missing.is_empty() {
        println!("miplib: not present: {}", missing.join(", "));
    }
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}
