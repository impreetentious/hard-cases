//! The fetched netlib subset, and an honest account of what is not there.
//!
//! These instances are not committed, so on most machines this test finds
//! nothing. That is the interesting case, and the one this file is written
//! around: a harness that quietly passes when its corpus is absent has proved
//! that somebody once intended to test something.
//!
//! So the harness itself is checked unconditionally — the manifest parses, the
//! fetch script exists and names exactly the instances the manifest does — and
//! whatever *is* present is run and must not be a finding. What is missing is
//! reported by name rather than skipped, which is the same rule the scoreboard
//! follows: an instance that cannot be fetched, read, or solved stays on the
//! board with the reason beside it.

use std::fs;
use std::path::PathBuf;

use hc_bench::corpus::{Row, run_one};
use hc_core::SpecDocument;

const MANIFEST: &str = include_str!("../../../instances/netlib/manifest.toml");
const FETCH_SCRIPT: &str = include_str!("../../../scripts/fetch-netlib.sh");

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn names() -> Vec<String> {
    let document = SpecDocument::parse(MANIFEST).expect("the manifest is inside the subset");
    document
        .table("easy")
        .and_then(|table| table.get("names"))
        .and_then(hc_core::SpecValue::as_list)
        .expect("the manifest names its instances")
        .to_vec()
}

#[test]
fn the_manifest_states_what_it_covers_and_what_pinning_means() {
    let document = SpecDocument::parse(MANIFEST).expect("inside the subset");
    let meta = document.table("meta").expect("a meta table");
    assert_eq!(meta.integer("version"), Some(1));
    assert!(
        meta.text("source").is_some(),
        "the manifest does not say where the files come from"
    );
    assert_eq!(
        meta.get("committed")
            .and_then(hc_core::SpecValue::as_boolean),
        Some(false),
        "the manifest must say plainly that these files are not committed"
    );

    let pin = document.table("pin").expect("a pin table");
    // The two sentences that keep a pin from becoming a formality: it is
    // recorded from a real fetch, and its absence is reported rather than
    // assumed away.
    assert!(
        pin.text("rule")
            .is_some_and(|text| text.contains("verified"))
    );
    assert!(
        pin.text("absent")
            .is_some_and(|text| text.contains("says so"))
    );

    assert!(!names().is_empty(), "the manifest names no instances");
}

#[test]
fn the_fetch_script_and_the_manifest_name_the_same_instances() {
    // Two lists that must agree, in a place where drifting apart is silent:
    // the script would fetch what nothing reads, or the harness would look for
    // what nothing fetches.
    let listed = FETCH_SCRIPT
        .lines()
        .find(|line| line.starts_with("names=("))
        .expect("the script lists its instances in one place");
    let in_script: Vec<&str> = listed
        .trim_start_matches("names=(")
        .trim_end_matches(')')
        .split_whitespace()
        .collect();
    assert_eq!(
        in_script.len(),
        names().len(),
        "the two lists are different lengths"
    );
    for name in names() {
        assert!(
            in_script.contains(&name.as_str()),
            "the script omits {name}"
        );
    }
}

#[test]
fn the_fetch_script_records_a_first_pin_rather_than_claiming_to_verify_it() {
    // The distinction this whole arrangement rests on. A first fetch cannot
    // verify anything, and a script that said it had would be the most
    // convincing possible way to pin nothing.
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
fn whatever_has_been_fetched_is_run_and_what_is_missing_is_named() {
    let directory = root().join("instances").join("netlib");
    let mut ran: Vec<Row> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    for name in names() {
        let path = directory.join(format!("{name}.mps"));
        match fs::read(&path) {
            Ok(raw) => ran.push(run_one(&name, &raw, None)),
            Err(_) => missing.push(name),
        }
    }

    // Nothing present is the ordinary state, and it is reported rather than
    // passed over. The assertion that matters is about what *is* present.
    if ran.is_empty() {
        assert_eq!(missing.len(), names().len());
        println!(
            "netlib: no instance has been fetched on this machine; run scripts/fetch-netlib.sh"
        );
        println!("netlib: not present: {}", missing.join(", "));
        return;
    }

    println!("netlib: {} present, {} missing", ran.len(), missing.len());
    if !missing.is_empty() {
        println!("netlib: not present: {}", missing.join(", "));
    }
    let findings: Vec<&Row> = ran.iter().filter(|row| row.is_finding()).collect();
    assert!(
        findings.is_empty(),
        "{}",
        findings
            .iter()
            .map(|row| format!("{}: {}", row.name, row.finding.clone().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
