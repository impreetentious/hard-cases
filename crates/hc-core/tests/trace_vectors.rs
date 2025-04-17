//! The trace encodings against their frozen vector.
//!
//! One trace, hand-encoded in binary and rendered in text, neither produced by
//! this code. The binary form has to read back as the events the annotation
//! describes, rewrite itself byte for byte, and render exactly the text beside
//! it. A change to any of the three is a change to the contract.

use hc_core::trace::{Event, Trace};
use hc_core::{SpecDocument, SpecValue};

const VECTOR: &str = include_str!("../../../spec/v1/vectors/trace/tiny-run.hex");
const TEXT: &str = include_str!("../../../spec/v1/vectors/trace/tiny-run.json");
const MANIFEST: &str = include_str!("../../../spec/v1/vectors/trace/manifest.toml");
const SCHEMA: &str = include_str!("../../../spec/v1/trace.toml");

/// Strips the annotation and returns the bytes the vector is.
fn bytes() -> Vec<u8> {
    let mut out = Vec::new();
    for line in VECTOR.lines() {
        let body = line.split('#').next().unwrap_or_default();
        for pair in body.split_whitespace() {
            out.push(u8::from_str_radix(pair, 16).expect("a hexadecimal byte pair"));
        }
    }
    out
}

/// The trace the annotation describes, written out here by hand.
fn expected() -> Trace {
    Trace {
        instance: [
            0xb4, 0xe7, 0x9a, 0xf6, 0x38, 0x0a, 0x28, 0xd6, 0x4f, 0x56, 0x64, 0x98, 0x0b, 0xb3,
            0xd2, 0x52, 0xb7, 0xd4, 0x5c, 0xe5, 0x27, 0xdd, 0xc7, 0xee, 0xea, 0xb2, 0xe0, 0xdb,
            0x16, 0x0d, 0xce, 0x8d,
        ],
        events: vec![
            Event::RootLp {
                iterations: 1,
                objective: -4.0,
            },
            Event::NodeOpen {
                node: 0,
                parent: 0,
                depth: 0,
            },
            Event::NodeSolved {
                node: 0,
                iterations: 1,
                objective: -4.0,
            },
            Event::NodeIntegral {
                node: 0,
                objective: "-4".to_owned(),
            },
            Event::Incumbent {
                objective: "-4".to_owned(),
                source: 4,
            },
            Event::CertifyStart,
            Event::CertifyComplete {
                lower: "-4".to_owned(),
                upper: "-4".to_owned(),
                gap: "0".to_owned(),
            },
            Event::ProofAssembled {
                bytes: 160,
                checksum: 3_838_920_469,
            },
            Event::VerifyResult {
                accepted: true,
                rejection: None,
                work: 473,
            },
            Event::Terminal { status: 0 },
        ],
    }
}

#[test]
fn the_frozen_bytes_read_back_as_the_run_they_annotate() {
    let raw = bytes();
    let read = Trace::decode(&raw).expect("the vector is a trace");
    assert_eq!(read, expected());
}

#[test]
fn the_encoder_reproduces_the_frozen_bytes() {
    let raw = bytes();
    assert_eq!(
        expected().encode().expect("the trace is writable"),
        raw,
        "the encoder and the frozen vector disagree"
    );
    // And the round trip in the other direction, which is what a determinism
    // gate compares between two runs of one solve.
    let read = Trace::decode(&raw).expect("a trace");
    assert_eq!(read.encode().expect("writable"), raw);
}

#[test]
fn the_textual_form_is_a_rendering_of_the_same_events() {
    let read = Trace::decode(&bytes()).expect("a trace");
    assert_eq!(read.render().expect("renderable"), TEXT);
}

#[test]
fn the_trace_is_bound_to_the_instance_it_is_about() {
    // The digest in the header is the one the instance vector beside it is
    // pinned at, and the same one the certificate of that run carries. A trace
    // that could not be tied to a problem could be drawn beside any of them.
    let read = Trace::decode(&bytes()).expect("a trace");
    assert_eq!(
        hc_core::hex(&read.instance),
        "b4e79af6380a28d64f5664980bb3d252b7d45ce527ddc7eeeab2e0db160dce8d"
    );
    assert_eq!(
        hc_core::hex(&hc_core::instance_digest(
            hc_core::FormatTag::MpsFixed.byte(),
            include_bytes!("../../../spec/v1/vectors/mps/tiny.mps")
        )),
        hc_core::hex(&read.instance),
        "the trace names an instance other than the one it says it ran"
    );
    assert!(TEXT.contains("\"instance\": \"b4e79af6"));
}

#[test]
fn a_float_is_a_number_and_an_exact_value_is_a_string() {
    // The distinction the schema is built around, checked on the rendering
    // rather than asserted about it. A consumer cannot render one as the other
    // by accident.
    assert!(
        TEXT.contains("\"objective\": -4.0"),
        "the search objective is not written as a number"
    );
    assert!(
        TEXT.contains("\"objective\": \"-4\""),
        "the exact objective is not written as a string"
    );
    assert!(
        TEXT.contains("\"gap\": \"0\""),
        "an exact gap is not written as a string"
    );
    assert!(!TEXT.contains("\"gap\": 0"));
}

#[test]
fn the_manifest_and_the_vector_agree_about_what_it_is() {
    let manifest = SpecDocument::parse(MANIFEST).expect("inside the subset");
    let vector = manifest
        .table("vector.tiny_run")
        .expect("the manifest names its vector");
    assert_eq!(
        vector.integer("bytes"),
        Some(bytes().len() as i64),
        "the manifest and the vector disagree about its length"
    );
    assert_eq!(
        vector.integer("events"),
        Some(expected().events.len() as i64),
        "the manifest and the vector disagree about its event count"
    );
}

#[test]
fn every_event_the_schema_declares_has_a_code_here_and_the_words_match() {
    // The asset is the specification and this is the implementation; a kind
    // declared there with no code here is a kind nobody can write.
    let schema = SpecDocument::parse(SCHEMA).expect("inside the subset");
    let mut declared = 0usize;
    for table in schema.tables_under("event") {
        let code = table.integer("code").expect("every event states its code");
        declared += 1;
        assert!(
            (0..=18).contains(&code) || code == 255,
            "the schema declares code {code}, which this module has no case for"
        );
    }
    assert_eq!(declared, 20, "the schema no longer declares twenty records");

    for (table, list) in [
        ("words.stage", hc_core::trace::STAGES.as_slice()),
        ("words.family", hc_core::trace::FAMILIES.as_slice()),
        ("words.source", hc_core::trace::SOURCES.as_slice()),
        ("words.blocked", hc_core::trace::BLOCKED.as_slice()),
        ("words.status", hc_core::trace::STATUSES.as_slice()),
    ] {
        let values = schema
            .table(table)
            .and_then(|found| found.get("values"))
            .and_then(SpecValue::as_list)
            .unwrap_or_else(|| panic!("[{table}] has no values"));
        assert_eq!(values, list, "[{table}] and the module disagree");
    }
}
