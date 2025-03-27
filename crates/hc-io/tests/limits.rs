//! The parser's limits against the normative asset.
//!
//! The numbers live in the code so that a solver never needs its
//! specification present in order to know its own caps — a second input nobody
//! audited is a worse problem than a duplicated constant. This test is the
//! price of that choice, and it is a fair price: the two are held together in
//! both directions, and a change to either without the other fails the build.

use hc_core::SpecDocument;
use hc_io::mps::{Limits, Section};

const NUMERIC: &str = include_str!("../../../spec/v1/numeric.toml");
const MPS: &str = include_str!("../../../spec/v1/mps.toml");

#[test]
fn every_limit_matches_the_asset() {
    let document = SpecDocument::parse(NUMERIC).expect("inside the subset");
    let limits = Limits::SPEC;

    let parser = document.table("fence.parser").expect("the parser fence");
    assert_eq!(parser.integer("file_bytes"), Some(limits.file_bytes as i64));
    assert_eq!(parser.integer("nonzeros"), Some(limits.nonzeros as i64));
    assert_eq!(parser.integer("name_bytes"), Some(limits.name_bytes as i64));

    let token = document
        .table("fence.token_length")
        .expect("the token fence");
    assert_eq!(
        token.integer("limit_bytes"),
        Some(limits.token_bytes as i64)
    );

    let bits = document
        .table("fence.projected_bits")
        .expect("the width fence");
    assert_eq!(
        bits.integer("limit_bits"),
        Some(limits.projected_bits as i64)
    );
}

#[test]
fn the_asset_still_states_the_order_the_fence_applies_in() {
    // The order is the load-bearing half of the fence and it is prose in the
    // asset, so what can be checked is that the asset keeps saying it. A
    // silent deletion of this sentence would leave a caps table that reads
    // fine and a rule that no longer exists.
    let document = SpecDocument::parse(NUMERIC).expect("inside the subset");
    let fence = document.table("fence").expect("the fence table");
    let order = fence.text("order").expect("the order is stated");
    let token_at = order.find("Token length").expect("token length is named");
    let bits_at = order
        .find("projected bit length")
        .expect("the projection is named");
    let build_at = order.find("construction").expect("construction is named");
    let round_at = order.find("round-trip").expect("the round trip is named");
    assert!(
        token_at < bits_at,
        "the asset no longer puts token length first"
    );
    assert!(
        bits_at < build_at,
        "the asset no longer projects before constructing"
    );
    assert!(
        build_at < round_at,
        "the asset no longer constructs before the round trip"
    );
}

#[test]
fn the_accepted_sections_match_the_asset() {
    let document = SpecDocument::parse(MPS).expect("inside the subset");
    let stated = document
        .table("sections")
        .and_then(|table| table.get("accepted"))
        .and_then(hc_core::SpecValue::as_list)
        .expect("the accepted list");
    let ours: Vec<String> = Section::all()
        .iter()
        .map(|section| section.as_str().to_owned())
        .collect();
    assert_eq!(
        stated, ours,
        "the section vocabulary and the asset disagree, in this order"
    );

    let required = document
        .table("sections")
        .and_then(|table| table.get("required"))
        .and_then(hc_core::SpecValue::as_list)
        .expect("the required list");
    for name in required {
        assert!(
            Section::from_header(name).is_some(),
            "the asset requires a section the reader does not know: {name}"
        );
    }
}

#[test]
fn the_objective_sense_spellings_match_the_asset() {
    let document = SpecDocument::parse(MPS).expect("inside the subset");
    let stated = document
        .table("section.objsense")
        .and_then(|table| table.get("values"))
        .and_then(hc_core::SpecValue::as_list)
        .expect("the accepted spellings");
    for spelling in stated {
        assert!(
            hc_io::mps::section::read_sense(spelling, 1).is_ok(),
            "the asset accepts a spelling the reader refuses: {spelling}"
        );
    }
    assert_eq!(stated.len(), 4);
}
