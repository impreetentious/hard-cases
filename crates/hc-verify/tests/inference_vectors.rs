//! The hand-worked inference vectors, driven through the rules.
//!
//! `spec/v1/vectors/proof/inferences.toml` states, for each derivation, the
//! multipliers, the verdict, and — where the verdict is a refusal — the exact
//! rejection identifier the earliest check must produce. Every derivation in
//! it was done with a pencil, which is the point: a vector produced by running
//! this code would agree with this code forever.
//!
//! The harness reads the asset with the verifier's own reader rather than with
//! a dependency, because the crate that must be able to read its own
//! specification is this one.

use hc_verify::vaxiom::Axioms;
use hc_verify::vmodel::{Cell, Column, Constraint, Direction, Instance, Tag, whole};
use hc_verify::vrat::Frac;
use hc_verify::vrules::{bound_is, contradiction, dual_bound, everything};
use hc_verify::vspec::Asset;
use hc_verify::vwork::Work;

const VECTORS: &str = include_str!("../../../spec/v1/vectors/proof/inferences.toml");

/// The model the dual vectors are written against.
fn model_a() -> Instance {
    Instance {
        tag: Tag::MpsFixed,
        digest: [0; 32],
        written: Direction::Min,
        constant: Frac::zero(),
        columns: vec![Column {
            low: Some(whole(0)),
            high: Some(whole(10)),
            whole: true,
            cost: whole(-1),
            name: "X".to_owned(),
        }],
        rows: vec![Constraint {
            low: None,
            high: Some(whole(4)),
            name: "C1".to_owned(),
        }],
        cells: vec![Cell {
            column: 0,
            row: 0,
            value: whole(1),
        }],
        cancelled: Vec::new(),
        dropped: Vec::new(),
    }
}

/// The model the contradiction vectors are written against.
fn model_b() -> Instance {
    Instance {
        columns: vec![Column {
            low: Some(whole(3)),
            high: None,
            whole: false,
            cost: Frac::zero(),
            name: "X".to_owned(),
        }],
        rows: vec![Constraint {
            low: None,
            high: Some(whole(2)),
            name: "C1".to_owned(),
        }],
        ..model_a()
    }
}

/// Reads a `identifier: multiplier` entry from a vector's multiplier list.
fn term(text: &str) -> (u64, Frac) {
    let (id, value) = text
        .split_once(':')
        .expect("a multiplier entry has a colon");
    (
        id.trim().parse().expect("an identifier"),
        Frac::from_canonical(value.trim()).expect("canonical rational text"),
    )
}

fn asset() -> Asset {
    Asset::read(VECTORS).expect("the frozen vectors parse")
}

/// The tables under a prefix, minus the one that describes the model.
fn cases(asset: &Asset, prefix: &str, model: &str) -> Vec<String> {
    asset
        .tables_under(prefix)
        .into_iter()
        .filter(|table| !table.ends_with(model))
        .filter(|table| asset.items(table, "multipliers").is_some())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_dual_vector_reaches_the_verdict_and_the_bound_it_states() {
    let asset = asset();
    let instance = model_a();
    let axioms = Axioms::of(&instance);
    let tables = cases(&asset, "dual", "model_a");
    assert!(tables.len() >= 6, "the asset lost its dual vectors");

    for table in tables {
        let terms: Vec<(u64, Frac)> = asset
            .items(&table, "multipliers")
            .expect("filtered for")
            .iter()
            .map(|entry| term(entry))
            .collect();
        let verdict = asset.text(&table, "verdict").expect("a verdict");
        let outcome = dual_bound(&mut Work::new(), &axioms, &terms, &everything);

        // A vector carrying a claimed bound is about the comparison rather
        // than about the multipliers: the derivation is sound and only the
        // number written down is wrong, which is the mutation the recomputed
        // value exists to catch.
        let outcome = match asset.text(&table, "claimed_bound") {
            None => outcome.map(|_| ()),
            Some(claimed) => {
                let established = outcome.unwrap_or_else(|error| {
                    panic!("[{table}] has sound multipliers and gave {error}");
                });
                let stated = Frac::from_canonical(claimed).expect("canonical rational text");
                bound_is(&established, &stated)
            }
        };

        match verdict {
            "accept" => {
                outcome.unwrap_or_else(|error| {
                    panic!("[{table}] should be accepted and gave {error}");
                });
                let stated = asset
                    .text(&table, "bound")
                    .expect("an accepted vector states its bound");
                let established =
                    dual_bound(&mut Work::new(), &axioms, &terms, &everything).expect("accepted");
                assert_eq!(established.value.to_canonical(), stated, "[{table}] bound");
            }
            "reject" => {
                let refusal = outcome.err().unwrap_or_else(|| {
                    panic!("[{table}] should be refused and was accepted");
                });
                let stated = asset
                    .text(&table, "id")
                    .expect("a refused vector names its rule");
                assert_eq!(
                    refusal.code.text(),
                    stated,
                    "[{table}] rejection identifier"
                );
            }
            other => panic!("[{table}] states the unknown verdict {other}"),
        }
    }
}

#[test]
fn every_contradiction_vector_reaches_the_verdict_it_states() {
    let asset = asset();
    let instance = model_b();
    let axioms = Axioms::of(&instance);
    let tables = cases(&asset, "contradiction", "model_b");
    assert!(
        tables.len() >= 3,
        "the asset lost its contradiction vectors"
    );

    for table in tables {
        let terms: Vec<(u64, Frac)> = asset
            .items(&table, "multipliers")
            .expect("filtered for")
            .iter()
            .map(|entry| term(entry))
            .collect();
        let verdict = asset.text(&table, "verdict").expect("a verdict");
        let outcome = contradiction(&mut Work::new(), &axioms, &terms, &everything);

        match verdict {
            "accept" => outcome.unwrap_or_else(|error| {
                panic!("[{table}] should be accepted and gave {error}");
            }),
            "reject" => {
                let refusal = outcome.err().unwrap_or_else(|| {
                    panic!("[{table}] should be refused and was accepted");
                });
                let stated = asset
                    .text(&table, "id")
                    .expect("a refused vector names its rule");
                assert_eq!(
                    refusal.code.text(),
                    stated,
                    "[{table}] rejection identifier"
                );
            }
            other => panic!("[{table}] states the unknown verdict {other}"),
        }
    }
}

#[test]
fn the_vector_asset_names_only_rules_this_crate_implements() {
    let asset = asset();
    for entry in asset.entries() {
        if entry.key != "id" {
            continue;
        }
        let hc_verify::vspec::Value::Text(id) = &entry.value else {
            panic!(
                "[{}] states a rejection identifier that is not text",
                entry.table
            );
        };
        assert!(
            hc_verify::vreject::KNOWN
                .iter()
                .any(|code| code.text() == id),
            "[{}] names {id}, which this crate does not implement",
            entry.table
        );
    }
}
