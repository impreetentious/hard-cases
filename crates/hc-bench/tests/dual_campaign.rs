//! The two simplex methods against each other, on generated instances.
//!
//! The primal keeps every basic variable inside its bounds; the dual keeps
//! every column priced correctly. They are different algorithms with different
//! failure modes, and on any instance they both finish they must agree about
//! the status and about the objective.
//!
//! Refusals are counted separately from agreements. The dual method declines
//! to start when the logical basis does not price correctly, which is common
//! and correct, and a campaign that counted those as passes would report a
//! perfect record for a method that had not run.

use hc_bench::campaign::{Report, compare_methods, run_with};

/// The per-change campaign size.
const PER_CHANGE: usize = 2_000;

/// The nightly size.
const NIGHTLY: usize = 50_000;

fn describe(report: &Report) -> String {
    let mut out = format!(
        "{} rounds, {} agreed, {} could not be compared, {} disagreed",
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

#[test]
fn the_two_methods_never_disagree() {
    let report = run_with(0xd0a1_0000_0000_0001, PER_CHANGE, &compare_methods);
    assert!(
        report.disagreements.is_empty(),
        "the two methods disagree\n{}",
        describe(&report)
    );
    // A campaign in which the dual never started would be clean and useless.
    assert!(
        report.agreements > PER_CHANGE / 10,
        "the dual method ran on only {} of {} instances\n{}",
        report.agreements,
        report.rounds,
        describe(&report)
    );
}

#[test]
fn the_comparison_is_reproducible() {
    let first = run_with(0xd0a1_0000_0000_002a, 200, &compare_methods);
    let second = run_with(0xd0a1_0000_0000_002a, 200, &compare_methods);
    assert_eq!(first.agreements, second.agreements);
    assert_eq!(first.inconclusive, second.inconclusive);
    assert_eq!(first.statuses, second.statuses);
}

#[test]
#[ignore = "nightly size; run with --ignored"]
fn the_nightly_dual_campaign_is_clean() {
    let report = run_with(0xd0a1_0000_0000_0003, NIGHTLY, &compare_methods);
    assert!(
        report.disagreements.is_empty(),
        "the two methods disagree\n{}",
        describe(&report)
    );
}
