//! Every accepted vector, written back out in all three formats and read again.
//!
//! The property is not that the file comes back byte for byte — it cannot,
//! since the writers have their own spelling and the digest is over the bytes.
//! The property is that the *model* comes back: the canonical dump below the
//! three lines that identify the file must be identical, coefficient for
//! coefficient, cancellation for cancellation, bound for bound.
//!
//! Each written file is read twice, once by each implementation. A writer that
//! produced something only its own reader accepted would be a writer whose
//! output nobody else could check, which for a file a proof is about is the
//! same as producing nothing.

use hc_core::Model;
use hc_io::write::{json as write_json, lp as write_lp, mps as write_mps};
use hc_io::{json, lp, mps};

const VECTORS: &[(&str, &[u8], Kind)] = &[
    (
        "tiny.mps",
        include_bytes!("../../../spec/v1/vectors/mps/tiny.mps"),
        Kind::Mps,
    ),
    (
        "ranges.mps",
        include_bytes!("../../../spec/v1/vectors/mps/ranges.mps"),
        Kind::Mps,
    ),
    (
        "exact.mps",
        include_bytes!("../../../spec/v1/vectors/mps/exact.mps"),
        Kind::Mps,
    ),
    (
        "tiny.lp",
        include_bytes!("../../../spec/v1/vectors/lp/tiny.lp"),
        Kind::Lp,
    ),
    (
        "ranges.lp",
        include_bytes!("../../../spec/v1/vectors/lp/ranges.lp"),
        Kind::Lp,
    ),
    (
        "exact.lp",
        include_bytes!("../../../spec/v1/vectors/lp/exact.lp"),
        Kind::Lp,
    ),
    (
        "tiny.json",
        include_bytes!("../../../spec/v1/vectors/json/tiny.json"),
        Kind::Json,
    ),
    (
        "ranges.json",
        include_bytes!("../../../spec/v1/vectors/json/ranges.json"),
        Kind::Json,
    ),
    (
        "exact.json",
        include_bytes!("../../../spec/v1/vectors/json/exact.json"),
        Kind::Json,
    ),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Mps,
    Lp,
    Json,
}

fn parse(raw: &[u8], kind: Kind) -> Model {
    match kind {
        Kind::Mps => mps::read(raw, mps::Form::Fixed).expect("a frozen vector"),
        Kind::Lp => lp::read(raw).expect("a frozen vector"),
        Kind::Json => json::read(raw).expect("a frozen vector"),
    }
}

/// Everything the canonical dump says about the model rather than the file.
fn model_of(dump: &str) -> String {
    dump.lines().skip(3).collect::<Vec<_>>().join("\n")
}

#[test]
fn every_vector_survives_a_round_trip_through_every_format() {
    for (name, raw, kind) in VECTORS {
        let original = parse(raw, *kind);
        let expected = model_of(&original.canonical_dump());

        let written = write_mps::write(&original).unwrap_or_else(|error| panic!("{name}: {error}"));
        let again = mps::read(written.as_bytes(), mps::Form::Free)
            .unwrap_or_else(|error| panic!("{name} as MPS: {error}\n{written}"));
        assert_eq!(
            model_of(&again.canonical_dump()),
            expected,
            "{name} through MPS\n{written}"
        );

        let written = write_lp::write(&original).unwrap_or_else(|error| panic!("{name}: {error}"));
        let again = lp::read(written.as_bytes())
            .unwrap_or_else(|error| panic!("{name} as LP: {error}\n{written}"));
        assert_eq!(
            model_of(&again.canonical_dump()),
            expected,
            "{name} through LP\n{written}"
        );

        let written =
            write_json::write(&original).unwrap_or_else(|error| panic!("{name}: {error}"));
        let again = json::read(written.as_bytes())
            .unwrap_or_else(|error| panic!("{name} as JSON: {error}\n{written}"));
        assert_eq!(
            model_of(&again.canonical_dump()),
            expected,
            "{name} through JSON\n{written}"
        );
    }
}

#[test]
fn the_independent_reader_accepts_everything_the_writers_produce() {
    // A writer checked only by its own reader has been checked against the
    // same misunderstanding twice.
    for (name, raw, kind) in VECTORS {
        let original = parse(raw, *kind);
        let expected = model_of(&original.canonical_dump());

        let written = write_mps::write(&original).expect("writable");
        let verifier = hc_verify::vparse::read(written.as_bytes(), hc_verify::vparse::Layout::Free)
            .unwrap_or_else(|error| panic!("{name} as MPS: {error}\n{written}"));
        assert_eq!(model_of(&verifier.dump()), expected, "{name} through MPS");

        let written = write_lp::write(&original).expect("writable");
        let verifier = hc_verify::vlp::read(written.as_bytes())
            .unwrap_or_else(|error| panic!("{name} as LP: {error}\n{written}"));
        assert_eq!(model_of(&verifier.dump()), expected, "{name} through LP");

        let written = write_json::write(&original).expect("writable");
        let verifier = hc_verify::vjson::read(written.as_bytes())
            .unwrap_or_else(|error| panic!("{name} as JSON: {error}\n{written}"));
        assert_eq!(model_of(&verifier.dump()), expected, "{name} through JSON");
    }
}

#[test]
fn a_round_trip_reaches_the_same_place_however_many_times_it_is_taken() {
    // The writers are idempotent on their own output: writing a model that
    // came from a written file produces the same file. A writer that was not
    // would be one whose spelling depended on where the model came from.
    for (name, raw, kind) in VECTORS {
        let original = parse(raw, *kind);
        let once = write_lp::write(&original).expect("writable");
        let reparsed = lp::read(once.as_bytes()).expect("its own output");
        let twice = write_lp::write(&reparsed).expect("writable");
        assert_eq!(once, twice, "{name} through LP twice");

        let once = write_json::write(&original).expect("writable");
        let reparsed = json::read(once.as_bytes()).expect("its own output");
        let twice = write_json::write(&reparsed).expect("writable");
        assert_eq!(once, twice, "{name} through JSON twice");

        let once = write_mps::write(&original).expect("writable");
        let reparsed = mps::read(once.as_bytes(), mps::Form::Free).expect("its own output");
        let twice = write_mps::write(&reparsed).expect("writable");
        assert_eq!(once, twice, "{name} through MPS twice");
    }
}

#[test]
fn a_writer_refuses_what_its_format_cannot_spell() {
    // The refusals are the honest half. A name with a space has no free-form
    // MPS spelling and no LP spelling at all; the JSON schema can carry it,
    // and does.
    let spaced = mps::read(
        b"ROWS\n N  obj\n G  r\nCOLUMNS\n    a b        obj       1\n    a b        r         1\nRHS\n    rhs       r         1\nENDATA\n",
        mps::Form::Fixed,
    )
    .expect("fixed form reads a name with a space");
    assert_eq!(spaced.variables()[0].name, "a b");
    assert!(write_mps::write(&spaced).is_err(), "free form splits on it");
    assert!(write_lp::write(&spaced).is_err(), "not an LP name");
    let written = write_json::write(&spaced).expect("the schema quotes it");
    let again = json::read(written.as_bytes()).expect("and reads it back");
    assert_eq!(again.variables()[0].name, "a b");

    // A reserved word is a name everywhere but here.
    let reserved = json::read(
        br#"{"version": 1, "sense": "min", "objective_constant": "0",
             "vars": [{"name": "free", "lb": "0", "ub": null, "integer": false, "obj": "1"}],
             "rows": [{"name": "r", "sense": "G", "rhs": "1", "range": null}],
             "nonzeros": [[0, 0, "1"]]}"#,
    )
    .expect("the schema has no reserved words");
    assert!(write_lp::write(&reserved).is_err());
    assert!(write_mps::write(&reserved).is_ok());
}
