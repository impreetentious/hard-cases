//! Supplied solutions, and what a stopped search leaves behind.
//!
//! Two properties, and both are about honesty rather than speed.
//!
//! A supplied solution is a candidate, not an incumbent. It goes through the
//! same exact screening every other candidate does, because the incumbent is
//! what every prune in the tree is measured against and a wrong one silently
//! discards the answer.
//!
//! And a search that runs out of budget bounds what it has not explored rather
//! than abandoning it. An open node with an exact bound is a frontier and can
//! be certified as a gap; an open node with no bound is a hole, and a tree with
//! a hole proves nothing at all.

use hc_exact::Rational;
use hc_io::mps::{Form, read};
use hc_mip::search::{Ending, Node, Options, run};

const KNAPSACK: &str = concat!(
    "NAME K\n",
    "ROWS\n",
    " N obj\n",
    " L c1\n",
    "COLUMNS\n",
    " MARKER 'MARKER' 'INTORG'\n",
    " x obj -7 c1 5\n",
    " y obj -9 c1 7\n",
    " z obj -5 c1 4\n",
    " MARKER 'MARKER' 'INTEND'\n",
    "RHS\n",
    " rhs c1 13\n",
    "BOUNDS\n",
    " UI b x 3\n",
    " UI b y 3\n",
    " UI b z 3\n",
    "ENDATA\n",
);

fn model() -> hc_core::Model {
    read(KNAPSACK.as_bytes(), Form::Free).expect("the source parses")
}

fn whole(value: i64) -> Rational {
    Rational::from_i64(value)
}

/// A point over the working shape: the three columns and the row's activity.
fn point(x: i64, y: i64, z: i64) -> Vec<Rational> {
    let activity = 5 * x + 7 * y + 4 * z;
    vec![whole(x), whole(y), whole(z), whole(activity)]
}

fn with_start(start: Option<Vec<Rational>>) -> Options {
    Options {
        start,
        ..Options::default()
    }
}

#[test]
fn a_genuine_supplied_solution_becomes_the_incumbent_before_the_search_begins() {
    // x = 1, z = 2 fills the knapsack exactly for 17, which is the optimum.
    let search = run(&model(), &with_start(Some(point(1, 0, 2))));
    assert_eq!(search.ending, Ending::Optimal);
    assert_eq!(
        search
            .incumbent
            .as_ref()
            .map(|found| found.objective.to_canonical_string()),
        Some("-17".to_owned())
    );
}

#[test]
fn a_supplied_solution_that_is_not_feasible_is_refused_rather_than_repaired() {
    // Weights 5, 7 and 4 against a capacity of 13: three of each is far over.
    let search = run(&model(), &with_start(Some(point(3, 3, 3))));
    assert_eq!(search.ending, Ending::Optimal);
    assert_eq!(
        search
            .incumbent
            .as_ref()
            .map(|found| found.objective.to_canonical_string()),
        Some("-17".to_owned()),
        "the search should reach its own answer, not the supplied one"
    );
}

#[test]
fn a_supplied_solution_of_the_wrong_shape_is_ignored() {
    let search = run(&model(), &with_start(Some(vec![whole(1)])));
    assert_eq!(search.ending, Ending::Optimal);
    assert!(search.incumbent.is_some());
}

#[test]
fn a_worse_supplied_solution_does_not_hold_the_answer_back() {
    // x = 1 alone is feasible and worth 7, well short of the optimum.
    let search = run(&model(), &with_start(Some(point(1, 0, 0))));
    assert_eq!(
        search
            .incumbent
            .as_ref()
            .map(|found| found.objective.to_canonical_string()),
        Some("-17".to_owned())
    );
}

#[test]
fn a_stopped_search_bounds_what_it_did_not_explore() {
    for limit in [1_u64, 2, 3, 4, 5] {
        let search = run(
            &model(),
            &Options {
                node_limit: limit,
                ..Options::default()
            },
        );
        // Whatever it managed, no node is left without a conclusion.
        for (id, node) in search.nodes.iter().enumerate() {
            let node = node
                .as_ref()
                .unwrap_or_else(|| panic!("limit {limit}: node {id} has no result"));
            assert!(
                !matches!(node, Node::Unproved { .. }),
                "limit {limit}: node {id} was abandoned: {node:?}"
            );
        }
        assert!(
            matches!(search.ending, Ending::Optimal | Ending::Frontier),
            "limit {limit} ended {:?}",
            search.ending
        );
        // And a stopped search with a frontier has a finite global bound.
        if search.ending == Ending::Frontier {
            assert!(search.lower_bound().is_some());
        }
    }
}
