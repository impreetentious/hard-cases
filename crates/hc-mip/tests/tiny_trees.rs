//! The tiny search, against models whose answers are known by hand.
//!
//! Small enough that the whole tree can be reasoned about on paper, which is
//! the point of an early vertical slice: the recursive proof structure is what
//! is being established, and establishing it on an instance nobody can check by
//! hand would establish nothing.

use hc_core::Model;
use hc_io::mps::{Form, read};
use hc_mip::search::{Ending, Node, Options, run};

const TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

/// Minimize -x subject to x <= 4, with 0 <= x <= 10 and x integer. The
/// relaxation's optimum is already integral, so the tree should be one node.
fn tiny() -> Model {
    read(TINY, Form::Fixed).expect("the frozen instance parses")
}

/// Free-form MPS for a model built here.
fn parse(source: &str) -> Model {
    read(source.as_bytes(), Form::Free).expect("the source parses")
}

#[test]
fn an_integral_relaxation_closes_at_the_root_with_no_split() {
    let search = run(&tiny(), &Options::default());
    assert_eq!(search.ending, Ending::Optimal);
    assert_eq!(search.tree.len(), 1, "no split was needed");
    assert!(matches!(search.nodes[0], Some(Node::Integral { .. })));

    let incumbent = search.incumbent.as_ref().expect("an incumbent");
    assert_eq!(incumbent.objective.to_canonical_string(), "-4");
    assert!(search.frontier().is_empty());
    assert_eq!(
        search
            .lower_bound()
            .map(|value| value.to_canonical_string()),
        Some("-4".to_owned())
    );
}

#[test]
fn a_fractional_relaxation_is_split_and_the_tree_closes() {
    // maximize 4x + 3y written as a minimization, subject to
    //   3x + 4y <= 12
    //   2x +  y <=  6
    // with x and y non-negative integers. The relaxation is fractional; the
    // integer optimum is x = 3, y = 0, which uses 9 of the first row's 12 and
    // all 6 of the second, for a value of 12 and a minimized objective of -12.
    // The nearby integer points are worse: (2,1) gives 11, (1,2) gives 10,
    // (0,3) gives 9.
    let model = parse(concat!(
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
    ));

    let search = run(&model, &Options::default());
    assert_eq!(
        search.ending,
        Ending::Optimal,
        "the tree should close: {:?}",
        search.ending
    );
    assert!(search.tree.len() > 1, "the relaxation is fractional");

    let incumbent = search.incumbent.as_ref().expect("an incumbent");
    assert_eq!(incumbent.objective.to_canonical_string(), "-12");
    assert!(search.frontier().is_empty());

    // Every node reached a conclusion, and none of them is unproved.
    for (id, node) in search.nodes.iter().enumerate() {
        let node = node
            .as_ref()
            .unwrap_or_else(|| panic!("node {id} has no result"));
        assert!(
            !matches!(node, Node::Unproved { .. }),
            "node {id} is unproved: {node:?}"
        );
    }
}

#[test]
fn an_infeasible_model_closes_with_no_incumbent() {
    let model = parse(concat!(
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
    ));

    let search = run(&model, &Options::default());
    assert_eq!(search.ending, Ending::Infeasible);
    assert!(search.incumbent.is_none());
    assert!(matches!(search.nodes[0], Some(Node::Infeasible { .. })));
}

#[test]
fn a_model_with_no_integer_split_available_needs_no_branching() {
    // A pure linear program: the tree is one node and the answer is the
    // relaxation's.
    let model = parse(concat!(
        "NAME          PLAIN\n",
        "ROWS\n",
        " N  obj\n",
        " L  c1\n",
        "COLUMNS\n",
        "    x         obj       -1.0       c1        2.0\n",
        "RHS\n",
        "    rhs       c1        7.0\n",
        "ENDATA\n",
    ));

    let search = run(&model, &Options::default());
    assert_eq!(search.tree.len(), 1);
    assert_eq!(search.ending, Ending::Optimal);
    let incumbent = search.incumbent.as_ref().expect("an incumbent");
    assert_eq!(incumbent.objective.to_canonical_string(), "-7/2");
}

#[test]
fn a_node_limit_leaves_a_frontier_rather_than_a_false_answer() {
    let model = parse(concat!(
        "NAME          KNAP\n",
        "ROWS\n",
        " N  obj\n",
        " L  c1\n",
        "COLUMNS\n",
        "    MARKER                 'MARKER'                 'INTORG'\n",
        "    x         obj       -7.0       c1        5.0\n",
        "    y         obj       -9.0       c1        7.0\n",
        "    z         obj       -5.0       c1        4.0\n",
        "    MARKER                 'MARKER'                 'INTEND'\n",
        "RHS\n",
        "    rhs       c1        13.0\n",
        "BOUNDS\n",
        "    UI bnd       x         9\n",
        "    UI bnd       y         9\n",
        "    UI bnd       z         9\n",
        "ENDATA\n",
    ));

    let stopped = run(
        &model,
        &Options {
            node_limit: 2,
            ..Options::default()
        },
    );
    assert!(
        matches!(stopped.ending, Ending::Frontier | Ending::NoProgress),
        "a stopped search must not claim optimality: {:?}",
        stopped.ending
    );
    assert!(!stopped.frontier().is_empty());

    let complete = run(&model, &Options::default());
    assert_eq!(complete.ending, Ending::Optimal);
    // Whatever the stopped search knew, it cannot have been better than the
    // complete one's answer.
    if let Some(early) = stopped.incumbent.as_ref() {
        let best = complete.incumbent.as_ref().expect("an incumbent");
        assert!(early.objective.cmp_value(&best.objective).is_ge());
    }
}

#[test]
fn the_search_is_reproducible_down_to_its_tree() {
    let model = tiny();
    let first = run(&model, &Options::default());
    let second = run(&model, &Options::default());
    assert_eq!(first.tree, second.tree);
    assert_eq!(first.opened, second.opened);
    assert_eq!(first.iterations, second.iterations);
    assert_eq!(
        first
            .incumbent
            .as_ref()
            .map(|found| found.objective.to_canonical_string()),
        second
            .incumbent
            .as_ref()
            .map(|found| found.objective.to_canonical_string())
    );
}
