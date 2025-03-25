//! Every normative asset is inside the accepted subset, read by this side's
//! reader.
//!
//! The continuous-integration document generator already reads these files
//! with its own reader, written separately. That is the point of checking them
//! again here: two readers accepting the same bytes is evidence the subset is
//! real, and an asset that only one of them accepts is a defect in whichever
//! one is wrong. The verifier will make it three.
//!
//! The list is written out rather than discovered, because the assets are
//! compiled in. A crate inside the deterministic perimeter does not read the
//! filesystem, so `include_str!` is how a test reaches inert data at all —
//! and an explicit list has the useful side effect that adding an asset
//! without adding it here is visible in review.

use hc_core::SpecDocument;

/// The name and contents of every asset, in directory order.
const ASSETS: &[(&str, &str)] = &[
    (
        "canonical.toml",
        include_str!("../../../spec/v1/canonical.toml"),
    ),
    ("claims.toml", include_str!("../../../spec/v1/claims.toml")),
    ("cuts.toml", include_str!("../../../spec/v1/cuts.toml")),
    ("hcc.toml", include_str!("../../../spec/v1/hcc.toml")),
    ("mps.toml", include_str!("../../../spec/v1/mps.toml")),
    (
        "numeric.toml",
        include_str!("../../../spec/v1/numeric.toml"),
    ),
    (
        "rejections.toml",
        include_str!("../../../spec/v1/rejections.toml"),
    ),
    ("rng.toml", include_str!("../../../spec/v1/rng.toml")),
    (
        "transforms.toml",
        include_str!("../../../spec/v1/transforms.toml"),
    ),
    (
        "vectors/arith/arithmetic.toml",
        include_str!("../../../spec/v1/vectors/arith/arithmetic.toml"),
    ),
    (
        "vectors/hash/hashes.toml",
        include_str!("../../../spec/v1/vectors/hash/hashes.toml"),
    ),
    (
        "vectors/mps/manifest.toml",
        include_str!("../../../spec/v1/vectors/mps/manifest.toml"),
    ),
    (
        "vectors/proof/inferences.toml",
        include_str!("../../../spec/v1/vectors/proof/inferences.toml"),
    ),
    (
        "vectors/proof/transforms.toml",
        include_str!("../../../spec/v1/vectors/proof/transforms.toml"),
    ),
];

#[test]
fn every_asset_is_inside_the_accepted_subset() {
    for (name, text) in ASSETS {
        let document = match SpecDocument::parse(text) {
            Ok(document) => document,
            Err(error) => panic!("{name}: {error}"),
        };
        assert!(!document.tables().is_empty(), "{name}: no tables");
    }
}

#[test]
fn every_asset_declares_its_version_and_title() {
    for (name, text) in ASSETS {
        let document = SpecDocument::parse(text).expect("inside the subset");
        let meta = document
            .table("meta")
            .unwrap_or_else(|| panic!("{name}: no meta table"));
        assert_eq!(meta.integer("version"), Some(1), "{name}: meta version");
        assert!(meta.text("title").is_some(), "{name}: meta title");
        assert!(meta.text("summary").is_some(), "{name}: meta summary");
    }
}

#[test]
fn the_asset_list_is_not_silently_empty() {
    // A conformance test over an empty list passes while checking nothing,
    // which is the worst thing a check can do.
    assert!(ASSETS.len() >= 14);
}

/// The rejection registry against the asset, in both directions.
///
/// The registry is written by hand rather than generated, so this is where the
/// two are held together. Generation would make them agree by construction,
/// which is not agreement.
#[test]
fn the_rejection_registry_matches_the_asset() {
    use hc_core::reject::{COVERED_GROUPS, REGISTERED};

    let document = SpecDocument::parse(include_str!("../../../spec/v1/rejections.toml"))
        .expect("inside the subset");
    let declared: Vec<&str> = document
        .tables_under("reject")
        .map(|table| table.name().trim_start_matches("reject."))
        .collect();

    // Nothing named here may be absent from the specification.
    for entry in REGISTERED {
        assert!(
            declared.contains(&entry.as_str()),
            "{entry} is named in the registry and not declared in the asset"
        );
        assert!(
            COVERED_GROUPS.contains(&entry.group()),
            "{entry} belongs to a group this crate does not claim to cover"
        );
    }

    // And nothing in a covered group may be missing from here.
    for id in &declared {
        let group = &id[..id.rfind('-').expect("an identifier has a group")];
        if !COVERED_GROUPS.contains(&group) {
            continue;
        }
        assert!(
            REGISTERED.iter().any(|entry| entry.as_str() == *id),
            "{id} is declared in the asset and missing from the registry"
        );
    }

    assert!(
        !REGISTERED.is_empty(),
        "the registry is empty and the check is vacuous"
    );
}

/// The terminal vocabulary against the asset.
#[test]
fn the_claim_vocabulary_matches_the_asset() {
    use hc_core::{Claim, NoProgressReason};

    let document = SpecDocument::parse(include_str!("../../../spec/v1/claims.toml"))
        .expect("inside the subset");

    let stated: Vec<&str> = document
        .tables_under("claim")
        .map(|table| {
            table
                .text("name")
                .unwrap_or_else(|| panic!("[{}] has no name", table.name()))
        })
        .collect();
    let named: Vec<&str> = Claim::all().iter().map(|claim| claim.as_str()).collect();
    assert_eq!(
        stated, named,
        "the vocabulary and the asset disagree, in this order"
    );

    let reasons = document
        .table("claim.no_progress")
        .and_then(|table| table.get("reasons"))
        .and_then(hc_core::SpecValue::as_list)
        .expect("the asset lists the reasons");
    let ours: Vec<&str> = NoProgressReason::all()
        .iter()
        .map(|reason| reason.as_str())
        .collect();
    assert_eq!(
        reasons, ours,
        "the reasons and the asset disagree, in this order"
    );
}

/// The gap formula the asset states is the one the code will recompute.
#[test]
fn the_gap_formula_is_the_one_the_asset_states() {
    let document = SpecDocument::parse(include_str!("../../../spec/v1/claims.toml"))
        .expect("inside the subset");
    let gap = document.table("gap").expect("the gap table is present");
    assert_eq!(gap.text("formula"), Some("(U - L) / max(1, |U|)"));
    // A negative gap is an invariant violation rather than a small number, and
    // the asset has to keep saying so, because the temptation to clamp it is
    // exactly what the rule exists to refuse.
    assert!(
        gap.text("negative")
            .is_some_and(|text| text.contains("panic"))
    );
}
