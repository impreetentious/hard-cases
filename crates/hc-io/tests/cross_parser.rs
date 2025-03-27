//! The two parsers against each other.
//!
//! This is the differential the whole arrangement exists for. Two readers,
//! written separately, sharing no code and no data structure, are given the
//! same bytes; their canonical dumps are compared byte for byte, and their
//! refusals are compared identifier for identifier.
//!
//! The comparison is on the dumps rather than on the models, deliberately. A
//! comparison performed through either implementation's model type would be
//! comparing one of them against itself, and would pass for exactly the
//! disagreements it was built to find.
//!
//! This test lives on the solver's side of the boundary because the direction
//! of the dependency matters: a caller may use the verifier's public interface,
//! and the verifier may use nothing. A harness inside the verifier that reached
//! for the solver's parser would dissolve the property it is measuring.

use hc_io::mps::{Form, read as read_solver};
use hc_verify::vparse::{Layout, read as read_verifier};

const ACCEPTED: &[(&str, &[u8])] = &[
    (
        "tiny.mps",
        include_bytes!("../../../spec/v1/vectors/mps/tiny.mps"),
    ),
    (
        "ranges.mps",
        include_bytes!("../../../spec/v1/vectors/mps/ranges.mps"),
    ),
    (
        "exact.mps",
        include_bytes!("../../../spec/v1/vectors/mps/exact.mps"),
    ),
];

const REFUSED: &[(&str, &[u8])] = &[
    (
        "unknown-bound-type",
        include_bytes!("../../../spec/v1/vectors/mps/reject/unknown-bound-type.mps"),
    ),
    (
        "multiple-rhs-vectors",
        include_bytes!("../../../spec/v1/vectors/mps/reject/multiple-rhs-vectors.mps"),
    ),
    (
        "no-objective-row",
        include_bytes!("../../../spec/v1/vectors/mps/reject/no-objective-row.mps"),
    ),
    (
        "duplicate-row-name",
        include_bytes!("../../../spec/v1/vectors/mps/reject/duplicate-row-name.mps"),
    ),
    (
        "negative-up-default-lower",
        include_bytes!("../../../spec/v1/vectors/mps/reject/negative-up-default-lower.mps"),
    ),
    (
        "noninteger-ui",
        include_bytes!("../../../spec/v1/vectors/mps/reject/noninteger-ui.mps"),
    ),
    (
        "ranges-on-objective",
        include_bytes!("../../../spec/v1/vectors/mps/reject/ranges-on-objective.mps"),
    ),
    (
        "noncontiguous-column",
        include_bytes!("../../../spec/v1/vectors/mps/reject/noncontiguous-column.mps"),
    ),
    (
        "unbalanced-marker",
        include_bytes!("../../../spec/v1/vectors/mps/reject/unbalanced-marker.mps"),
    ),
    (
        "infinity-token",
        include_bytes!("../../../spec/v1/vectors/mps/reject/infinity-token.mps"),
    ),
    (
        "underflow-value",
        include_bytes!("../../../spec/v1/vectors/mps/reject/underflow-value.mps"),
    ),
    (
        "exponent-cap",
        include_bytes!("../../../spec/v1/vectors/mps/reject/exponent-cap.mps"),
    ),
    (
        "unknown-section",
        include_bytes!("../../../spec/v1/vectors/mps/reject/unknown-section.mps"),
    ),
    (
        "missing-endata",
        include_bytes!("../../../spec/v1/vectors/mps/reject/missing-endata.mps"),
    ),
];

/// Rewrites fixed-form text as free-form text naming the same model.
fn to_free_form(raw: &[u8]) -> Vec<u8> {
    const WINDOWS: [(usize, usize); 6] = [(2, 3), (5, 12), (15, 22), (25, 36), (40, 47), (50, 61)];
    let text = core::str::from_utf8(raw).expect("the vectors are ASCII");
    let mut out = String::new();
    for line in text.split('\n') {
        if line.is_empty() {
            out.push('\n');
            continue;
        }
        let first = line.as_bytes()[0];
        if first != b' ' && first != b'\t' {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let mut fields = Vec::new();
        for &(from, to) in &WINDOWS {
            let bytes = line.as_bytes();
            let start = (from - 1).min(bytes.len());
            let end = to.min(bytes.len());
            if start < end {
                let field = line[start..end].trim();
                if !field.is_empty() {
                    fields.push(field);
                }
            }
        }
        out.push(' ');
        out.push_str(&fields.join(" "));
        out.push('\n');
    }
    if out.ends_with("\n\n") {
        out.pop();
    }
    out.into_bytes()
}

#[test]
fn the_two_parsers_agree_on_every_accepted_vector() {
    for (name, raw) in ACCEPTED {
        let solver =
            read_solver(raw, Form::Fixed).unwrap_or_else(|error| panic!("{name}: {error}"));
        let verifier =
            read_verifier(raw, Layout::Fixed).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            solver.canonical_dump(),
            verifier.dump(),
            "{name}: the two parsers produced different canonical dumps"
        );
    }
}

#[test]
fn the_two_parsers_agree_under_the_other_variant_too() {
    for (name, raw) in ACCEPTED {
        let free = to_free_form(raw);
        let solver =
            read_solver(&free, Form::Free).unwrap_or_else(|error| panic!("{name} free: {error}"));
        let verifier = read_verifier(&free, Layout::Free)
            .unwrap_or_else(|error| panic!("{name} free: {error}"));
        assert_eq!(
            solver.canonical_dump(),
            verifier.dump(),
            "{name} in free form"
        );
    }
}

#[test]
fn the_two_parsers_refuse_the_same_files_for_the_same_reason() {
    for (name, raw) in REFUSED {
        let solver = read_solver(raw, Form::Fixed).err();
        let verifier = read_verifier(raw, Layout::Fixed).err();
        match (solver, verifier) {
            (Some(left), Some(right)) => assert_eq!(
                left.id.as_str(),
                right.code.text(),
                "{name}: the two parsers refused for different reasons ({left} against {right})"
            ),
            (left, right) => panic!(
                "{name}: only one parser refused it ({:?} against {:?})",
                left.map(|refusal| refusal.to_string()),
                right.map(|refusal| refusal.to_string())
            ),
        }
    }
}

#[test]
fn the_two_parsers_agree_on_files_neither_vector_set_covers() {
    // A handful of shapes the frozen corpus does not reach, checked for
    // agreement rather than for any particular answer. Where both accept, the
    // dumps must match; where both refuse, the identifiers must match; and one
    // accepting what the other refuses is the finding.
    let cases: &[(&str, &[u8])] = &[
        ("bare minimum", b"ROWS\n N obj\nCOLUMNS\nENDATA\n"),
        ("empty row kept", b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1\nRHS\n rhs r 1\nENDATA\n"),
        ("crossed bounds", b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nBOUNDS\n LO b x 5\n UP b x 2\nENDATA\n"),
        ("maximize", b"OBJSENSE MAX\nROWS\n N obj\n G r\nCOLUMNS\n x obj 3 r 1\nRHS\n rhs r 1\nENDATA\n"),
        ("objective constant", b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nRHS\n rhs obj 5 r 1\nENDATA\n"),
        ("cancelling pair", b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 3\n x r -3\nRHS\n rhs r 1\nENDATA\n"),
        ("explicit zero", b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 0\nRHS\n rhs r 1\nENDATA\n"),
        ("markers", b"ROWS\n N obj\n G r\nCOLUMNS\n m MARKER INTORG\n x obj 1 r 1\n m MARKER INTEND\nRHS\n rhs r 1\nENDATA\n"),
        ("free bound", b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nBOUNDS\n FR b x\nENDATA\n"),
        ("minus infinity bound", b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nBOUNDS\n MI b x\nENDATA\n"),
        ("binary bound", b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nBOUNDS\n BV b x\nENDATA\n"),
        ("fixed bound", b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nBOUNDS\n FX b x 2\nENDATA\n"),
        ("range on equality", b"ROWS\n N obj\n E r\nCOLUMNS\n x obj 1 r 1\nRHS\n rhs r 10\nRANGES\n rng r -4\nENDATA\n"),
        ("no terminator", b"ROWS\n N obj\nCOLUMNS\n"),
        ("record before section", b" x obj 1\nROWS\n N obj\nCOLUMNS\nENDATA\n"),
        ("unknown section", b"ROWS\n N obj\nCOLUMNS\nQUADOBJ\nENDATA\n"),
        ("duplicate column name", b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1\n y obj 1\n x r 1\nENDATA\n"),
        ("too many free fields", b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1 extra\nENDATA\n"),
        ("empty file", b""),
        ("only a comment", b"* nothing here\n"),
    ];

    for (name, raw) in cases {
        let solver = read_solver(raw, Form::Free);
        let verifier = read_verifier(raw, Layout::Free);
        match (solver, verifier) {
            (Ok(left), Ok(right)) => {
                assert_eq!(left.canonical_dump(), right.dump(), "{name}: dumps differ");
            }
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
}
