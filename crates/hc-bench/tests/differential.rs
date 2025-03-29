//! The production solver against the reference solver, on generated
//! instances.
//!
//! This is the campaign the whole reference implementation exists for. Two
//! linear-programming solvers that share no code are given the same instance
//! text; where they disagree about a status or an objective, one of them is
//! wrong, and which one is a question worth spending a day on.
//!
//! The run inside a preflight is the per-change size from the normative
//! constants. The nightly size is the same code with a larger count, run under
//! its own name so the ordinary suite does not pay for it.
//!
//! A campaign that produced only one kind of answer would pass while testing
//! almost nothing, so the shape of what it found is asserted as well as the
//! absence of findings.

use hc_bench::campaign::{Report, run};

/// The per-change campaign size, from the normative constants.
const PER_CHANGE: usize = 2_000;

/// The nightly campaign size.
const NIGHTLY: usize = 100_000;

fn describe(report: &Report) -> String {
    let mut out = format!(
        "{} rounds, {} agreed, {} inconclusive, {} disagreed",
        report.rounds,
        report.agreements,
        report.inconclusive,
        report.disagreements.len()
    );
    for (pair, count) in &report.statuses {
        out.push_str(&format!("\n  {pair}: {count}"));
    }
    for finding in report.disagreements.iter().take(3) {
        out.push_str(&format!(
            "\n--- round {}: {}\n{}",
            finding.round, finding.detail, finding.source
        ));
    }
    out
}

/// Fails with everything needed to reproduce whatever it found.
fn expect_agreement(report: &Report) {
    assert!(report.disagreements.is_empty(), "{}", describe(report));
}

/// Asserts the campaign actually visited the answers it is meant to compare.
fn expect_coverage(report: &Report) {
    let seen = |name: &str| report.statuses.keys().any(|pair| pair == name);
    assert!(
        seen("optimal/optimal"),
        "no instance had an optimum:\n{}",
        describe(report)
    );
    assert!(
        seen("infeasible/infeasible"),
        "no instance was infeasible:\n{}",
        describe(report)
    );
    assert!(
        seen("unbounded/unbounded"),
        "no instance was unbounded:\n{}",
        describe(report)
    );
    // Every round must reach a comparison. A campaign quietly dominated by
    // effort stops would report a pass it did not earn.
    assert!(
        report.inconclusive * 20 < report.rounds,
        "too many rounds reached no conclusion:\n{}",
        describe(report)
    );
}

#[test]
fn the_per_change_campaign_finds_no_disagreement() {
    let report = run(0x0d1f_0001, PER_CHANGE);
    expect_agreement(&report);
    expect_coverage(&report);
    assert_eq!(report.agreements + report.inconclusive, report.rounds);
}

#[test]
fn a_second_stream_finds_no_disagreement_either() {
    // One stream visits one region of the generator's space.
    let report = run(0x0d1f_0002, PER_CHANGE / 2);
    expect_agreement(&report);
    expect_coverage(&report);
}

#[test]
fn the_campaign_is_reproducible() {
    // A campaign that cannot be replayed cannot be minimized when it fails,
    // which would make every finding a curiosity rather than a bug report.
    let first = run(0x0d1f_0003, 200);
    let second = run(0x0d1f_0003, 200);
    assert_eq!(first.rounds, second.rounds);
    assert_eq!(first.agreements, second.agreements);
    assert_eq!(first.statuses, second.statuses);
    assert_eq!(first.disagreements.len(), second.disagreements.len());
}

#[test]
#[ignore = "the nightly campaign; run it by name or under the nightly job"]
fn the_nightly_campaign_finds_no_disagreement() {
    let report = run(0x0d1f_0001, NIGHTLY);
    expect_agreement(&report);
    expect_coverage(&report);
}
