//! The two variants against each other, and the readings only one of them can
//! reach.
//!
//! The manifest deliberately does not check in free-form copies of the frozen
//! instances. Two spellings of one model must produce *different* digests and
//! *identical* dumps below the digest line, and a pair of checked-in files
//! would let those two facts drift apart without anything noticing. So the
//! free-form copy is generated here, from the fixed-form bytes, and both are
//! read.
//!
//! The conversion is mechanical: a section header passes through, and a record
//! becomes its non-empty column windows separated by single spaces. That works
//! because a fixed record's windows are already in field order, and because
//! none of these instances carries a name with a space in it — which the fixed
//! form permits and the free form does not, and which is exactly why the two
//! variants get different digests.

use hc_core::{Model, SpecDocument};
use hc_io::mps::{Form, Options, read, read_with};

const MANIFEST: &str = include_str!("../../../spec/v1/vectors/mps/manifest.toml");

const INSTANCES: &[(&str, &[u8])] = &[
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

/// The fixed-form column windows, one-based and inclusive.
const WINDOWS: [(usize, usize); 6] = [(2, 3), (5, 12), (15, 22), (25, 36), (40, 47), (50, 61)];

/// Rewrites fixed-form text as free-form text naming the same model.
fn to_free_form(raw: &[u8]) -> Vec<u8> {
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
            if start >= end {
                continue;
            }
            let field = line[start..end].trim();
            if !field.is_empty() {
                fields.push(field);
            }
        }
        out.push(' ');
        out.push_str(&fields.join(" "));
        out.push('\n');
    }
    // The trailing empty split produced one line feed too many.
    if out.ends_with("\n\n") {
        out.pop();
    }
    out.into_bytes()
}

/// A dump with its digest line removed, which is the part two spellings share.
fn dump_below_digest(model: &Model) -> String {
    let mut dump = String::new();
    for line in model
        .canonical_dump()
        .lines()
        .filter(|line| !line.starts_with("digest ") && !line.starts_with("format "))
    {
        dump.push_str(line);
        dump.push('\n');
    }
    dump
}

#[test]
fn each_instance_reads_the_same_under_both_variants() {
    for (name, raw) in INSTANCES {
        let fixed = read(raw, Form::Fixed).unwrap_or_else(|error| panic!("{name}: {error}"));
        let free_text = to_free_form(raw);
        let free = read(&free_text, Form::Free)
            .unwrap_or_else(|error| panic!("{name} in free form: {error}"));
        assert_eq!(
            dump_below_digest(&fixed),
            dump_below_digest(&free),
            "{name}: the two variants read different models"
        );
    }
}

#[test]
fn the_two_variants_never_share_a_digest() {
    // The tag is prepended before hashing precisely so that a file legal under
    // both sets of rules cannot claim to be one instance.
    for (name, raw) in INSTANCES {
        let fixed = read(raw, Form::Fixed).expect("accepted");
        let free = read(&to_free_form(raw), Form::Free).expect("accepted");
        assert_ne!(fixed.digest(), free.digest(), "{name}");
        // Even the same bytes under the two tags differ, before any rewriting.
        let same_bytes = read(raw, Form::Free);
        if let Ok(model) = same_bytes {
            assert_ne!(fixed.digest(), model.digest(), "{name} under two tags");
        }
    }
}

#[test]
fn the_manifest_still_says_why_the_free_copies_are_not_checked_in() {
    let manifest = SpecDocument::parse(MANIFEST).expect("inside the subset");
    let note = manifest
        .table("meta")
        .and_then(|table| table.text("note"))
        .expect("a note");
    assert!(
        note.to_lowercase().contains("free-form"),
        "the manifest no longer explains the generated variants"
    );
}

#[test]
fn the_objective_sense_can_be_written_two_ways_in_free_form() {
    // On its own record, and on the header line. Both are accepted in the free
    // form; the header-line spelling is not accepted in the fixed form,
    // because a tail there would be content outside every window.
    let on_record =
        b"OBJSENSE\n MAX\nROWS\n N obj\n G r\nCOLUMNS\n x obj 2 r 1\nRHS\n rhs r 1\nENDATA\n";
    let on_header =
        b"OBJSENSE MAX\nROWS\n N obj\n G r\nCOLUMNS\n x obj 2 r 1\nRHS\n rhs r 1\nENDATA\n";
    let a = read(on_record, Form::Free).expect("accepted");
    let b = read(on_header, Form::Free).expect("accepted");
    assert_eq!(dump_below_digest(&a), dump_below_digest(&b));

    // Maximization is negated at the parse boundary, and the original sense is
    // recorded so the negation is visible.
    assert_eq!(a.variables()[0].objective.to_canonical_string(), "-2");
    assert!(a.canonical_dump().contains("sense min original=max\n"));

    // The same file without the sense keeps the coefficient as written.
    let plain = read(
        b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 2 r 1\nRHS\n rhs r 1\nENDATA\n",
        Form::Free,
    )
    .expect("accepted");
    assert_eq!(plain.variables()[0].objective.to_canonical_string(), "2");
    assert!(plain.canonical_dump().contains("sense min original=min\n"));
}

#[test]
fn the_objective_constant_is_the_negation_of_its_right_hand_side_entry() {
    let model = read(
        b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nRHS\n rhs r 1 obj 5\nENDATA\n",
        Form::Free,
    )
    .expect("accepted");
    assert_eq!(model.objective_constant().to_canonical_string(), "-5");

    // A second entry on the objective row replaces the first, matching the
    // general last-record-wins rule for repeated entries.
    let replaced = read(
        b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nRHS\n rhs obj 5\n rhs obj 7\nENDATA\n",
        Form::Free,
    )
    .expect("accepted");
    assert_eq!(replaced.objective_constant().to_canonical_string(), "-7");

    // Under maximization the constant is negated with the rest of the
    // objective, so the two negations compose rather than cancel by accident.
    let maximized = read(
        b"OBJSENSE MAX\nROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nRHS\n rhs obj 5\nENDATA\n",
        Form::Free,
    )
    .expect("accepted");
    assert_eq!(maximized.objective_constant().to_canonical_string(), "5");
}

#[test]
fn a_lenient_parse_forgives_the_ambiguous_bound_and_says_what_it_forgave() {
    let source = b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nBOUNDS\n UP bnd x -3\nENDATA\n";
    let strict = read(source, Form::Free).expect_err("strict refuses the ambiguity");
    assert_eq!(strict.id.as_str(), "HC-INSTANCE-19");

    let lenient = read_with(source, Options::lenient(Form::Free)).expect("lenient accepts it");
    assert_eq!(lenient.forgiven.len(), 1);
    assert!(lenient.forgiven[0].contains("negative upper bound"));
    // The forgiven reading is the one the rules name: the lower bound is
    // removed rather than left at zero.
    assert!(lenient.model.variables()[0].lower.is_none());
    assert_eq!(
        lenient.model.variables()[0]
            .upper
            .as_ref()
            .map(hc_exact::Rational::to_canonical_string),
        Some("-3".to_owned())
    );

    // A strict parse never forgives anything, and says so by carrying nothing.
    let clean = read_with(
        b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1 r 1\nENDATA\n",
        Options::strict(Form::Free),
    )
    .expect("accepted");
    assert!(clean.forgiven.is_empty());
}

#[test]
fn a_free_row_after_the_first_is_discarded_and_recorded_as_discarded() {
    let model = read(
        b"ROWS\n N obj\n N spare\n G r\nCOLUMNS\n x obj 1 spare 9\n x r 1\nRHS\n rhs r 1\nENDATA\n",
        Form::Free,
    )
    .expect("accepted");
    assert_eq!(model.discarded(), &["spare".to_owned()]);
    assert_eq!(model.rows().len(), 1);
    // The coefficient on the discarded row goes nowhere, exactly as the row
    // does, and does not become a stored nonzero.
    assert_eq!(model.matrix().nonzeros(), 1);
    assert!(model.canonical_dump().contains("discarded spare\n"));
}

#[test]
fn integrality_markers_switch_only_the_columns_between_them() {
    let model = read(
        b"ROWS\n N obj\n G r\nCOLUMNS\n a obj 1 r 1\n m MARKER INTORG\n b obj 1 r 1\n m MARKER INTEND\n c obj 1 r 1\nRHS\n rhs r 1\nENDATA\n",
        Form::Free,
    )
    .expect("accepted");
    let integral: Vec<bool> = model
        .variables()
        .iter()
        .map(|variable| variable.integral)
        .collect();
    assert_eq!(integral, vec![false, true, false]);

    // Apostrophes around the two literals are stripped.
    let quoted = read(
        b"ROWS\n N obj\n G r\nCOLUMNS\n m MARKER 'MARKER' \n a obj 1 r 1\nRHS\n rhs r 1\nENDATA\n",
        Form::Free,
    );
    // That record is not a marker: the code position holds nothing, so it is
    // an ordinary entry, and it names a row that does not exist.
    assert_eq!(
        quoted.expect_err("not a marker").id.as_str(),
        "HC-INSTANCE-12"
    );
}

#[test]
fn the_byte_level_shape_of_the_input_is_checked_before_anything_else() {
    // A carriage return makes this a different byte sequence and therefore a
    // different instance; accepting it and normalizing would let two digests
    // name one model.
    let carriage = b"ROWS\r\n N obj\r\nCOLUMNS\r\nENDATA\r\n";
    assert_eq!(
        read(carriage, Form::Free)
            .expect_err("not text")
            .id
            .as_str(),
        "HC-INSTANCE-29"
    );

    let high_byte = b"ROWS\n N ob\xc3\xa9\nCOLUMNS\nENDATA\n";
    assert_eq!(
        read(high_byte, Form::Free)
            .expect_err("not text")
            .id
            .as_str(),
        "HC-INSTANCE-29"
    );

    let null = b"ROWS\n N obj\0\nCOLUMNS\nENDATA\n";
    assert_eq!(
        read(null, Form::Free).expect_err("not text").id.as_str(),
        "HC-INSTANCE-29"
    );
}

#[test]
fn records_and_references_that_have_nowhere_to_go_are_refused() {
    let cases: &[(&[u8], &str)] = &[
        (
            b" x obj 1\nROWS\n N obj\nCOLUMNS\nENDATA\n",
            "HC-INSTANCE-10",
        ),
        (
            b"ROWS\n N obj\n G r\nCOLUMNS\n x nope 1\nENDATA\n",
            "HC-INSTANCE-12",
        ),
        (
            b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1\nBOUNDS\n UP bnd nope 1\nENDATA\n",
            "HC-INSTANCE-13",
        ),
        (
            b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1\nBOUNDS\n UP bnd x 1\n UP bnd x 2\nENDATA\n",
            "HC-INSTANCE-24",
        ),
        (
            b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1\nRHS\n one r 1\n two r 2\nENDATA\n",
            "HC-INSTANCE-16",
        ),
        (
            b"ROWS\n N obj\n G r\nCOLUMNS\n x obj 1\nRANGES\n rng r 1\nRHS\n rhs r 1\nENDATA\n",
            "HC-INSTANCE-09",
        ),
    ];
    for (source, expected) in cases {
        let refusal = read(source, Form::Free).expect_err("refused");
        assert_eq!(
            refusal.id.as_str(),
            *expected,
            "for {}",
            String::from_utf8_lossy(source)
        );
    }
}
