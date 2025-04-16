//! The budget a change pays before it lands, and the one the nightly pays.
//!
//! Both run the same targets over the same generators; the only difference is
//! how many seeds they walk. That matters more than it looks: a nightly run
//! that exercised different code from the per-change run would be a nightly
//! run whose findings nobody could reproduce locally.

use hc_fuzz::edits::{Edit, apply};
use hc_fuzz::shapes::{Shape, draft, render};
use hc_fuzz::{Budget, parsers, proofs, solver};

#[test]
fn the_parser_targets_are_clean_at_the_budget_a_change_pays() {
    let report = parsers::campaign(Budget::PER_CHANGE);
    assert!(report.cases > 0, "the campaign examined nothing");
    // Both halves have to be non-empty. A corpus that is refused entirely is
    // testing the first fence over and over, and one that is accepted entirely
    // is not testing the fences at all.
    assert!(report.accepted > 0, "nothing in the corpus was accepted");
    assert!(report.refused > 0, "nothing in the corpus was refused");
    assert!(report.clean(), "{}", render_findings(&report));
}

#[test]
fn the_proof_target_is_clean_at_the_budget_a_change_pays() {
    let report = proofs::campaign(Budget { from: 1, cases: 40 });
    assert!(report.accepted > 0, "no certificate was assembled at all");
    assert!(report.clean(), "{}", render_findings(&report));
}

#[test]
fn the_solver_target_is_clean_at_the_budget_a_change_pays() {
    // A smaller budget in seeds than the parser target pays, because each case
    // here is a solve rather than a parse: two searches, a reduction, two
    // certificates, and two independent verifications.
    let report = solver::campaign(Budget { from: 1, cases: 60 });
    assert!(report.accepted > 0, "nothing was solved at all");
    assert!(report.clean(), "{}", render_findings(&report));
}

#[test]
#[ignore = "the nightly budget; run it with --ignored"]
fn the_targets_are_clean_at_the_budget_the_nightly_pays() {
    let mut report = parsers::campaign(Budget::NIGHTLY);
    report.absorb(proofs::campaign(Budget {
        from: 1,
        cases: 2_000,
    }));
    report.absorb(solver::campaign(Budget {
        from: 1,
        cases: 4_000,
    }));
    assert!(report.clean(), "{}", render_findings(&report));
}

#[test]
fn a_finding_names_the_seed_that_reproduces_it() {
    // The property that makes a case file possible: every case comes from its
    // seed and nothing else, so the same seed produces the same bytes on any
    // machine, in any order, at any budget.
    for seed in [1u64, 2, 99, 100_000] {
        let first = render(&draft(seed), Shape::Lp, seed);
        let second = render(&draft(seed), Shape::Lp, seed);
        assert_eq!(first, second, "seed {seed} rendered differently twice");
        for edit in Edit::ALL {
            assert_eq!(
                apply(&first, edit, seed),
                apply(&second, edit, seed),
                "seed {seed} mutated differently twice"
            );
        }
    }

    // And a narrow run over one seed sees exactly what the wide run saw at
    // that seed, so a finding can be replayed without replaying everything
    // before it.
    let narrow = parsers::campaign(Budget { from: 7, cases: 1 });
    let wide = parsers::campaign(Budget { from: 7, cases: 3 });
    assert_eq!(narrow.findings, Vec::new());
    assert!(wide.cases > narrow.cases);
}

#[test]
fn the_generators_reach_every_reading_and_every_mutation() {
    // A generator that quietly stopped emitting one of the four readings, or a
    // mutation that stopped changing anything, would leave a region unfuzzed
    // while the campaign still reported success.
    let drafted = draft(12345);
    let mut seen = Vec::new();
    for shape in Shape::ALL {
        let text = render(&drafted, shape, 12345);
        assert!(!text.is_empty(), "{} rendered nothing", shape.as_str());
        assert!(
            !seen.contains(&text),
            "two readings rendered the same bytes"
        );
        seen.push(text);
    }

    let text = render(&drafted, Shape::Lp, 12345);
    for edit in Edit::ALL {
        let mutated = apply(&text, edit, 3);
        if edit == Edit::None {
            assert_eq!(mutated, text);
        } else {
            assert_ne!(mutated, text, "{} changed nothing", edit.as_str());
        }
    }
}

fn render_findings(report: &hc_fuzz::Report) -> String {
    let mut out = format!("{}\n", report.line());
    for finding in &report.findings {
        out.push_str(&finding.to_string());
    }
    out
}
