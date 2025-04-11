//! The two LP readers against each other.
//!
//! Two readers, written separately, sharing no code and no data structure, are
//! given the same bytes; their canonical dumps are compared byte for byte, and
//! their refusals are compared identifier for identifier.
//!
//! The comparison is on the dumps rather than on the models, deliberately. A
//! comparison performed through either implementation's model type would be
//! comparing one of them against itself, and would pass for exactly the
//! disagreements it was built to find.
//!
//! This test lives on the solver's side of the boundary because the direction
//! of the dependency matters: a caller may use the verifier's public interface,
//! and the verifier may use nothing.

use hc_io::lp::read as read_solver;
use hc_verify::vlp::read as read_verifier;

const ACCEPTED: &[(&str, &[u8])] = &[
    (
        "tiny.lp",
        include_bytes!("../../../spec/v1/vectors/lp/tiny.lp"),
    ),
    (
        "ranges.lp",
        include_bytes!("../../../spec/v1/vectors/lp/ranges.lp"),
    ),
    (
        "exact.lp",
        include_bytes!("../../../spec/v1/vectors/lp/exact.lp"),
    ),
];

/// The pieces the shapes below are assembled from.
const HEAD: &str = "Minimize\n obj: 2 x - y\nSubject To\n";
const TAIL: &str = "End\n";

/// Compares the two readers on one file and reports what it found.
fn agree(name: &str, raw: &[u8]) {
    match (read_solver(raw), read_verifier(raw)) {
        (Ok(left), Ok(right)) => assert_eq!(
            left.canonical_dump(),
            right.dump(),
            "{name}: the two readers produced different canonical dumps"
        ),
        (Err(left), Err(right)) => assert_eq!(
            left.id.as_str(),
            right.code.text(),
            "{name}: refused for different reasons ({left} against {right})"
        ),
        (Ok(_), Err(right)) => {
            panic!("{name}: the solver accepted what the verifier refused ({right})")
        }
        (Err(left), Ok(_)) => {
            panic!("{name}: the verifier accepted what the solver refused ({left})")
        }
    }
}

#[test]
fn the_two_readers_agree_on_every_accepted_vector() {
    for (name, raw) in ACCEPTED {
        let solver = read_solver(raw).unwrap_or_else(|error| panic!("{name}: {error}"));
        let verifier = read_verifier(raw).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(solver.canonical_dump(), verifier.dump(), "{name}");
    }
}

#[test]
fn the_two_readers_agree_on_the_shapes_the_grammar_can_take() {
    // The grammar enumerated rather than sampled: every statement shape, every
    // spelling of every keyword, and the whitespace and comment placements
    // that are supposed to make no difference. Agreement is what is checked;
    // no particular answer is.
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();

    let bodies = [
        " c: x + y <= 4\n",
        " c: x + y >= 4\n",
        " c: x + y = 4\n",
        " c: 1 <= x + y <= 4\n",
        " c: 4 >= x + y >= 1\n",
        " c: 1 <= x + y >= 4\n",
        " c: 1 = x + y = 4\n",
        " c: -x <= -4\n",
        " c: 3x<=4\n",
        " c: 1.5e2 x <= 4\n",
        " c: 2e1 <= 4\n",
        " c: x + x + x <= 4\n",
        " c: x - x <= 4\n",
        " c: 0 x <= 4\n",
        " c: <= 4\n",
        " c: x <= \n",
        " c: x + 3 <= 4\n",
        " c: x + + y <= 4\n",
        " c: x y <= 4\n",
        " x + y <= 4\n",
        " c x + y <= 4\n",
        " c : x + y <= 4\n",
        " c: x\n  + y\n  <= 4\n",
        " c: x =< 4\n",
        " c: x < 4\n",
        " c: x > 4\n",
        " end: x <= 4\n",
        " c: x + free <= 4\n",
        " c: x , y <= 4\n",
    ];
    for body in bodies {
        cases.push((
            format!("body {body:?}"),
            format!("{HEAD}{body}{TAIL}").into_bytes(),
        ));
    }

    let bounds = [
        " x <= 9\n",
        " x >= -9\n",
        " x = 3\n",
        " x free\n",
        " -1 <= x <= 9\n",
        " 9 >= x >= -1\n",
        " -inf <= x <= 9\n",
        " x >= -INFINITY\n",
        " x <= +inf\n",
        " x <= -2\n",
        " x >= 1\n x >= 2\n",
        " x <= inf\n",
        " x >= +inf\n",
        " z >= 1\n",
        " 1 <= x\n",
        " x\n",
        " x <= 9 <= 10\n",
    ];
    for bound in bounds {
        cases.push((
            format!("bound {bound:?}"),
            format!("{HEAD} c: x + y <= 4\nBounds\n{bound}{TAIL}").into_bytes(),
        ));
    }

    let tails = [
        "General\n x\nEnd\n",
        "General\n x y\nEnd\n",
        "Binary\n x\nEnd\n",
        "General\n x\nBinary\n x\nEnd\n",
        "Binary\n z\nEnd\n",
        "Gen\n x\nBin\n y\nEnd\n",
        "Binary\n x,\nEnd\n",
        "Binary\nEnd\n",
        "End\n",
        "",
    ];
    for tail in tails {
        cases.push((
            format!("tail {tail:?}"),
            format!("{HEAD} c: x + y <= 4\n{tail}").into_bytes(),
        ));
    }

    let heads = [
        "Minimize\n obj: x\n",
        "MAXIMIZE\n obj: x\n",
        "min\n obj: x\n",
        "Maximum\n obj: x\n",
        "Minimize obj: x\n",
        "Minimize\n obj:\n",
        "Minimize\n obj: 5\n",
        "Minimize\n obj: x + 5 - 2\n",
        "Minimize\n x\n",
        "Minimize\n obj: x\nMaximize\n obj2: x\n",
        "\\ leading comment\nMinimize\n obj: x \\ tail\n",
        "\n\nMinimize\n\n obj: x\n\n",
        " obj: x\nMinimize\n obj2: x\n",
    ];
    for head in heads {
        cases.push((
            format!("head {head:?}"),
            format!("{head}Subject To\n c: x <= 4\nEnd\n").into_bytes(),
        ));
    }

    let whole = [
        ("empty", String::new()),
        ("only end", "End\n".to_owned()),
        ("no constraints", "Minimize\n obj: x\nEnd\n".to_owned()),
        ("no objective", "Subject To\n c: x <= 4\nEnd\n".to_owned()),
        (
            "sections reversed",
            "Subject To\n c: x <= 4\nMinimize\n obj: x\nEnd\n".to_owned(),
        ),
        (
            "bounds before rows",
            "Minimize\n obj: x\nBounds\n x <= 1\nSubject To\n c: x <= 4\nEnd\n".to_owned(),
        ),
        (
            "unsupported section",
            "Minimize\n obj: x\nSubject To\n c: x <= 4\nSOS\n s1\nEnd\n".to_owned(),
        ),
        (
            "content after the terminator",
            "Minimize\n obj: x\nSubject To\n c: x <= 4\nEnd\nleftovers\n".to_owned(),
        ),
        (
            "crossed row",
            "Minimize\n obj: x\nSubject To\n c: 9 <= x <= 1\nEnd\n".to_owned(),
        ),
        (
            "crossed bounds",
            "Minimize\n obj: x\nSubject To\n c: x <= 4\nBounds\n x >= 5\n x <= 2\nEnd\n".to_owned(),
        ),
        (
            // Written out rather than with a negative exponent, because a
            // token spelled that way is indistinguishable from a bare
            // tolerance to the scan that keeps those out of the workspace.
            "underflowing coefficient",
            format!(
                "Minimize\n obj: x\nSubject To\n c: 0.{}1 x <= 4\nEnd\n",
                "0".repeat(400)
            ),
        ),
        (
            "overflowing coefficient",
            "Minimize\n obj: x\nSubject To\n c: 1e400 x <= 4\nEnd\n".to_owned(),
        ),
        (
            "coefficient past the width cap",
            "Minimize\n obj: x\nSubject To\n c: 1.0e99999999 x <= 4\nEnd\n".to_owned(),
        ),
        (
            "not a number",
            "Minimize\n obj: x\nSubject To\n c: 1.2.3 x <= 4\nEnd\n".to_owned(),
        ),
    ];
    for (name, text) in whole {
        cases.push((format!("whole {name}"), text.into_bytes()));
    }

    for (name, raw) in &cases {
        agree(name, raw);
    }
    assert!(cases.len() >= 70, "the sweep shrank: {}", cases.len());
}

#[test]
fn the_two_readers_agree_on_every_single_byte_truncation_of_a_vector() {
    // Truncation is what a partial write and a bad download look like, and it
    // reaches shapes no hand-written case does: a statement cut mid-token, a
    // section header cut in half, a file ending inside a comment.
    for (name, raw) in ACCEPTED {
        for cut in 0..raw.len() {
            agree(&format!("{name} truncated to {cut}"), &raw[..cut]);
        }
    }
}

#[test]
fn the_two_readers_agree_on_every_single_byte_edit_of_a_vector() {
    // One byte at a time, replaced by each of a handful of characters that
    // change the grammar rather than the arithmetic. Every result is a file,
    // and the two readers have to answer the same way about all of them.
    const EDITS: &[u8] = b" :,-+<>=\\.09xE\n";
    for (name, raw) in ACCEPTED {
        for at in 0..raw.len() {
            for edit in EDITS {
                let mut mutated = raw.to_vec();
                mutated[at] = *edit;
                agree(&format!("{name} byte {at} as {}", *edit as char), &mutated);
            }
        }
    }
}
