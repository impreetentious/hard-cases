//! A search that proved no bound still has something true to say.
//!
//! When a node is left open with no exact bound, the tree covers nothing: the
//! region that node stood for is unaccounted for, so no bound follows from the
//! rest of it. A gap claim is unavailable and a tree would be worse than
//! useless — a reader who saw one would reasonably infer that a bound existed
//! somewhere in it.
//!
//! What remains is the incumbent, and the incumbent is a real thing: a point
//! that is feasible in the original model, integral where the model declares
//! it, and attaining the objective it states. The point-only form says exactly
//! that and refuses to imply more. The verifier reparses the original bytes and
//! checks the point directly, deriving no bound and reporting no gap.
//!
//! Without an incumbent there is nothing to say, and the search reports no
//! progress — which is the status the vocabulary reserves for saying nothing.

use hc_cert::hcc::{ClaimKind, Record};
use hc_cert::mip::assemble_mip;
use hc_core::Model;
use hc_cuts::pool::Pool;
use hc_io::mps::{Form, read};
use hc_mip::search::{Ending, Node, Options, Search, run};
use hc_mip::tree::Tree;
use hc_verify::vclose::verify;
use hc_verify::vhcc::Claim;

/// A knapsack big enough that a tight node budget stops the search mid-tree.
const SOURCE: &str = "NAME PO
ROWS
 N obj
 L r0
COLUMNS
 MARKER 'MARKER' 'INTORG'
 a obj -7 r0 5
 b obj -6 r0 4
 c obj -5 r0 3
 d obj -4 r0 3
 e obj -3 r0 2
 MARKER 'MARKER' 'INTEND'
RHS
 rhs r0 11
BOUNDS
 UI b a 3
 UI b b 3
 UI b c 3
 UI b d 3
 UI b e 3
ENDATA
";

fn model() -> Model {
    read(SOURCE.as_bytes(), Form::Free).expect("the instance parses")
}

/// A search that found a point and left a node unproved.
///
/// Built rather than coaxed. The states that leave a node without any bound —
/// a numerical escalation that ran out, an exact rebuild that could not be
/// completed — are the states this project works hardest to avoid reaching, so
/// provoking one on a five-column knapsack would take a contrivance that
/// proved less than it obscured. What is under test here is the assembler's
/// answer to that ending, and the ending is what it is told.
fn stalled(model: &Model) -> Search {
    let found = run(model, &Options::default());
    let incumbent = found
        .incumbent
        .clone()
        .expect("the instance has a feasible point");
    Search {
        tree: Tree::new(),
        nodes: vec![Some(Node::Unproved {
            reason: "the relaxation reached no conclusion".to_owned(),
        })],
        incumbent: Some(incumbent),
        pool: Pool::new(),
        ending: Ending::NoProgress,
        opened: 1,
        iterations: 0,
        heuristics: Vec::new(),
        spent: Vec::new(),
        separated: Vec::new(),
        unbounded: None,
    }
}

#[test]
fn a_search_that_bounded_nothing_still_certifies_its_point() {
    let model = model();
    let search = stalled(&model);
    let incumbent = search
        .incumbent
        .as_ref()
        .expect("the search carries a point")
        .clone();

    let certificate = assemble_mip(&model, SOURCE.as_bytes(), &search)
        .expect("a point-only certificate assembles");
    assert_eq!(certificate.claim, ClaimKind::FeasibleNoBound);

    let verdict = verify(&certificate.encode(), SOURCE.as_bytes())
        .unwrap_or_else(|refusal| panic!("the verifier refused an honest point: {refusal}"));
    assert_eq!(verdict.claim, Claim::PointOnly);
    assert!(
        verdict.lower.is_none() && verdict.gap.is_none(),
        "a point-only claim established a bound it does not have"
    );
    assert_eq!(
        verdict
            .objective
            .as_ref()
            .map(hc_verify::vrat::Frac::to_canonical),
        Some(incumbent.objective.to_canonical_string()),
        "the verifier recomputed a different objective than the point was said to attain"
    );
}

#[test]
fn the_point_only_form_carries_no_tree_and_no_root() {
    let model = model();
    let certificate = assemble_mip(&model, SOURCE.as_bytes(), &stalled(&model))
        .expect("a point-only certificate assembles");

    for record in &certificate.records {
        assert!(
            !matches!(
                record,
                Record::Root { .. }
                    | Record::Branch { .. }
                    | Record::LeafBound { .. }
                    | Record::LeafInfeas { .. }
                    | Record::LeafIntegral { .. }
                    | Record::LeafFrontier { .. }
                    | Record::Transform { .. }
                    | Record::DualBound { .. }
            ),
            "the point-only form carries a record that implies a bound: {record:?}"
        );
    }
}

/// With no point, there is nothing to certify and nothing is claimed.
#[test]
fn a_stalled_search_with_no_point_claims_nothing() {
    let model = model();
    let mut search = stalled(&model);
    search.incumbent = None;
    assert!(
        assemble_mip(&model, SOURCE.as_bytes(), &search).is_err(),
        "a search with neither a bound nor a point certified something"
    );
}
