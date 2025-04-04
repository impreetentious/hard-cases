//! Tiny integer programs, answered three ways.
//!
//! The search, the enumeration oracle, and the standalone verifier all answer
//! the same instance, and the campaign fails if any two of them disagree. The
//! shape of what it found is asserted as well as the absence of findings: a
//! campaign that certified nothing, or that only ever saw infeasible
//! instances, would be clean and worthless.

use hc_bench::tiny::{Report, run_campaign};

/// The per-change campaign size, from the normative constants.
const PER_CHANGE: usize = 2_000;

/// The nightly size.
const NIGHTLY: usize = 50_000;

fn expect_clean(report: &Report) {
    assert!(
        report.clean(),
        "the tiny campaign found something\n{}",
        report.describe()
    );
}

#[test]
fn the_search_the_oracle_and_the_verifier_agree_on_every_tiny_instance() {
    let report = run_campaign(0x7a17_0000_0000_0001, PER_CHANGE);
    expect_clean(&report);

    assert!(
        report.answerable > PER_CHANGE / 2,
        "only {} of {} instances were small enough to enumerate\n{}",
        report.answerable,
        report.rounds,
        report.describe()
    );
    assert!(
        report.accepted() > PER_CHANGE / 4,
        "only {} certificates were accepted\n{}",
        report.accepted(),
        report.describe()
    );
    for claim in ["optimal", "infeasible"] {
        assert!(
            report.certified.get(claim).copied().unwrap_or(0) > 0,
            "the campaign never certified {claim}\n{}",
            report.describe()
        );
    }
    // A campaign that never branched would be testing the linear machinery
    // again under a different name.
    assert!(
        report.widest_tree > 1,
        "no instance needed a split\n{}",
        report.describe()
    );
}

#[test]
fn the_campaign_is_reproducible() {
    let first = run_campaign(0x7a17_0000_0000_002a, 300);
    let second = run_campaign(0x7a17_0000_0000_002a, 300);
    assert_eq!(first.certified, second.certified);
    assert_eq!(first.declined, second.declined);
    assert_eq!(first.answerable, second.answerable);
    assert_eq!(first.widest_tree, second.widest_tree);
}

/// The nightly size. Ignored in the ordinary suite so a preflight does not pay
/// for it, and run by name in the scheduled build.
#[test]
#[ignore = "nightly size; run with --ignored"]
fn the_nightly_tiny_campaign_is_clean() {
    let report = run_campaign(0x7a17_0000_0000_0003, NIGHTLY);
    expect_clean(&report);
    assert!(report.accepted() > NIGHTLY / 4, "{}", report.describe());
}
