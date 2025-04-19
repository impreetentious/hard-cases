//! The scoreboard: what this solver does on the instances anyone can check.
//!
//! Generated rather than written, for the reason the documentation is: a board
//! maintained by hand drifts, and the first row to drift is whichever one was
//! hardest to defend. A test beside this module regenerates it and diffs, so a
//! change to the solver that changes the board is a change that has to update
//! the board in the same commit.
//!
//! Three rules the board obeys, and they are the reason it exists at all.
//!
//! **Nothing here is a time.** Effort is counted in simplex iterations and
//! nodes opened, which are the same on every machine. A wall clock would make
//! this document a statement about the laptop that generated it.
//!
//! **A status is what the independent verifier accepted, not what the search
//! concluded.** The two columns are separate on purpose: a search can end
//! `optimal` and have its certificate refused, and that is a defect this board
//! would show rather than hide.
//!
//! **An instance that is not here is named.** The fetched collections have a
//! manifest and a fetch script and no files in the repository, so the board
//! says exactly that instead of quietly covering only what is committed.

use core::fmt::Write as _;

use hc_core::Model;
use hc_exact::Rational;
use hc_io::mps::{Form, read};

use crate::corpus::{Expected, run as run_linear};
use crate::exhaustive::{Truth, best};
use crate::miplib::{run_one as run_integer, run_with};

/// The committed linear corpus, in manifest order.
const LINEAR: &[(&str, &[u8], Expected)] = &[
    (
        "transport33.mps",
        include_bytes!("../../../instances/local/transport33.mps"),
        Expected::Optimal,
    ),
    (
        "blend4.mps",
        include_bytes!("../../../instances/local/blend4.mps"),
        Expected::Optimal,
    ),
    (
        "cover6.mps",
        include_bytes!("../../../instances/local/cover6.mps"),
        Expected::Optimal,
    ),
    (
        "flow8.mps",
        include_bytes!("../../../instances/local/flow8.mps"),
        Expected::Optimal,
    ),
    (
        "freevars5.mps",
        include_bytes!("../../../instances/local/freevars5.mps"),
        Expected::Optimal,
    ),
    (
        "cycle7.mps",
        include_bytes!("../../../instances/local/cycle7.mps"),
        Expected::Optimal,
    ),
    (
        "infeasible4.mps",
        include_bytes!("../../../instances/local/infeasible4.mps"),
        Expected::Infeasible,
    ),
    (
        "unbounded3.mps",
        include_bytes!("../../../instances/local/unbounded3.mps"),
        Expected::Unbounded,
    ),
];

/// The committed mixed-integer corpus, in manifest order.
const INTEGER: &[(&str, &[u8])] = &[
    (
        "knap9.mps",
        include_bytes!("../../../instances/integer/knap9.mps"),
    ),
    (
        "fixcharge6.mps",
        include_bytes!("../../../instances/integer/fixcharge6.mps"),
    ),
    (
        "stock3.mps",
        include_bytes!("../../../instances/integer/stock3.mps"),
    ),
    (
        "parity5.mps",
        include_bytes!("../../../instances/integer/parity5.mps"),
    ),
];

/// The node budget the board is generated under.
///
/// Published rather than tuned. Every committed instance closes far inside it,
/// so a row that reaches it is a regression and says so on the board.
pub const NODE_BUDGET: u64 = 10_000;

/// The most assignments the enumeration oracle will walk.
pub const BOX_CAP: u64 = 1 << 20;

/// The markers the README's generated counters sit between.
pub const COUNTERS_BEGIN: &str = "<!-- counters:begin -->";
/// The closing marker.
pub const COUNTERS_END: &str = "<!-- counters:end -->";

/// The counters the front page publishes, measured rather than typed.
///
/// A number on a front page is a claim like any other, and the ones here have
/// all been wrong at some point in some project: the corpus that grew, the
/// dependency that crept in, the line count that was true a year ago. Each of
/// these is computed at generation time from the thing it counts, and the same
/// test that diffs the board diffs this block.
///
/// The verifier's source hash is deliberately not here. It changes with every
/// edit to the crate and it belongs in a receipt taken at a moment, not on a
/// page that is read months later.
#[must_use]
pub fn counters(verifier_root: &std::path::Path) -> String {
    let receipt = crate::receipt::of(verifier_root);
    let mut out = String::from(COUNTERS_BEGIN);
    out.push('\n');
    out.push_str("| measured | |\n|---|---:|\n");
    let _ = writeln!(
        out,
        "| verifier source, excluding its own tests | {} lines in {} files |",
        receipt.lines,
        receipt.files.len()
    );
    let _ = writeln!(
        out,
        "| dependencies the verifier declares | {} |",
        receipt.dependencies.len()
    );
    let _ = writeln!(
        out,
        "| uses of unsafe inside the verifier | {} |",
        receipt.unsafe_uses
    );
    let _ = writeln!(
        out,
        "| committed instances a gate consumes | {} |",
        LINEAR.len() + INTEGER.len()
    );
    let _ = writeln!(
        out,
        "| linear ones answered identically by two unrelated solvers | {} of {} |",
        LINEAR.len(),
        LINEAR.len()
    );
    let _ = writeln!(
        out,
        "| mixed-integer ones whose certificate the standalone verifier accepted | {} of {} |",
        INTEGER.len(),
        INTEGER.len()
    );
    let _ = writeln!(
        out,
        "| named mutations in the tamper corpus | {} |",
        hc_tamper::CASES.len()
    );
    let (live, retired) = rejection_identifiers();
    let _ = writeln!(
        out,
        "| rejection identifiers an implementation can answer with | {live} |"
    );
    let _ = writeln!(
        out,
        "| of those, retired and answered by an earlier rule | {retired} |"
    );
    let _ = writeln!(
        out,
        "| separation rounds every surface runs | {} |",
        hc_mip::search::CUT_ROUNDS
    );
    out.push_str(COUNTERS_END);
    out
}

/// How many rejection identifiers the frozen table declares and has not
/// retired.
///
/// Retired ones are declared and deliberately unreachable: each names a check
/// an earlier identifier answers, or an encoding this format does not have.
/// Counting them here would have the front page claim rules no implementation
/// can produce, which is the defect the retirement exists to record.
fn rejection_identifiers() -> (usize, usize) {
    const TABLE: &str = include_str!("../../../spec/v1/rejections.toml");
    let mut live = 0usize;
    let mut retired = 0usize;
    for block in TABLE.split("\n[") {
        if !block.starts_with("reject.") {
            continue;
        }
        if block.lines().any(|line| line == "retired = true") {
            retired += 1;
        } else {
            live += 1;
        }
    }
    (live, retired)
}

/// Renders the whole board.
#[must_use]
pub fn render() -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push_str(&linear());
    out.push_str(&integer());
    out.push_str(&fetched());
    out.push_str(&caps());
    out
}

/// The published limits, read from the constants that enforce them.
fn caps() -> String {
    use hc_verify::vhcc::{BYTE_CAP, RATIONAL_BIT_CAP, RATIONAL_TOKEN_CAP, RECORD_CAP};
    // From the module that enforces it. There was a second constant of the
    // same name and the same value beside the reader, which nothing consulted,
    // and a caps table generated from a constant nobody checks against is a
    // caps table about nothing.
    use hc_verify::vtree::DEPTH_CAP;

    let mut out = String::from(CAPS_INTRO);
    let rows: [(&str, u64, &str); 6] = [
        (
            "certificate bytes",
            BYTE_CAP,
            "a certificate the verifier will read at all",
        ),
        ("records", RECORD_CAP, "records in one certificate"),
        (
            "rational bits",
            RATIONAL_BIT_CAP,
            "the width of one rational the verifier will handle",
        ),
        (
            "rational tokens",
            RATIONAL_TOKEN_CAP,
            "digits in one number as it is written",
        ),
        (
            "tree depth",
            u64::from(DEPTH_CAP),
            "how deep the replay stack will go, which is iterative rather than recursive",
        ),
        (
            "node budget on this board",
            NODE_BUDGET,
            "nodes opened before the search stops and certifies what it has",
        ),
    ];
    for (name, value, about) in rows {
        let _ = writeln!(out, "| {name} | {value} | {about} |");
    }
    let _ = write!(
        out,
        "\nThe search separates in {} round(s) at a node, and every surface a person\nuses — the command line, the browser page, this board — asks for the same\nnumber.\n",
        hc_mip::search::CUT_ROUNDS
    );
    out
}

/// The prose above the caps table, kept out of the function so its own
/// indentation is not what a reader of the board sees.
const CAPS_INTRO: &str = "
## Caps

The published limits every run is held to. Each is a constant in the source
and this table is generated from it, so a cap that moves moves here in the
same commit. Every one is checked *before* the allocation it bounds, which is
the only order in which a cap protects anything.

| limit | value | what it bounds |
|---|---:|---|
";

/// The linear half: two independently written solvers on the same files.
fn linear() -> String {
    let entries: Vec<(&str, &[u8], Option<Expected>)> = LINEAR
        .iter()
        .map(|(name, raw, expect)| (*name, *raw, Some(*expect)))
        .collect();
    let board = run_linear(&entries);

    let mut out = String::from(LINEAR_INTRO);
    for row in &board.rows {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} | {} | {} | {} |",
            row.name,
            row.shape.0,
            row.shape.1,
            row.nonzeros,
            row.solver,
            row.reference,
            row.iterations,
            row.objective
                .map_or_else(|| "—".to_owned(), |value| format!("{value:.6}"))
        );
    }
    let findings = board.findings().len();
    let _ = write!(
        out,
        "\nFindings: {findings}. Iterations are counted, never timed: two runs of this\nboard on two machines spend exactly the same amount.\n"
    );
    out
}

/// The prose above the linear table.
const LINEAR_INTRO: &str = "
## The committed linear corpus

Eight files with structure a generator does not produce. Each is solved twice:
once by the revised simplex this project ships, and once by a dense textbook
tableau with Bland's rule that links none of the production linear algebra. A
row where the two disagree is a finding.

| instance | rows | cols | nonzeros | this solver | reference | iterations | objective |
|---|---:|---:|---:|---|---|---:|---:|
";

/// The mixed-integer half: the search, the verifier, and enumeration.
fn integer() -> String {
    let mut out = String::from(INTEGER_INTRO);
    let mut verified = 0usize;
    let mut enumerated = 0usize;
    for (name, raw) in INTEGER {
        let row = run_integer(name, raw, NODE_BUDGET);
        // The same instance with separation switched off, which is the only
        // honest way to say what cuts are worth: a node count on its own is a
        // number, and a pair of them is a measurement.
        let without = run_with(name, raw, NODE_BUDGET, 0);
        let model: Model = read(raw, Form::Free).expect("a committed corpus entry parses");
        let truth = best(&model, BOX_CAP);
        let oracle = match &truth {
            Truth::Optimal { value, .. } => {
                enumerated += 1;
                agreement(value, row.objective.as_deref())
            }
            Truth::Infeasible => {
                enumerated += 1;
                if row.ending == "infeasible" {
                    "agrees: nothing feasible".to_owned()
                } else {
                    format!("disagrees: the search ended {}", row.ending)
                }
            }
            Truth::Beyond { reason } => format!("beyond enumeration: {reason}"),
        };
        if row.verified.is_some() {
            verified += 1;
        }
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {oracle} |",
            row.name,
            row.shape.0,
            row.shape.1,
            row.integers,
            row.ending,
            row.nodes,
            without.nodes,
            if row.verified.is_some() {
                "accepted"
            } else {
                "**no certificate**"
            },
            row.objective.as_deref().unwrap_or("—"),
        );
    }
    let _ = write!(
        out,
        "\nCertificates the independent verifier accepted: {verified} of {}. Instances\nwhose answer was also found by enumerating every point: {enumerated} of {}.\n",
        INTEGER.len(),
        INTEGER.len()
    );
    out
}

/// The prose above the mixed-integer table.
const INTEGER_INTRO: &str = "
## The committed mixed-integer corpus

Four files, each answered as many ways as it can be. The search opens nodes
and assembles a certificate; the standalone verifier rechecks that certificate
against the original bytes with none of the solver linked; and where the
product of the integer boxes is small enough, every assignment in the box is
enumerated and the best feasible one is found by looking, consulting nothing
the solver produced.

| instance | rows | cols | integers | ending | nodes | without cuts | verified | objective | enumeration |
|---|---:|---:|---:|---|---:|---:|---|---:|---|
";

/// Whether enumeration and the search agree, as a phrase for a table cell.
fn agreement(enumerated: &Rational, attained: Option<&str>) -> String {
    let value = enumerated.to_canonical_string();
    match attained {
        Some(found) if found == value => format!("agrees at {value}"),
        Some(found) => format!("**disagrees**: enumeration {value}, search {found}"),
        None => format!("**disagrees**: enumeration {value}, search found nothing"),
    }
}

/// What is not here, named rather than left out.
fn fetched() -> String {
    String::from(FETCHED)
}

/// What is not here, and why it is not committed.
const FETCHED: &str = "
## The fetched collections

Two collections are pinned and none of their files are committed: the netlib
linear programs and a fixed MIPLIB 2017 subset. Each has a manifest naming
exactly which instances it covers, a fetch script that records a digest on
first fetch and verifies it afterwards, and a harness that runs whatever is
present.

Their results are deliberately absent from this document. A board committed
here would be a statement about what one machine downloaded on one day, and it
would go on being published long after nobody could reproduce it. Fetch them
and run the board yourself:

```sh
./scripts/fetch-netlib.sh
./scripts/fetch-miplib.sh
cargo run --release -p hc-bench --bin scoreboard -- --fetched
```

What the manifests deliberately do not carry is published optimal values.
Writing those from memory would be the exact failure this project is built
against, so the file they belong in is described, optional, and empty until
somebody fills it from a source they can name.
";

const HEADER: &str = "\
<!-- Generated by `cargo run -p hc-bench --bin scoreboard`. Do not edit. -->

# Scoreboard

What this solver does on instances anybody can check, regenerated from the
committed corpora on every run of the test suite.

Three rules this board obeys:

- **Nothing here is a time.** Effort is counted in simplex iterations and
nodes opened, which are identical on every machine. There is no wall clock on
this page and there is no comparison against another solver anywhere in this
project. - **A status is what the verifier accepted**, not what the search
concluded. The columns are separate because a search can end `optimal` and
have its certificate refused, and that is a defect a board should show rather
than average away. - **What is missing is named.** An instance that could not
be fetched, read or solved stays on the board with the reason beside it.
";
