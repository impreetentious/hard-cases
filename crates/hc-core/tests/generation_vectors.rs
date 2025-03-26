//! The deterministic generator against its frozen vectors.
//!
//! Each vector gives a seed word, the four state words the seeding rule
//! produces from it, and the first outputs of the generator initialized with
//! that state. A generator that is right about its outputs and wrong about its
//! state will diverge from the other implementation the first time a state is
//! carried across a boundary, so both halves are checked rather than just the
//! visible one.
//!
//! One vector begins a step earlier, at an instance file on disk, and derives
//! the seed word from the instance digest. Its digest half needs a hash that
//! does not exist yet; the half that starts from the stated seed word is
//! checked here, and the harness fails if that vector ever stops naming the
//! seed word explicitly, so the deferral cannot quietly become a hole.

use hc_core::{SpecDocument, SpecTable, Xoshiro256PlusPlus};

const GENERATION: &str = include_str!("../../../spec/v1/rng.toml");

/// Reads a `0x`-prefixed sixty-four bit word from a vector.
fn word(table: &SpecTable, key: &str) -> u64 {
    let text = table
        .text(key)
        .unwrap_or_else(|| panic!("[{}] has no {key}", table.name()));
    parse_word(table.name(), key, text)
}

fn parse_word(table: &str, key: &str, text: &str) -> u64 {
    let digits = text
        .strip_prefix("0x")
        .unwrap_or_else(|| panic!("[{table}] {key} is not a hexadecimal word: {text}"));
    u64::from_str_radix(digits, 16).unwrap_or_else(|error| panic!("[{table}] {key}: {error}"))
}

fn words(table: &SpecTable, key: &str) -> Vec<u64> {
    let list = table
        .get(key)
        .and_then(hc_core::SpecValue::as_list)
        .unwrap_or_else(|| panic!("[{}] has no {key} list", table.name()));
    list.iter()
        .map(|text| parse_word(table.name(), key, text))
        .collect()
}

fn document() -> SpecDocument {
    SpecDocument::parse(GENERATION).expect("the generation asset is inside the subset")
}

#[test]
fn every_vector_reproduces_its_state_and_its_outputs() {
    let document = document();
    let mut seen = 0usize;
    for table in document.tables_under("vector") {
        let name = table.name();
        let seed = word(table, "seed64");
        let expected_state = words(table, "state");
        assert_eq!(expected_state.len(), 4, "[{name}] state is not four words");

        let seeded = Xoshiro256PlusPlus::from_seed(seed);
        assert_eq!(seeded.state().to_vec(), expected_state, "[{name}] state");

        let mut generator = seeded;
        let expected_outputs = words(table, "outputs");
        assert!(!expected_outputs.is_empty(), "[{name}] states no outputs");
        for (index, expected) in expected_outputs.iter().enumerate() {
            assert_eq!(generator.next_u64(), *expected, "[{name}] output {index}");
        }
        seen += 1;
    }
    assert!(seen >= 4, "only {seen} generation vectors were read");
}

#[test]
fn the_instance_derived_vector_still_states_its_seed_word() {
    // The step from instance bytes to a seed word runs through a hash this
    // side does not yet implement. Everything after that step is checked by
    // the test above, and this one fails if the vector ever stops carrying the
    // intermediate value, which is what makes the deferral visible rather than
    // silent.
    let document = document();
    let table = document
        .table("vector.from_instance")
        .expect("the end-to-end vector is present");
    assert!(
        table.text("seed64").is_some(),
        "the seed word is no longer stated"
    );
    assert!(
        table.text("instance_digest").is_some(),
        "the digest is no longer stated"
    );
    assert_eq!(table.integer("user_seed"), Some(0));
    assert_eq!(table.integer("format_tag"), Some(1));
}

#[test]
fn the_seeding_rule_is_the_one_the_asset_states() {
    let document = document();
    let seeding = document
        .table("seeding")
        .expect("the seeding rule is present");
    let rule = seeding.text("rule").expect("the rule is stated");
    // The rule is prose, but the exclusive-or of the instance-derived word
    // with a user seed is the part an implementation can get backwards, so the
    // asset is required to keep naming it.
    assert!(
        rule.contains("XOR"),
        "the seeding rule no longer names its combination"
    );
    let initialization = seeding
        .text("initialization")
        .expect("initialization is stated");
    assert!(
        initialization.contains("SplitMix64"),
        "the expander is no longer named"
    );
}
