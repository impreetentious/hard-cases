//! Every claim the solver certifies, rechecked by something that shares none
//! of its code.
//!
//! This is the gate the certificate machinery exists for. Thousands of
//! generated instances are solved, rebuilt exactly, and handed with their
//! original bytes to the standalone verifier. Two things must hold and the
//! second is the one that catches a subtle solver defect: every certificate
//! must be accepted, and every accepted claim must agree with a dense
//! reference solver that shares no code with the fast one either.
//!
//! The shape of what the campaign found is asserted as well as the absence of
//! findings, because a campaign that certified nothing would otherwise report a
//! perfect record.

use hc_bench::proof::{Report, run};

/// The per-change campaign size, from the normative constants.
const PER_CHANGE: usize = 2_000;

/// The nightly campaign size.
const NIGHTLY: usize = 100_000;

fn expect_clean(report: &Report) {
    assert!(
        report.clean(),
        "the certification campaign found something\n{}",
        report.describe()
    );
}

#[test]
fn every_certificate_the_campaign_produces_is_accepted_and_agrees_with_the_oracle() {
    let report = run(0x00c0_ffee_0000_0001, PER_CHANGE);
    expect_clean(&report);

    // A campaign that certified nothing would be clean and worthless.
    assert!(
        report.accepted() > PER_CHANGE / 4,
        "only {} of {} rounds certified\n{}",
        report.accepted(),
        report.rounds,
        report.describe()
    );

    // And one that only ever certified optimality would leave the other two
    // definite claims untested.
    for claim in ["optimal", "infeasible", "unbounded"] {
        assert!(
            report.certified.get(claim).copied().unwrap_or(0) > 0,
            "the campaign never certified {claim}\n{}",
            report.describe()
        );
    }

    // Declines are tallied by reason and not by message, so that a hundred
    // declines for one cause read as one cause. A tally that exploded into
    // hundreds of entries would mean the key had drifted back to the detail.
    assert!(
        report.declined.len() < 12,
        "the decline tally has {} distinct reasons, which suggests it is keyed by detail\n{}",
        report.declined.len(),
        report.describe()
    );
}

#[test]
fn the_campaign_is_deterministic_down_to_its_work_count() {
    let first = run(0x5eed_0000_0000_002a, 200);
    let second = run(0x5eed_0000_0000_002a, 200);
    assert_eq!(first.certified, second.certified);
    assert_eq!(first.declined, second.declined);
    assert_eq!(
        first.work, second.work,
        "the work count must be reproducible"
    );
    assert_eq!(first.widest_rational, second.widest_rational);
    assert_eq!(first.widest_certificate, second.widest_certificate);
}

#[test]
fn certificates_and_the_numbers_inside_them_stay_far_inside_their_caps() {
    let report = run(0x00c0_ffee_0000_0002, 500);
    expect_clean(&report);
    // The published caps are enormous compared with anything a generated
    // instance produces. Checking against them here is not a size test; it is a
    // check that the reported figures are being computed at all, since a
    // measurement stuck at zero would pass every cap forever.
    assert!(report.widest_certificate > 0);
    assert!(report.widest_rational > 0);
    assert!(report.work > 0);
    assert!(report.widest_rational < hc_verify::vhcc::RATIONAL_BIT_CAP);
    assert!((report.widest_certificate as u64) < hc_verify::vhcc::BYTE_CAP);
}

/// The nightly size. Ignored in the ordinary suite so that a preflight does
/// not pay for it, and run with `--ignored` by the `check` workflow.
#[test]
#[ignore = "nightly size; run with --ignored"]
fn the_nightly_certification_campaign_is_clean() {
    let report = run(0x00c0_ffee_0000_0003, NIGHTLY);
    expect_clean(&report);
    assert!(report.accepted() > NIGHTLY / 4, "{}", report.describe());
}
