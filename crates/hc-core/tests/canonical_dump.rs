//! The canonical dump against every frozen vector, from models assembled by
//! hand.
//!
//! No parser exists yet, and that is the point of doing this now. If the first
//! parser were written first, the dump it happened to produce would become the
//! definition of the dump, and the frozen `.canon` files would be checking the
//! parser against itself. Hand-assembling each model from the rules — the
//! ranges table, the objective-constant negation, the duplicate-entry
//! cancellation, the bound types — and rendering it is the only way to find out
//! whether the renderer and the rules agree before a parser has an opinion.
//!
//! It is also a rehearsal. Every reading below is one a parser will have to
//! perform, and getting them wrong here is much cheaper than getting them wrong
//! inside a state machine.

use hc_core::{FormatTag, Model, ModelParts, Row, RowId, Sense, VarId, Variable, instance_digest};
use hc_exact::Rational;

fn number(value: i64) -> Rational {
    Rational::from_i64(value)
}

fn decimal(text: &str) -> Rational {
    Rational::parse_decimal(text).expect("a decimal token")
}

fn bounded(lower: Rational, upper: Option<Rational>, objective: Rational, name: &str) -> Variable {
    Variable {
        lower: Some(lower),
        upper,
        integral: false,
        objective,
        name: name.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// The smallest instance
// ---------------------------------------------------------------------------

const TINY_FILE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");
const TINY_DUMP: &str = include_str!("../../../spec/v1/vectors/mps/tiny.canon");

#[test]
fn the_smallest_instance_renders_its_frozen_dump() {
    let model = Model::new(ModelParts {
        format: FormatTag::MpsFixed,
        digest: instance_digest(FormatTag::MpsFixed.byte(), TINY_FILE),
        original_sense: Sense::Min,
        objective_constant: Rational::zero(),
        // `UI` sets an upper bound and marks the column integer; the lower
        // bound stays at the default zero.
        variables: vec![Variable {
            lower: Some(number(0)),
            upper: Some(number(10)),
            integral: true,
            objective: number(-1),
            name: "X".to_owned(),
        }],
        // An `L` row becomes an absent lower side and the right-hand side above.
        rows: vec![Row {
            lower: None,
            upper: Some(number(4)),
            name: "C1".to_owned(),
        }],
        entries: vec![(VarId(0), RowId(0), number(1))],
        zero_sums: Vec::new(),
        discarded: Vec::new(),
    });
    assert_eq!(model.canonical_dump(), TINY_DUMP);
}

// ---------------------------------------------------------------------------
// Every row sense crossed with every range sign
// ---------------------------------------------------------------------------

const RANGES_FILE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/ranges.mps");
const RANGES_DUMP: &str = include_str!("../../../spec/v1/vectors/mps/ranges.canon");

#[test]
fn the_ranges_table_renders_its_frozen_dump() {
    // Seven rows, one right-hand side of ten, one range of magnitude four, and
    // the sign of that range mattering for an equality row and not for an
    // inequality row. This is the single most consequential disagreement
    // between readers of the format, so the table is applied here by hand:
    //
    //   G with any sign  ->  (b, b + |R|)
    //   L with any sign  ->  (b - |R|, b)
    //   E with R above 0 ->  (b, b + R)
    //   E with R below 0 ->  (b + R, b)
    //   E with R of 0    ->  (b, b)
    let rows = vec![
        Row {
            lower: Some(number(10)),
            upper: Some(number(14)),
            name: "R1".to_owned(),
        },
        Row {
            lower: Some(number(10)),
            upper: Some(number(14)),
            name: "R2".to_owned(),
        },
        Row {
            lower: Some(number(6)),
            upper: Some(number(10)),
            name: "R3".to_owned(),
        },
        Row {
            lower: Some(number(6)),
            upper: Some(number(10)),
            name: "R4".to_owned(),
        },
        Row {
            lower: Some(number(10)),
            upper: Some(number(14)),
            name: "R5".to_owned(),
        },
        Row {
            lower: Some(number(6)),
            upper: Some(number(10)),
            name: "R6".to_owned(),
        },
        Row {
            lower: Some(number(10)),
            upper: Some(number(10)),
            name: "R7".to_owned(),
        },
    ];
    let entries = (0..7)
        .map(|row| (VarId(0), RowId(row), number(1)))
        .collect();

    let model = Model::new(ModelParts {
        format: FormatTag::MpsFixed,
        digest: instance_digest(FormatTag::MpsFixed.byte(), RANGES_FILE),
        original_sense: Sense::Min,
        objective_constant: Rational::zero(),
        // `FR` removes both bounds and ignores its value field.
        variables: vec![Variable {
            lower: None,
            upper: None,
            integral: false,
            objective: number(1),
            name: "X".to_owned(),
        }],
        rows,
        entries,
        zero_sums: Vec::new(),
        discarded: Vec::new(),
    });
    assert_eq!(model.canonical_dump(), RANGES_DUMP);
}

// ---------------------------------------------------------------------------
// Text a float round-trip would corrupt
// ---------------------------------------------------------------------------

const EXACT_FILE: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/exact.mps");
const EXACT_DUMP: &str = include_str!("../../../spec/v1/vectors/mps/exact.canon");

#[test]
fn the_exactness_instance_renders_its_frozen_dump() {
    // Four readings meet in this one file:
    //
    //   - a tenth is a tenth, not the nearest binary64 to it;
    //   - the two entries of the first column in the first row sum to zero and
    //     leave a note rather than nothing;
    //   - the two entries of the second column in the same row sum to two;
    //   - a right-hand side entry on the objective row supplies the *negation*
    //     of the objective constant, which is the convention most likely to
    //     shift every reported objective without anything looking wrong.
    let model = Model::new(ModelParts {
        format: FormatTag::MpsFixed,
        digest: instance_digest(FormatTag::MpsFixed.byte(), EXACT_FILE),
        original_sense: Sense::Min,
        objective_constant: number(-5),
        variables: vec![
            bounded(number(0), Some(decimal("2.5")), decimal("0.1"), "X"),
            // `BV` fixes the bounds at zero and one and marks the column
            // integer, whether or not it appeared between the markers.
            Variable {
                lower: Some(number(0)),
                upper: Some(number(1)),
                integral: true,
                objective: decimal("-2.5"),
                name: "Y".to_owned(),
            },
        ],
        rows: vec![
            Row {
                lower: Some(number(1)),
                upper: None,
                name: "R1".to_owned(),
            },
            Row {
                lower: Some(number(300)),
                upper: Some(number(300)),
                name: "R2".to_owned(),
            },
        ],
        entries: vec![
            (VarId(0), RowId(1), decimal("1.5e2")),
            (VarId(1), RowId(0), number(2)),
        ],
        zero_sums: vec![(VarId(0), RowId(0))],
        discarded: Vec::new(),
    });
    assert_eq!(model.canonical_dump(), EXACT_DUMP);
}

// ---------------------------------------------------------------------------
// Properties every dump has
// ---------------------------------------------------------------------------

#[test]
fn every_frozen_dump_is_ascii_and_line_feed_terminated() {
    for (name, dump) in [
        ("tiny", TINY_DUMP),
        ("ranges", RANGES_DUMP),
        ("exact", EXACT_DUMP),
    ] {
        assert!(dump.is_ascii(), "{name} is not ASCII");
        assert!(
            dump.ends_with("end\n"),
            "{name} does not end where it should"
        );
        assert!(!dump.contains('\r'), "{name} carries a carriage return");
        assert!(
            dump.starts_with("hc-canonical 1\n"),
            "{name} has no magic line"
        );
        for line in dump.lines() {
            assert_eq!(
                line.trim_end(),
                line,
                "{name} has a line with trailing whitespace"
            );
        }
    }
}

#[test]
fn each_frozen_dump_states_the_digest_of_its_own_file() {
    // The dump is not merely about a model; it is about a file. A dump whose
    // digest line does not match the bytes beside it would let a certificate
    // be bound to one file while the comparison was performed against another.
    for (file, dump) in [
        (TINY_FILE, TINY_DUMP),
        (RANGES_FILE, RANGES_DUMP),
        (EXACT_FILE, EXACT_DUMP),
    ] {
        let digest = hc_core::hex(&instance_digest(FormatTag::MpsFixed.byte(), file));
        let stated = dump
            .lines()
            .find_map(|line| line.strip_prefix("digest "))
            .expect("a digest line");
        assert_eq!(stated, digest);
    }
}
