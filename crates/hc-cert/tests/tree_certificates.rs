//! Search trees, written down and read back.
//!
//! What is established here is that a tree the search built serializes into a
//! well-formed certificate: parent before child, every citation backwards,
//! every leaf carrying the inference it rests on, and the whole thing surviving
//! a round trip through the reader. Whether the mathematics holds is a question
//! for the independent verifier, and it is asked elsewhere.
//!
//! The models are small enough to reason about by hand, which is the point of
//! an early slice.

use hc_cert::hcc::{ClaimKind, Record};
use hc_cert::mip::assemble_mip;
use hc_cert::read::decode;
use hc_core::Model;
use hc_io::mps::{Form, read};
use hc_mip::search::{Ending, Options, run};

const TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

const SPLIT: &str = concat!(
    "NAME          SPLIT\n",
    "ROWS\n",
    " N  obj\n",
    " L  c1\n",
    " L  c2\n",
    "COLUMNS\n",
    "    MARKER                 'MARKER'                 'INTORG'\n",
    "    x         obj       -4.0       c1        3.0\n",
    "    x         c2        2.0\n",
    "    y         obj       -3.0       c1        4.0\n",
    "    y         c2        1.0\n",
    "    MARKER                 'MARKER'                 'INTEND'\n",
    "RHS\n",
    "    rhs       c1        12.0       c2        6.0\n",
    "BOUNDS\n",
    "    UI bnd       x         10\n",
    "    UI bnd       y         10\n",
    "ENDATA\n",
);

const IMPOSSIBLE: &str = concat!(
    "NAME          NOPE\n",
    "ROWS\n",
    " N  obj\n",
    " G  c1\n",
    "COLUMNS\n",
    "    MARKER                 'MARKER'                 'INTORG'\n",
    "    x         obj       1.0        c1        1.0\n",
    "    MARKER                 'MARKER'                 'INTEND'\n",
    "RHS\n",
    "    rhs       c1        5.0\n",
    "BOUNDS\n",
    "    UP bnd       x         2\n",
    "ENDATA\n",
);

fn parse(raw: &[u8], form: Form) -> Model {
    read(raw, form).expect("the source parses")
}

/// Assembles a certificate for a model, panicking with the reason if it will
/// not assemble.
fn certificate(raw: &[u8], form: Form) -> (Vec<u8>, ClaimKind) {
    let model = parse(raw, form);
    let search = run(&model, &Options::default());
    let assembled = assemble_mip(&model, raw, &search)
        .unwrap_or_else(|reason| panic!("no certificate: {reason}"));
    (assembled.encode(), assembled.claim)
}

#[test]
fn a_root_only_tree_writes_a_root_a_bound_a_point_and_a_claim() {
    let (bytes, claim) = certificate(TINY, Form::Fixed);
    assert_eq!(claim, ClaimKind::Optimal);

    let decoded = decode(&bytes).expect("the certificate reads back");
    let kinds: Vec<u8> = decoded.records.iter().map(Record::code).collect();
    // Root, DualBound, Leaf.Integral, Incumbent, Claim, End.
    assert_eq!(kinds, vec![3, 16, 66, 80, 96, 127]);
}

#[test]
fn a_split_tree_writes_its_branches_parent_before_child() {
    let (bytes, claim) = certificate(SPLIT.as_bytes(), Form::Free);
    assert_eq!(claim, ClaimKind::Optimal);
    let decoded = decode(&bytes).expect("the certificate reads back");

    // Every branch names children that have not been created yet, which is
    // what parent-before-child means when the tree is written as a stream.
    let mut created: Vec<u64> = vec![0];
    for record in &decoded.records {
        if let Record::Branch {
            parent,
            left,
            right,
            ..
        } = record
        {
            assert!(created.contains(parent), "branch of an uncreated node");
            assert!(!created.contains(left), "left child already exists");
            assert!(!created.contains(right), "right child already exists");
            created.push(*left);
            created.push(*right);
        }
    }
    assert!(created.len() > 1, "the model needs branching");
}

#[test]
fn every_leaf_cites_an_inference_that_precedes_it_and_names_its_own_node() {
    let (bytes, _) = certificate(SPLIT.as_bytes(), Form::Free);
    let decoded = decode(&bytes).expect("the certificate reads back");

    for (position, record) in decoded.records.iter().enumerate() {
        let (node, cited) = match record {
            Record::LeafBound { node, bound } | Record::LeafFrontier { node, bound } => {
                (*node, *bound)
            }
            Record::LeafIntegral { node, bound, .. } => (*node, *bound),
            Record::LeafInfeas {
                node,
                contradiction,
            } => (*node, *contradiction),
            _ => continue,
        };
        assert!(
            cited < position as u64,
            "a leaf cites a record that does not precede it"
        );
        let inference = &decoded.records[usize::try_from(cited).expect("fits")];
        let at = match inference {
            Record::DualBound { node, .. } | Record::Contradiction { node, .. } => *node,
            other => panic!("a leaf cites {other:?}, which is not an inference"),
        };
        assert_eq!(at, node, "a leaf cites an inference made at another node");
    }
}

#[test]
fn an_infeasible_tree_carries_contradictions_and_no_incumbent() {
    let (bytes, claim) = certificate(IMPOSSIBLE.as_bytes(), Form::Free);
    assert_eq!(claim, ClaimKind::Infeasible);
    let decoded = decode(&bytes).expect("the certificate reads back");

    assert!(
        decoded
            .records
            .iter()
            .any(|record| matches!(record, Record::Contradiction { .. })),
        "an infeasible tree needs a contradiction"
    );
    assert!(
        !decoded
            .records
            .iter()
            .any(|record| matches!(record, Record::Incumbent { .. })),
        "an infeasible claim carries no incumbent"
    );
}

#[test]
fn branch_assumptions_take_identifiers_above_the_model_and_in_record_order() {
    let model = parse(SPLIT.as_bytes(), Form::Free);
    let base = model.inequalities().len() as u64;
    let (bytes, _) = certificate(SPLIT.as_bytes(), Form::Free);
    let decoded = decode(&bytes).expect("the certificate reads back");

    // Every citation is either a model inequality or an assumption identifier
    // the stream had already allocated by the time it was cited.
    let mut allocated = base;
    for record in &decoded.records {
        match record {
            Record::Branch { .. } => allocated += 2,
            Record::DualBound { terms, .. } | Record::Contradiction { terms, .. } => {
                for (id, _) in terms {
                    assert!(
                        *id < allocated,
                        "an inference cites identifier {id} before it exists"
                    );
                }
            }
            _ => {}
        }
    }
    assert!(allocated > base, "the tree branched and allocated nothing");
}

#[test]
fn a_stopped_search_writes_a_gap_rather_than_optimality() {
    let model = parse(SPLIT.as_bytes(), Form::Free);
    let search = run(
        &model,
        &Options {
            node_limit: 3,
            ..Options::default()
        },
    );
    match search.ending {
        Ending::Optimal => {
            // Three nodes were enough. Nothing to assert about a gap, and
            // saying so beats asserting something that is not true here.
        }
        Ending::Frontier => {
            let assembled = assemble_mip(&model, SPLIT.as_bytes(), &search)
                .unwrap_or_else(|reason| panic!("no certificate: {reason}"));
            assert_eq!(assembled.claim, ClaimKind::FeasibleWithGap);
            let claim = assembled
                .records
                .iter()
                .find_map(|record| match record {
                    Record::Claim {
                        kind, lower, gap, ..
                    } => Some((*kind, lower.clone(), gap.clone())),
                    _ => None,
                })
                .expect("a claim record");
            assert_eq!(claim.0, ClaimKind::FeasibleWithGap);
            assert!(claim.1.is_some(), "a gap claim states a lower bound");
            assert!(claim.2.is_some(), "a gap claim states a gap");
        }
        other => {
            // No progress and an unbounded relaxation are honest outcomes and
            // neither produces a certificate.
            assert!(
                assemble_mip(&model, SPLIT.as_bytes(), &search).is_err(),
                "{other:?} must not produce a certificate"
            );
        }
    }
}
