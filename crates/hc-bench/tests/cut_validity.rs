//! A cut must never remove an answer.
//!
//! This is the gate that stands between cutting and a confident wrong number.
//! Every other property of a cut — that its derivation replays, that its scope
//! is respected, that the verifier recomputes it — is about the proof being
//! well formed. This one is about the proof being true: if a cut excludes a
//! feasible integer point of the region it is scoped to, the search may return
//! a worse answer than the model's, and every certificate built on top of that
//! cut will be a valid proof of a false statement.
//!
//! The campaign consults nothing the solver derived. It enumerates the model's
//! own feasible integer points and holds each cut against the ones inside its
//! scope.

use hc_bench::cuts::{Report, run};

/// The per-change campaign size.
const PER_CHANGE: usize = 1_500;

/// The nightly campaign size.
const NIGHTLY: usize = 100_000;

fn expect_clean(report: &Report) {
    assert!(report.clean(), "{}", report.describe());
}

/// Asserts the campaign actually did the work it claims to have done.
///
/// A run that generated no cuts, or cut nothing that could be checked, would
/// report a pass it did not earn — and would go on reporting one after cutting
/// was deleted entirely.
fn expect_coverage(report: &Report) {
    assert!(
        report.cutting > 0,
        "no instance produced a single cut:\n{}",
        report.describe()
    );
    assert!(
        report.cuts > 20,
        "only {} cuts were examined:\n{}",
        report.cuts,
        report.describe()
    );
    assert!(
        report.checks > 100,
        "only {} cut-and-point pairs were checked:\n{}",
        report.checks,
        report.describe()
    );
    assert!(
        report.answerable * 2 > report.cutting,
        "most cutting instances were beyond the enumerator:\n{}",
        report.describe()
    );
}

#[test]
fn no_cut_removes_a_feasible_integer_point() {
    let report = run(0xc07_0000_0001, PER_CHANGE);
    expect_clean(&report);
    expect_coverage(&report);
}

#[test]
fn a_second_stream_removes_none_either() {
    // One stream visits one region of the generator's space.
    let report = run(0xc07_0000_0002, PER_CHANGE / 2);
    expect_clean(&report);
    expect_coverage(&report);
}

#[test]
#[ignore = "the nightly campaign; run it by name or under the nightly job"]
fn the_nightly_campaign_removes_no_feasible_integer_point() {
    let report = run(0xc07_0000_0003, NIGHTLY);
    expect_clean(&report);
    expect_coverage(&report);
}
