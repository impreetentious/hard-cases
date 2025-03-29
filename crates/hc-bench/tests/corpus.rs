//! The committed corpus, run by both solvers on every preflight.
//!
//! The manifest is walked rather than transcribed. An instance named there and
//! not compiled in here fails, and an instance compiled in here and not named
//! there fails too — because a corpus entry nothing runs is a file that looks
//! like coverage and is not.
//!
//! This is the gate that consumes the corpus. Its existence is the point: a
//! pinned set of instance files that no gate reads proves only that somebody
//! once intended to test something.

use hc_bench::corpus::{Board, Expected, run};
use hc_core::SpecDocument;

const MANIFEST: &str = include_str!("../../../instances/local/manifest.toml");

const FILES: &[(&str, &[u8])] = &[
    (
        "transport33.mps",
        include_bytes!("../../../instances/local/transport33.mps"),
    ),
    (
        "blend4.mps",
        include_bytes!("../../../instances/local/blend4.mps"),
    ),
    (
        "cover6.mps",
        include_bytes!("../../../instances/local/cover6.mps"),
    ),
    (
        "flow8.mps",
        include_bytes!("../../../instances/local/flow8.mps"),
    ),
    (
        "freevars5.mps",
        include_bytes!("../../../instances/local/freevars5.mps"),
    ),
    (
        "cycle7.mps",
        include_bytes!("../../../instances/local/cycle7.mps"),
    ),
    (
        "infeasible4.mps",
        include_bytes!("../../../instances/local/infeasible4.mps"),
    ),
    (
        "unbounded3.mps",
        include_bytes!("../../../instances/local/unbounded3.mps"),
    ),
];

fn manifest() -> SpecDocument {
    SpecDocument::parse(MANIFEST).expect("the corpus manifest is inside the subset")
}

/// The corpus as the runner wants it, in manifest order.
fn entries() -> Vec<(&'static str, &'static [u8], Option<Expected>)> {
    let document = manifest();
    let mut out = Vec::new();
    for table in document.tables_under("instance") {
        let file = table
            .text("file")
            .unwrap_or_else(|| panic!("[{}] names no file", table.name()));
        let raw = FILES
            .iter()
            .find(|(name, _)| *name == file)
            .map(|(_, raw)| *raw)
            .unwrap_or_else(|| panic!("[{}] names {file}, which is not compiled in", table.name()));
        let expect = table
            .text("expect")
            .and_then(Expected::named)
            .unwrap_or_else(|| panic!("[{}] states no recognisable expectation", table.name()));
        let name = FILES
            .iter()
            .find(|(name, _)| *name == file)
            .expect("present")
            .0;
        out.push((name, raw, Some(expect)));
    }
    out
}

fn board() -> Board {
    run(&entries())
}

#[test]
fn every_instance_reaches_the_status_the_manifest_names() {
    let board = board();
    let findings = board.findings();
    assert!(findings.is_empty(), "{}", board.render());
    assert_eq!(board.rows.len(), FILES.len());
}

#[test]
fn the_manifest_and_the_compiled_corpus_are_the_same_set() {
    let document = manifest();
    let named: Vec<&str> = document
        .tables_under("instance")
        .map(|table| table.text("file").expect("a file"))
        .collect();
    for (compiled, _) in FILES {
        assert!(
            named.contains(compiled),
            "{compiled} is compiled in and the manifest does not name it"
        );
    }
    assert_eq!(named.len(), FILES.len());
    // Every entry states why it is in the corpus. An instance nobody can
    // explain is an instance nobody will maintain.
    for table in document.tables_under("instance") {
        assert!(
            table.text("covers").is_some(),
            "[{}] states no rationale",
            table.name()
        );
    }
}

#[test]
fn the_corpus_covers_more_than_one_kind_of_answer() {
    // A corpus of eight instances that all end the same way tests one path
    // eight times.
    let board = board();
    let tally = board.tally();
    assert!(
        tally.get("optimal").copied().unwrap_or(0) >= 5,
        "{}",
        board.render()
    );
    assert_eq!(
        tally.get("infeasible").copied(),
        Some(1),
        "{}",
        board.render()
    );
    assert_eq!(
        tally.get("unbounded").copied(),
        Some(1),
        "{}",
        board.render()
    );
}

#[test]
fn the_structured_instances_take_real_work() {
    // A corpus the search walks through in one pivot each is a corpus that
    // exercises the parser and nothing else.
    let board = board();
    let total: u64 = board.rows.iter().map(|row| row.iterations).sum();
    assert!(
        total >= 20,
        "the whole corpus took {total} iterations:\n{}",
        board.render()
    );
}

#[test]
fn the_board_is_reproducible() {
    let first = board();
    let second = board();
    for (left, right) in first.rows.iter().zip(&second.rows) {
        assert_eq!(left.name, right.name);
        assert_eq!(left.solver, right.solver);
        assert_eq!(left.iterations, right.iterations);
        assert_eq!(left.objective, right.objective);
    }
}
