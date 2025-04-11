//! The two JSON readers against each other.
//!
//! One builds a value tree and walks it; the other lexes the document into a
//! flat sequence of events and interprets the sequence with an index. They are
//! given the same bytes; their canonical dumps are compared byte for byte, and
//! their refusals are compared identifier for identifier.
//!
//! The comparison is on the dumps rather than on the models, deliberately. A
//! comparison performed through either implementation's model type would be
//! comparing one of them against itself, and would pass for exactly the
//! disagreements it was built to find.

use hc_io::json::read as read_solver;
use hc_verify::vjson::read as read_verifier;

const ACCEPTED: &[(&str, &[u8])] = &[
    (
        "tiny.json",
        include_bytes!("../../../spec/v1/vectors/json/tiny.json"),
    ),
    (
        "ranges.json",
        include_bytes!("../../../spec/v1/vectors/json/ranges.json"),
    ),
    (
        "exact.json",
        include_bytes!("../../../spec/v1/vectors/json/exact.json"),
    ),
];

/// The document the shapes below are edits of.
const TINY: &str = concat!(
    "{\"version\": 1, \"sense\": \"min\", \"objective_constant\": \"0\", ",
    "\"vars\": [{\"name\": \"x\", \"lb\": \"0\", \"ub\": \"10\", \"integer\": true, \"obj\": \"-1\"}], ",
    "\"rows\": [{\"name\": \"c1\", \"sense\": \"L\", \"rhs\": \"4\", \"range\": null}], ",
    "\"nonzeros\": [[0, 0, \"1\"]]}"
);

/// Compares the two readers on one document and reports what it found.
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
fn the_two_readers_agree_on_the_shapes_the_schema_can_take() {
    // The schema enumerated rather than sampled: every member replaced by
    // every kind of value the grammar admits, and the document shapes a
    // forgiving reader would repair. Agreement is what is checked; no
    // particular answer is.
    let mut cases: Vec<(String, String)> = Vec::new();
    let mut add = |name: &str, text: String| cases.push((name.to_owned(), text));

    // Every member of the document, given each wrong kind and then removed.
    for member in [
        "\"version\": 1",
        "\"sense\": \"min\"",
        "\"objective_constant\": \"0\"",
        "\"lb\": \"0\"",
        "\"ub\": \"10\"",
        "\"integer\": true",
        "\"obj\": \"-1\"",
        "\"sense\": \"L\"",
        "\"rhs\": \"4\"",
        "\"range\": null",
    ] {
        let key = member.split(':').next().expect("a member name");
        for replacement in [
            format!("{key}: null"),
            format!("{key}: 1"),
            format!("{key}: true"),
            format!("{key}: \"\""),
            format!("{key}: []"),
            format!("{key}: {{}}"),
            format!("{key}: \"nope\""),
        ] {
            add(
                &format!("{member} as {replacement}"),
                TINY.replace(member, &replacement),
            );
        }
        add(
            &format!("{member} removed"),
            TINY.replace(&format!("{member}, "), "")
                .replace(&format!(", {member}"), ""),
        );
    }

    // The whole-document shapes.
    for (name, text) in [
        ("empty", String::new()),
        ("blank", "   ".to_owned()),
        ("a bare array", "[]".to_owned()),
        ("a bare number", "1".to_owned()),
        ("an empty object", "{}".to_owned()),
        ("no variables", TINY.replace("\"vars\": [{\"name\": \"x\", \"lb\": \"0\", \"ub\": \"10\", \"integer\": true, \"obj\": \"-1\"}]", "\"vars\": []")),
        ("no rows", TINY.replace("\"rows\": [{\"name\": \"c1\", \"sense\": \"L\", \"rhs\": \"4\", \"range\": null}]", "\"rows\": []")),
        ("no coefficients", TINY.replace("[[0, 0, \"1\"]]", "[]")),
        ("a short coefficient", TINY.replace("[0, 0, \"1\"]", "[0, 0]")),
        ("a long coefficient", TINY.replace("[0, 0, \"1\"]", "[0, 0, \"1\", \"1\"]")),
        ("a coefficient of numbers", TINY.replace("[0, 0, \"1\"]", "[0, 0, 1]")),
        ("a negative index", TINY.replace("[0, 0, \"1\"]", "[-1, 0, \"1\"]")),
        ("a row index past the end", TINY.replace("[0, 0, \"1\"]", "[1, 0, \"1\"]")),
        ("a column index past the end", TINY.replace("[0, 0, \"1\"]", "[0, 1, \"1\"]")),
        ("summing coefficients", TINY.replace("[[0, 0, \"1\"]]", "[[0, 0, \"3\"], [0, 0, \"-2\"]]")),
        ("cancelling coefficients", TINY.replace("[[0, 0, \"1\"]]", "[[0, 0, \"3\"], [0, 0, \"-3\"]]")),
        ("a duplicate member", TINY.replace("\"sense\": \"min\"", "\"sense\": \"min\", \"sense\": \"max\"")),
        ("an unknown member", TINY.replace("\"version\": 1", "\"version\": 1, \"scale\": \"2\"")),
        ("a trailing comma", TINY.replace("[[0, 0, \"1\"]]", "[[0, 0, \"1\"],]")),
        ("content after the document", format!("{TINY} ")),
        ("two documents", format!("{TINY}{TINY}")),
        ("a truncated document", TINY[..TINY.len() - 1].to_owned()),
        ("a unicode escape", TINY.replace("\"x\"", "\"\\u0078\"")),
        ("an accepted escape", TINY.replace("\"x\"", "\"a\\tb\"")),
        ("a stray escape", TINY.replace("\"x\"", "\"a\\qb\"")),
        ("a leading zero", TINY.replace("\"version\": 1", "\"version\": 01")),
        ("a float version", TINY.replace("\"version\": 1", "\"version\": 1.0")),
        ("a huge version", TINY.replace("\"version\": 1", "\"version\": 99999999999999999999")),
        ("a maximizing document", TINY.replace("\"min\"", "\"max\"")),
        ("a free row", TINY.replace("\"sense\": \"L\"", "\"sense\": \"N\"")),
        ("a ranged equality", TINY.replace("\"sense\": \"L\", \"rhs\": \"4\", \"range\": null", "\"sense\": \"E\", \"rhs\": \"4\", \"range\": \"-2\"")),
        ("crossed bounds", TINY.replace("\"lb\": \"0\", \"ub\": \"10\"", "\"lb\": \"9\", \"ub\": \"1\"")),
        ("an unbounded variable", TINY.replace("\"lb\": \"0\", \"ub\": \"10\"", "\"lb\": null, \"ub\": null")),
        ("an exact decimal", TINY.replace("\"obj\": \"-1\"", "\"obj\": \"0.1\"")),
        ("an exponent in a string", TINY.replace("\"obj\": \"-1\"", "\"obj\": \"1.5e2\"")),
        ("a malformed decimal", TINY.replace("\"obj\": \"-1\"", "\"obj\": \"1.2.3\"")),
        ("an infinite decimal", TINY.replace("\"obj\": \"-1\"", "\"obj\": \"inf\"")),
        ("a plus-signed decimal", TINY.replace("\"obj\": \"-1\"", "\"obj\": \"+1\"")),
        ("a wide decimal", TINY.replace("\"obj\": \"-1\"", "\"obj\": \"1.0e99999999\"")),
        ("an overflowing decimal", TINY.replace("\"obj\": \"-1\"", "\"obj\": \"1e400\"")),
        ("a long name", TINY.replace("\"name\": \"x\"", &format!("\"name\": \"{}\"", "x".repeat(300)))),
        ("a duplicate row name", TINY.replace("\"rows\": [{\"name\": \"c1\"", "\"rows\": [{\"name\": \"c1\", \"sense\": \"L\", \"rhs\": \"4\", \"range\": null}, {\"name\": \"c1\"")),
        ("nested too deep", format!("{}{}", "[".repeat(32), "]".repeat(32))),
    ] {
        add(name, text);
    }

    for (name, text) in &cases {
        agree(name, text.as_bytes());
    }
    assert!(cases.len() >= 100, "the sweep shrank: {}", cases.len());
}

#[test]
fn the_two_readers_agree_on_every_single_byte_truncation_of_a_vector() {
    // Truncation is what a partial write and a bad download look like, and it
    // reaches shapes no hand-written case does: a document cut inside a
    // string, inside a number, inside a member name.
    for (name, raw) in ACCEPTED {
        for cut in 0..raw.len() {
            agree(&format!("{name} truncated to {cut}"), &raw[..cut]);
        }
    }
}

#[test]
fn the_two_readers_agree_on_every_single_byte_edit_of_a_vector() {
    // One byte at a time, replaced by each of a handful of characters that
    // change the grammar rather than the arithmetic.
    const EDITS: &[u8] = b"{}[],:\"\\ 0.eE-tn";
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
