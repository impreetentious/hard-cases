//! Replaying a search tree, without trusting any of it.
//!
//! A branch-and-bound certificate is a tree, and what makes a tree a proof is
//! that its children partition their parent. Two children with assumptions
//! `x <= k` and `x >= k + 1`, for an integer `k` and a variable the model
//! declares integer, are disjoint and together cover every integer point the
//! parent admitted. Nothing else about the search matters: not the order nodes
//! were explored in, not why a variable was chosen, not what the solver
//! believed at the time.
//!
//! So the replay checks exactly the properties the partition argument needs.
//! Every node has one parent and is reachable from the root, so there is no
//! orphan subtree quietly holding the answer. Every node is either branched or
//! terminated exactly once, so there is no region nobody accounted for. Every
//! leaf cites an inference made at its own node, because an inference made
//! somewhere else is about somewhere else. And every citation points backwards,
//! because a proof that could cite its own conclusion is not a proof.
//!
//! The replay is iterative throughout. A legal certificate may be a hundred
//! thousand nodes deep, which is deeper than the process call stack, and a
//! verifier that overflowed on a valid proof would have rejected it for a
//! reason with nothing to do with mathematics. There is no recursion here and
//! there is not meant to be.

use crate::vaxiom::{Axioms, Inequality};
use crate::vcut::{View, cover, gomory};
use crate::vhcc::{Proof, Step};
use crate::vmodel::Instance;
use crate::vrat::Frac;
use crate::vreject::{Code, Rejected};
use crate::vrules::{contradiction, dual_bound, objective_is, point_is_feasible};
use crate::vwork::Work;

/// One cut the replay has introduced.
///
/// The identifier it took, the node whose subtree it is true of, and the
/// inequality itself — which is here because a later cut may be derived from
/// it, exactly as one may be derived from a row of the model.
#[derive(Clone, Debug)]
struct PooledCut {
    id: u64,
    scope: u64,
    terms: Vec<(u32, Frac)>,
    right: Frac,
}

/// The deepest tree the replay will follow.
pub const DEPTH_CAP: u32 = 100_000;

/// How a node was closed, if it was.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Terminal {
    /// Closed by a bound, which must be at least the incumbent.
    Bound(Frac),
    /// Closed by a contradiction.
    Infeasible,
    /// Closed by an integral point attaining its own bound.
    Integral(Frac),
    /// Left open with a finite bound.
    Frontier(Frac),
}

/// One node, as the replay builds it.
#[derive(Clone, Debug)]
struct NodeState {
    parent: Option<u64>,
    depth: u32,
    /// The identifier of the inequality this node's assumption introduced.
    assumption: Option<u64>,
    /// The bound the assumption narrows, as a column, a side, and a value.
    ///
    /// The same fact as the assumption inequality, in the shape a cut
    /// derivation needs: a cut is built from the bounds in force at its node,
    /// and those are the model's narrowed by the path.
    narrowing: Option<(usize, bool, Frac)>,
    branched: bool,
    terminal: Option<Terminal>,
}

/// What a replayed tree establishes.
#[derive(Clone, Debug)]
pub struct Replay {
    /// The lowest bound among the open nodes, when any are open.
    pub frontier: Option<Frac>,
    /// Whether every leaf is a contradiction.
    pub all_infeasible: bool,
    /// Whether any node was left open.
    pub open: bool,
    /// The lowest bound among the closed leaves that carry one.
    pub weakest_closed: Option<Frac>,
}

/// Replays the tree in a certificate and reports what it establishes.
///
/// `axioms` is consumed and extended: branch assumptions are introduced into
/// it as their records are read, taking identifiers above the model's base
/// block in record order. That is how both implementations arrive at the same
/// identifiers without either being told.
///
/// # Errors
///
/// Returns [`Rejected`] naming the earliest rule that refused.
pub fn replay(
    work: &mut Work,
    proof: &Proof,
    model: &Instance,
    axioms: &mut Axioms<'_>,
    incumbent: Option<&Frac>,
) -> Result<Replay, Rejected> {
    let mut nodes: Vec<NodeState> = vec![NodeState {
        parent: None,
        depth: 0,
        assumption: None,
        narrowing: None,
        branched: false,
        terminal: None,
    }];

    // Everything below the model's own inequality count is visible everywhere;
    // everything at or above it was introduced by this certificate and is
    // visible only where its scope says.
    let base = axioms.base();

    // What each inference record established, indexed by record identifier.
    let mut inferences: Vec<Option<(u64, Frac)>> = Vec::new();
    let mut contradictions: Vec<Option<u64>> = Vec::new();
    // The cuts introduced so far, as an inequality identifier and the node
    // whose subtree the cut may be used in.
    let mut cuts: Vec<PooledCut> = Vec::new();

    for (position, step) in proof.steps.iter().enumerate() {
        let id = position as u64;
        inferences.push(None);
        contradictions.push(None);

        match step {
            // The chain was replayed before the tree was reached, and the
            // incumbent is checked by the closure. Both are skipped here
            // rather than refused.
            Step::Root { .. }
            | Step::Assertion { .. }
            | Step::Terminator { .. }
            | Step::Transform { .. }
            | Step::TransformContradiction { .. }
            | Step::Incumbent { .. } => {}
            Step::Branch {
                parent,
                variable,
                split,
                left,
                right,
            } => {
                branch(
                    &mut nodes, axioms, model, *parent, *variable, split, *left, *right,
                )?;
            }
            Step::GmiCut {
                scope,
                combination,
                isolated,
                stated_coefficients,
                stated_right,
            } => {
                let view = at_node(&nodes, model, &cuts, *scope)?;
                // The rows a combination may use are the model's own and the
                // cuts already visible where this one is derived. A cut from a
                // sibling subtree is not among them.
                let recomputed = gomory(&view, combination, *isolated)?;
                compare(
                    &recomputed,
                    stated_coefficients,
                    Some(stated_right),
                    Code::CUT_GMI_COEFFICIENT,
                )?;
                let id = axioms.introduce(Inequality {
                    terms: recomputed.coefficients.clone(),
                    right: recomputed.right.clone(),
                });
                cuts.push(PooledCut {
                    id,
                    scope: *scope,
                    terms: recomputed.coefficients,
                    right: recomputed.right,
                });
            }
            Step::CoverCut {
                scope,
                source,
                cover: members,
                lift_order,
                stated_coefficients,
            } => {
                let view = at_node(&nodes, model, &cuts, *scope)?;
                let recomputed = cover(model, &view, *source, members, lift_order)?;
                compare(
                    &recomputed,
                    stated_coefficients,
                    None,
                    Code::CUT_LIFT_COEFFICIENT,
                )?;
                let id = axioms.introduce(Inequality {
                    terms: recomputed.coefficients.clone(),
                    right: recomputed.right.clone(),
                });
                cuts.push(PooledCut {
                    id,
                    scope: *scope,
                    terms: recomputed.coefficients,
                    right: recomputed.right,
                });
            }
            Step::DualBound { node, terms } => {
                let visible = visibility(&nodes, base, &cuts, *node)?;
                let bound = dual_bound(work, axioms, terms, &|cited| visible(cited))?;
                inferences[position] = Some((*node, bound.value));
            }
            Step::Contradiction { node, terms } => {
                let visible = visibility(&nodes, base, &cuts, *node)?;
                contradiction(work, axioms, terms, &|cited| visible(cited))?;
                contradictions[position] = Some(*node);
            }
            Step::LeafBound { node, bound } => {
                let value = cited_bound(&inferences, *bound, *node, id)?;
                terminate(&mut nodes, *node, Terminal::Bound(value))?;
            }
            Step::LeafFrontier { node, bound } => {
                let value = cited_bound(&inferences, *bound, *node, id)?;
                terminate(&mut nodes, *node, Terminal::Frontier(value))?;
            }
            Step::LeafInfeas {
                node,
                contradiction: cited,
            } => {
                cited_contradiction(&contradictions, *cited, *node, id)?;
                terminate(&mut nodes, *node, Terminal::Infeasible)?;
            }
            Step::LeafIntegral { node, point, bound } => {
                let value = cited_bound(&inferences, *bound, *node, id)?;
                // The point proves nothing on its own; it has to be feasible
                // in the model and attain the bound the node established.
                point_is_feasible(work, axioms, point)
                    .map_err(|refusal| Rejected::new(Code::TREE_INTEGRAL_POINT, refusal.detail))?;
                objective_is(work, axioms, point, &value).map_err(|_| {
                    Rejected::new(
                        Code::TREE_INTEGRAL_OBJECTIVE,
                        format!("the integral point at node {node} does not attain {value}"),
                    )
                })?;
                terminate(&mut nodes, *node, Terminal::Integral(value))?;
            }
            other => {
                return Err(Rejected::new(
                    Code::CLAIM_NOT_CLOSED,
                    format!(
                        "a record of type {} is not part of a cut-free tree",
                        other.code()
                    ),
                ));
            }
        }
    }

    finish(&nodes, incumbent)
}

/// Creates two children under a parent.
#[allow(clippy::too_many_arguments)]
fn branch(
    nodes: &mut Vec<NodeState>,
    axioms: &mut Axioms<'_>,
    model: &Instance,
    parent: u64,
    variable: u64,
    split: &Frac,
    left: u64,
    right: u64,
) -> Result<(), Rejected> {
    let column = usize::try_from(variable).map_err(|_| {
        Rejected::new(
            Code::TREE_BRANCH_NOT_INTEGER,
            format!("variable {variable} is outside the model"),
        )
    })?;
    let Some(declared) = model.columns.get(column) else {
        return Err(Rejected::new(
            Code::TREE_BRANCH_NOT_INTEGER,
            format!("variable {variable} is outside the model"),
        ));
    };
    if !declared.whole {
        return Err(Rejected::new(
            Code::TREE_BRANCH_NOT_INTEGER,
            format!("variable {variable} is not declared integer"),
        ));
    }
    if !split.is_whole() {
        return Err(Rejected::new(
            Code::TREE_SPLIT_NOT_INTEGER,
            format!("the split value {split} is not an integer"),
        ));
    }

    let parent_slot = usize::try_from(parent).map_err(|_| {
        Rejected::new(
            Code::TREE_UNREACHABLE,
            format!("node {parent} does not exist"),
        )
    })?;
    let Some(state) = nodes.get(parent_slot) else {
        return Err(Rejected::new(
            Code::TREE_UNREACHABLE,
            format!("node {parent} does not exist"),
        ));
    };
    if state.branched || state.terminal.is_some() {
        return Err(Rejected::new(
            Code::TREE_DOUBLE_TERMINAL,
            format!("node {parent} is closed or already branched"),
        ));
    }
    let depth = state.depth + 1;
    if depth > DEPTH_CAP {
        return Err(Rejected::new(
            Code::TREE_DEPTH,
            format!("the tree reaches depth {depth}"),
        ));
    }

    // The two children take the next two identifiers, left before right. That
    // ordering is the whole of the agreement between the two implementations
    // about what a citation means.
    let column32 = u32::try_from(column).map_err(|_| {
        Rejected::new(
            Code::TREE_BRANCH_NOT_INTEGER,
            format!("variable {variable} is outside the model"),
        )
    })?;
    let left_id = axioms.introduce(Inequality {
        terms: vec![(column32, Frac::one().negated())],
        right: split.negated(),
    });
    let right_id = axioms.introduce(Inequality {
        terms: vec![(column32, Frac::one())],
        right: split.plus(&Frac::one()),
    });

    let narrowings = [
        (left, left_id, (column, true, split.clone())),
        (right, right_id, (column, false, split.plus(&Frac::one()))),
    ];
    for (child, assumption, narrowing) in narrowings {
        let slot = usize::try_from(child).map_err(|_| {
            Rejected::new(
                Code::TREE_CHILD_EXISTS,
                format!("child identifier {child} is not a position"),
            )
        })?;
        if slot < nodes.len() {
            return Err(Rejected::new(
                Code::TREE_CHILD_EXISTS,
                format!("node {child} already exists"),
            ));
        }
        if slot != nodes.len() {
            return Err(Rejected::new(
                Code::TREE_MULTIPLE_PARENTS,
                format!("node {child} leaves a gap in the tree"),
            ));
        }
        nodes.push(NodeState {
            parent: Some(parent),
            depth,
            assumption: Some(assumption),
            narrowing: Some(narrowing),
            branched: false,
            terminal: None,
        });
    }

    nodes[parent_slot].branched = true;
    Ok(())
}

/// Which inequalities a node may cite.
///
/// The model's own, always, plus the assumptions on the path from the root to
/// this node. The path is walked with a loop and a parent pointer; a
/// hundred-thousand-node lineage costs a hundred thousand steps and no stack.
fn visibility(
    nodes: &[NodeState],
    base: u64,
    cuts: &[PooledCut],
    node: u64,
) -> Result<impl Fn(u64) -> bool + 'static, Rejected> {
    let slot = usize::try_from(node).ok().filter(|at| *at < nodes.len());
    let Some(mut at) = slot else {
        return Err(Rejected::new(
            Code::SCOPE_NOT_VISIBLE,
            format!("an inference names node {node}, which does not exist"),
        ));
    };

    let mut assumptions: Vec<u64> = Vec::new();
    let mut lineage: Vec<u64> = Vec::new();
    loop {
        let state = &nodes[at];
        lineage.push(at as u64);
        if let Some(id) = state.assumption {
            assumptions.push(id);
        }
        match state.parent {
            None => break,
            Some(parent) => {
                at = usize::try_from(parent).map_err(|_| {
                    Rejected::new(Code::TREE_UNREACHABLE, "a parent identifier is not a node")
                })?;
            }
        }
    }
    assumptions.sort_unstable();

    // A cut is visible where it was derived and below, and nowhere else. That
    // is the rule a sibling citation breaks, and it is enforced here and again
    // by the solver: a cut escaping its subtree would invalidate every
    // descendant proof that used the pool entry.
    let mut visible_cuts: Vec<u64> = cuts
        .iter()
        .filter(|cut| lineage.contains(&cut.scope))
        .map(|cut| cut.id)
        .collect();
    visible_cuts.sort_unstable();

    Ok(move |cited: u64| {
        cited < base
            || assumptions.binary_search(&cited).is_ok()
            || visible_cuts.binary_search(&cited).is_ok()
    })
}

/// The bounds in force at a node: the model's, narrowed by the assumptions on
/// the path from the root.
///
/// A cut derived at a node is a statement about that node's region, so the
/// displacements it is built from are measured from that region's bounds.
fn at_node(
    nodes: &[NodeState],
    model: &Instance,
    cuts: &[PooledCut],
    node: u64,
) -> Result<View, Rejected> {
    let mut view = View::of(model);
    let Some(mut at) = usize::try_from(node).ok().filter(|at| *at < nodes.len()) else {
        return Err(Rejected::new(
            Code::SCOPE_BAD_SCOPE_NODE,
            format!("a cut names node {node}, which does not exist"),
        ));
    };
    loop {
        let state = &nodes[at];
        if let Some((column, at_most, value)) = &state.narrowing {
            if *at_most {
                view.narrow(*column, None, Some(value));
            } else {
                view.narrow(*column, Some(value), None);
            }
        }
        match state.parent {
            None => break,
            Some(parent) => {
                at = usize::try_from(parent).map_err(|_| {
                    Rejected::new(Code::TREE_UNREACHABLE, "a parent identifier is not a node")
                })?;
            }
        }
    }

    // The cuts already visible here become rows of the relaxation, in the
    // order the stream introduced them, so a later combination that names one
    // means the same row on both sides.
    let mut lineage = Vec::new();
    let mut at = usize::try_from(node).unwrap_or(usize::MAX);
    while at < nodes.len() {
        lineage.push(at as u64);
        match nodes[at].parent {
            None => break,
            Some(parent) => at = usize::try_from(parent).unwrap_or(usize::MAX),
        }
    }
    for cut in cuts {
        if lineage.contains(&cut.scope) {
            view.add_cut(cut.terms.clone(), cut.right.clone());
        }
    }
    Ok(view)
}

/// Compares a recomputed cut against the one a record states.
///
/// The recomputation decides. The serialized coefficients are compared so that
/// a certificate whose separation routine and whose record disagree is caught,
/// and they are never read as an input to the check.
fn compare(
    recomputed: &crate::vcut::Recomputed,
    stated: &[(u64, Frac)],
    right: Option<&Frac>,
    code: Code,
) -> Result<(), Rejected> {
    let ours: Vec<(u64, Frac)> = recomputed
        .coefficients
        .iter()
        .map(|(column, value)| (u64::from(*column), value.clone()))
        .collect();
    if ours != stated {
        return Err(Rejected::new(
            code,
            "the recomputed cut differs from the one the record states",
        ));
    }
    if let Some(stated) = right
        && *stated != recomputed.right
    {
        return Err(Rejected::new(
            code,
            format!(
                "the recomputed right-hand side is {} and the record states {stated}",
                recomputed.right
            ),
        ));
    }
    Ok(())
}

/// The bound a leaf cites, checked to be an inference at the leaf's own node.
fn cited_bound(
    inferences: &[Option<(u64, Frac)>],
    cited: u64,
    node: u64,
    leaf: u64,
) -> Result<Frac, Rejected> {
    if cited >= leaf {
        return Err(Rejected::new(
            Code::SCOPE_FORWARD,
            format!("a leaf cites record {cited}, which does not precede it"),
        ));
    }
    let Some(Some((at, value))) = inferences.get(usize::try_from(cited).unwrap_or(usize::MAX))
    else {
        return Err(Rejected::new(
            Code::TREE_LEAF_NODE,
            format!("record {cited} is not a bound"),
        ));
    };
    if *at != node {
        return Err(Rejected::new(
            Code::TREE_LEAF_NODE,
            format!("node {node} cites a bound made at node {at}"),
        ));
    }
    Ok(value.clone())
}

/// The contradiction a leaf cites, checked the same way.
fn cited_contradiction(
    contradictions: &[Option<u64>],
    cited: u64,
    node: u64,
    leaf: u64,
) -> Result<(), Rejected> {
    if cited >= leaf {
        return Err(Rejected::new(
            Code::SCOPE_FORWARD,
            format!("a leaf cites record {cited}, which does not precede it"),
        ));
    }
    let Some(Some(at)) = contradictions.get(usize::try_from(cited).unwrap_or(usize::MAX)) else {
        return Err(Rejected::new(
            Code::TREE_LEAF_NODE,
            format!("record {cited} is not a contradiction"),
        ));
    };
    if *at != node {
        return Err(Rejected::new(
            Code::TREE_LEAF_NODE,
            format!("node {node} cites a contradiction made at node {at}"),
        ));
    }
    Ok(())
}

/// Records how a node was closed, refusing a second closure.
fn terminate(nodes: &mut [NodeState], node: u64, how: Terminal) -> Result<(), Rejected> {
    let slot = usize::try_from(node)
        .ok()
        .filter(|at| *at < nodes.len())
        .ok_or_else(|| {
            Rejected::new(
                Code::TREE_UNREACHABLE,
                format!("node {node} does not exist"),
            )
        })?;
    if nodes[slot].terminal.is_some() || nodes[slot].branched {
        return Err(Rejected::new(
            Code::TREE_DOUBLE_TERMINAL,
            format!("node {node} is closed or branched more than once"),
        ));
    }
    nodes[slot].terminal = Some(how);
    Ok(())
}

/// Checks the cover and reports what the tree establishes.
fn finish(nodes: &[NodeState], incumbent: Option<&Frac>) -> Result<Replay, Rejected> {
    let mut frontier: Option<Frac> = None;
    let mut weakest_closed: Option<Frac> = None;
    let mut all_infeasible = true;
    let mut open = false;

    for (index, state) in nodes.iter().enumerate() {
        let id = index as u64;
        if state.branched {
            if state.terminal.is_some() {
                return Err(Rejected::new(
                    Code::TREE_DOUBLE_TERMINAL,
                    format!("node {id} is both branched and terminated"),
                ));
            }
            continue;
        }
        let Some(terminal) = &state.terminal else {
            return Err(Rejected::new(
                Code::TREE_UNTERMINATED,
                format!("node {id} is neither branched nor terminated"),
            ));
        };
        match terminal {
            Terminal::Infeasible => {}
            Terminal::Bound(value) | Terminal::Integral(value) => {
                all_infeasible = false;
                if let Some(best) = incumbent
                    && value.compare(best) == core::cmp::Ordering::Less
                {
                    return Err(Rejected::new(
                        Code::CLAIM_LEAF_BELOW,
                        format!("node {id} closes at {value}, below the incumbent {best}"),
                    ));
                }
                weakest_closed = Some(match weakest_closed {
                    None => value.clone(),
                    Some(seen) if value.compare(&seen) == core::cmp::Ordering::Less => {
                        value.clone()
                    }
                    Some(seen) => seen,
                });
            }
            Terminal::Frontier(value) => {
                all_infeasible = false;
                open = true;
                // A frontier whose bound already meets the incumbent is not a
                // frontier. Encoding it as one would let a complete tree be
                // presented as a partial result, or a zero gap be manufactured
                // out of a node that was actually closed.
                if let Some(best) = incumbent
                    && value.compare(best) != core::cmp::Ordering::Less
                {
                    return Err(Rejected::new(
                        Code::TREE_FRONTIER_CLOSED,
                        format!("node {id} is open at {value}, which already meets {best}"),
                    ));
                }
                frontier = Some(match frontier {
                    None => value.clone(),
                    Some(seen) if value.compare(&seen) == core::cmp::Ordering::Less => {
                        value.clone()
                    }
                    Some(seen) => seen,
                });
            }
        }
    }

    // Reachability, iteratively. Every node's parent chain must reach the
    // root; a cycle would spin forever, so the walk is bounded by the number
    // of nodes and a walk that runs out of budget is a cycle.
    for index in 0..nodes.len() {
        let mut at = index;
        let mut steps = 0;
        while let Some(parent) = nodes[at].parent {
            at = usize::try_from(parent).unwrap_or(usize::MAX);
            if at >= nodes.len() {
                return Err(Rejected::new(
                    Code::TREE_UNREACHABLE,
                    format!("node {index} has a parent that does not exist"),
                ));
            }
            steps += 1;
            if steps > nodes.len() {
                return Err(Rejected::new(
                    Code::TREE_UNREACHABLE,
                    format!("node {index} does not reach the root"),
                ));
            }
        }
        if at != 0 {
            return Err(Rejected::new(
                Code::TREE_UNREACHABLE,
                format!("node {index} is not reachable from the root"),
            ));
        }
    }

    Ok(Replay {
        frontier,
        all_infeasible,
        open,
        weakest_closed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmodel::{Cell, Column, Constraint, Direction, Tag, whole};

    /// Minimize -x subject to x <= 4, with 0 <= x <= 10 and x integer. Base
    /// inequalities: 0 is the row's upper side, 1 the variable's lower bound,
    /// 2 its upper bound.
    fn model() -> Instance {
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

    /// Wraps records into a proof, without the header checks.
    fn proof(steps: Vec<Step>) -> Proof {
        Proof {
            tag: 1,
            instance_bytes: 0,
            digest: [0; 32],
            claim: crate::vhcc::Claim::Optimal,
            bytes: 0,
            spans: Vec::new(),
            steps,
        }
    }

    fn run(steps: Vec<Step>, incumbent: Option<Frac>) -> Result<Replay, Rejected> {
        let instance = model();
        let mut axioms = Axioms::of(&instance);
        replay(
            &mut Work::new(),
            &proof(steps),
            &instance,
            &mut axioms,
            incumbent.as_ref(),
        )
    }

    fn root() -> Step {
        Step::Root {
            node: 0,
            stated_hash: [0; 32],
        }
    }

    /// A bound of -4 at a node, from the row's upper side alone.
    fn bound(node: u64) -> Step {
        Step::DualBound {
            node,
            terms: vec![(0, Frac::one())],
        }
    }

    #[test]
    fn a_single_closed_root_is_a_complete_cover() {
        let replayed = run(
            vec![root(), bound(0), Step::LeafBound { node: 0, bound: 1 }],
            Some(whole(-4)),
        )
        .expect("a complete cover");
        assert!(!replayed.open);
        assert!(!replayed.all_infeasible);
    }

    #[test]
    fn a_node_that_is_neither_branched_nor_terminated_is_refused() {
        let refusal = run(vec![root(), bound(0)], None).expect_err("the root is open");
        assert_eq!(refusal.code, Code::TREE_UNTERMINATED);
    }

    /// The two children of a split cover their parent only because the column is
    /// integer. On a continuous one they leave everything between them.
    #[test]
    fn trap_branch_noninteger_variable() {
        let mut instance = model();
        instance.columns[0].whole = false;
        let mut axioms = Axioms::of(&instance);
        let refusal = replay(
            &mut Work::new(),
            &proof(vec![
                root(),
                Step::Branch {
                    parent: 0,
                    variable: 0,
                    split: whole(2),
                    left: 1,
                    right: 2,
                },
            ]),
            &instance,
            &mut axioms,
            None,
        )
        .expect_err("the variable is continuous");
        assert_eq!(refusal.code, Code::TREE_BRANCH_NOT_INTEGER);
    }

    #[test]
    fn a_split_at_a_fraction_is_refused() {
        let half = Frac::ratio(
            &crate::vint::Int::from_i64(1),
            &crate::vint::Int::from_i64(2),
        )
        .expect("one half");
        let refusal = run(
            vec![
                root(),
                Step::Branch {
                    parent: 0,
                    variable: 0,
                    split: half,
                    left: 1,
                    right: 2,
                },
            ],
            None,
        )
        .expect_err("the split is fractional");
        assert_eq!(refusal.code, Code::TREE_SPLIT_NOT_INTEGER);
    }

    #[test]
    fn a_node_closed_twice_is_refused() {
        let refusal = run(
            vec![
                root(),
                bound(0),
                Step::LeafBound { node: 0, bound: 1 },
                Step::LeafBound { node: 0, bound: 1 },
            ],
            Some(whole(-4)),
        )
        .expect_err("the root closes twice");
        assert_eq!(refusal.code, Code::TREE_DOUBLE_TERMINAL);
    }

    #[test]
    fn a_leaf_citing_an_inference_from_another_node_is_refused() {
        let refusal = run(
            vec![
                root(),
                Step::Branch {
                    parent: 0,
                    variable: 0,
                    split: whole(2),
                    left: 1,
                    right: 2,
                },
                bound(1),
                Step::LeafBound { node: 2, bound: 2 },
                Step::LeafBound { node: 1, bound: 2 },
            ],
            Some(whole(-4)),
        )
        .expect_err("node 2 cites node 1's bound");
        assert_eq!(refusal.code, Code::TREE_LEAF_NODE);
    }

    /// A branch assumption belongs to one child. An inference at the other citing
    /// it is citing a fact about a region it is not in.
    #[test]
    fn trap_scope_sibling_reference() {
        // Node 1's assumption takes identifier 3 and node 2's takes 4. An
        // inference at node 1 citing 4 is citing its sibling's region.
        let refusal = run(
            vec![
                root(),
                Step::Branch {
                    parent: 0,
                    variable: 0,
                    split: whole(2),
                    left: 1,
                    right: 2,
                },
                Step::DualBound {
                    node: 1,
                    terms: vec![(0, Frac::one()), (4, Frac::one())],
                },
            ],
            None,
        )
        .expect_err("the sibling's assumption is not visible");
        assert_eq!(refusal.code, Code::SCOPE_NOT_VISIBLE);
    }

    /// A record may cite only records before it. A forward citation is how a proof
    /// would rest on something derived from it.
    #[test]
    fn trap_inference_future_reference() {
        let refusal = run(
            vec![root(), Step::LeafBound { node: 0, bound: 2 }, bound(0)],
            Some(whole(-4)),
        )
        .expect_err("the citation points forward");
        assert_eq!(refusal.code, Code::SCOPE_FORWARD);
    }

    /// A spine of `count` branches, each splitting the one integer column.
    ///
    /// The children take the next two identifiers, left before right, so the
    /// left child of one branch is the parent of the next and the tree is a
    /// single path. Splitting at the same value every time is redundant and
    /// legal: the replay is checking structure, and a nested region that
    /// repeats its parent's assumption is still a region.
    fn spine(count: u32) -> Vec<Step> {
        let mut steps = Vec::with_capacity(count as usize + 1);
        steps.push(root());
        let mut parent = 0;
        for index in 0..u64::from(count) {
            let left = 2 * index + 1;
            steps.push(Step::Branch {
                parent,
                variable: 0,
                split: whole(2),
                left,
                right: 2 * index + 2,
            });
            parent = left;
        }
        steps
    }

    /// A legal proof may be deeper than the process call stack, and the replay
    /// has to reach the bottom of it rather than crash on the way down.
    ///
    /// Nothing here asserts the tree is *good* — a spine with no terminal
    /// records is an incomplete cover and is refused for that. What is being
    /// established is that the refusal arrives as a rule, from a replay that
    /// walked a hundred thousand levels iteratively. A recursive implementation
    /// would never reach the verdict; it would take the process with it, and a
    /// verifier that dies on a deep proof has rejected it for a reason with
    /// nothing to do with mathematics.
    #[test]
    fn a_proof_as_deep_as_the_cap_is_answered_by_a_rule_and_not_a_crash() {
        let refusal = run(spine(DEPTH_CAP), None)
            .expect_err("a spine with no terminal records is not a cover");
        assert_ne!(
            refusal.code,
            Code::TREE_DEPTH,
            "a tree exactly at the cap is within it"
        );
    }

    /// And one level past the cap is refused by the cap, not by exhaustion.
    #[test]
    fn a_proof_past_the_depth_cap_is_refused_by_the_cap() {
        let refusal =
            run(spine(DEPTH_CAP + 1), None).expect_err("the tree is deeper than the cap allows");
        assert_eq!(refusal.code, Code::TREE_DEPTH);
    }

    #[test]
    fn a_child_identifier_that_is_not_the_next_one_is_refused() {
        let refusal = run(
            vec![
                root(),
                Step::Branch {
                    parent: 0,
                    variable: 0,
                    split: whole(2),
                    left: 5,
                    right: 6,
                },
            ],
            None,
        )
        .expect_err("the children leave a gap");
        assert_eq!(refusal.code, Code::TREE_MULTIPLE_PARENTS);
    }

    #[test]
    fn a_frontier_whose_bound_already_meets_the_incumbent_must_close_its_node() {
        let refusal = run(
            vec![root(), bound(0), Step::LeafFrontier { node: 0, bound: 1 }],
            Some(whole(-4)),
        )
        .expect_err("the frontier is not open");
        assert_eq!(refusal.code, Code::TREE_FRONTIER_CLOSED);
    }

    #[test]
    fn a_closed_leaf_below_the_incumbent_is_refused() {
        // The bound from the variable's upper side is -10, which is below an
        // incumbent of -4, so it does not close anything.
        let refusal = run(
            vec![
                root(),
                Step::DualBound {
                    node: 0,
                    terms: vec![(2, Frac::one())],
                },
                Step::LeafBound { node: 0, bound: 1 },
            ],
            Some(whole(-4)),
        )
        .expect_err("the bound does not reach the incumbent");
        assert_eq!(refusal.code, Code::CLAIM_LEAF_BELOW);
    }

    #[test]
    /// A Leaf.Integral cites a DualBound at its own node, and the point it
    /// carries must attain exactly that bound.
    ///
    /// The point alone proves nothing about the node — a feasible point is an
    /// upper bound and says nothing about what else the region holds. The
    /// format cannot express a leaf without the citation, and this is the check
    /// that the citation is doing work rather than sitting there.
    fn trap_integral_leaf_without_bound() {
        let refusal = run(
            vec![
                root(),
                bound(0),
                Step::LeafIntegral {
                    node: 0,
                    point: vec![whole(3)],
                    bound: 1,
                },
            ],
            Some(whole(-4)),
        )
        .expect_err("the point attains -3 and the bound is -4");
        assert_eq!(refusal.code, Code::TREE_INTEGRAL_OBJECTIVE);
    }

    #[test]
    fn an_integral_leaf_whose_point_is_infeasible_is_refused() {
        let refusal = run(
            vec![
                root(),
                bound(0),
                Step::LeafIntegral {
                    node: 0,
                    point: vec![whole(99)],
                    bound: 1,
                },
            ],
            Some(whole(-4)),
        )
        .expect_err("the point is outside the model");
        assert_eq!(refusal.code, Code::TREE_INTEGRAL_POINT);
    }
}
