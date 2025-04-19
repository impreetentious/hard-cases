//! The scoreboard is regenerated and diffed on every run of the suite.
//!
//! A published board that nothing checks is a board that was true once. This
//! runs the same corpora the committed document reports on and requires the
//! result to be the document, byte for byte — so a change to the solver that
//! moves an iteration count is a change that updates the board in the same
//! commit, and a board edited by hand fails immediately.
//!
//! It is also, incidentally, a determinism check with a long memory: the
//! counts in the committed file were produced by a run on another machine on
//! another day.

use std::fs;
use std::path::PathBuf;

use hc_bench::figure;
use hc_bench::scoreboard::{COUNTERS_BEGIN, COUNTERS_END, counters, render};

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn committed() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("scoreboard.md");
    fs::read_to_string(path).expect("the committed scoreboard")
}

#[test]
fn the_committed_board_is_what_the_corpora_produce_now() {
    let fresh = render();
    let held = committed();
    if fresh != held {
        let mut differences = Vec::new();
        for (at, (a, b)) in fresh.lines().zip(held.lines()).enumerate() {
            if a != b {
                differences.push(format!("line {}:\n  fresh: {a}\n  held:  {b}", at + 1));
            }
        }
        if fresh.lines().count() != held.lines().count() {
            differences.push(format!(
                "the board has {} lines and the committed file has {}",
                fresh.lines().count(),
                held.lines().count()
            ));
        }
        panic!(
            "docs/scoreboard.md is stale. Regenerate with\n  \
             cargo run -p hc-bench --bin scoreboard\n\n{}",
            differences.join("\n")
        );
    }
}

#[test]
fn the_board_reports_no_finding_and_no_unverified_claim() {
    // The two sentences a reader should be able to check without reading the
    // table: nothing disagreed, and nothing claimed a status without a
    // certificate an independent program accepted.
    let board = render();
    assert!(
        board.contains("Findings: 0."),
        "the linear corpus reports a finding"
    );
    assert!(
        !board.contains("**no certificate**"),
        "a mixed-integer instance reached a status without a certificate"
    );
    assert!(
        !board.contains("**disagrees**"),
        "enumeration and the search disagree about an instance"
    );
    assert!(
        board.contains("Certificates the independent verifier accepted: 4 of 4"),
        "the verified count is not what the corpus says it should be"
    );
}

#[test]
fn the_front_page_counters_are_what_the_repository_measures_now() {
    // The README is written by hand everywhere except between these two
    // markers. Those numbers are claims, and a claim on a front page that
    // nothing regenerates is a claim that was true once.
    let readme = fs::read_to_string(repository().join("README.md")).expect("the front page");
    let fresh = counters(&repository().join("crates").join("hc-verify"));
    let from = readme
        .find(COUNTERS_BEGIN)
        .expect("README.md has no opening counter marker");
    let to = readme
        .find(COUNTERS_END)
        .expect("README.md has no closing counter marker");
    let held = &readme[from..to + COUNTERS_END.len()];
    assert_eq!(
        held, fresh,
        "the front page's counters are stale. Regenerate with\n  cargo run -p hc-bench --bin scoreboard"
    );
}

#[test]
fn the_committed_figure_is_the_tree_the_solver_produces_now() {
    // The front page's picture is a solve, not a drawing. Regenerating it here
    // means a change to branching or to separation that changes the tree fails
    // the suite until the figure is redrawn — which is the only way a picture
    // on a front page stays true.
    let fresh = figure::render();
    let held = fs::read_to_string(repository().join("docs").join("proof-tree.svg"))
        .expect("the committed figure");
    assert_eq!(
        fresh, held,
        "docs/proof-tree.svg is stale. Regenerate with\n  cargo run -p hc-bench --bin scoreboard"
    );
    // And it is a picture of something that verified.
    assert!(fresh.contains("claim optimal"));
    assert!(fresh.contains("by hc-verify, which links none of the solver"));
}
