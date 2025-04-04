//! The search itself.
//!
//! A deliberately small branch and bound: best-first by exact node bound, one
//! integer split per node, cold child solves, no cuts, no presolve, no
//! heuristics, no pseudocosts. That restraint is the point of this stage. The
//! recursive proof structure is the thing being established, and establishing
//! it with the rest of the machinery already in place would mean never knowing
//! which part was responsible for what.
//!
//! What happens at a node, in order. Solve the node's linear relaxation from
//! cold. If it is infeasible, the node is closed and the reason is a
//! contradiction that exists in the basis. If it is optimal, rebuild the exact
//! vertex and the exact multipliers from the basis, and take the exact bound
//! those multipliers establish — not the objective the simplex reported, which
//! is a floating-point number about a scaled copy. If the exact bound is no
//! better than the incumbent, the node is closed. If the exact vertex is
//! integral, it is screened and may become the incumbent, and the node is
//! closed by a point that attains its own bound. Otherwise split.
//!
//! Every one of those closures is a claim, and each is recorded with the basis
//! it came from so the certificate can be written from what happened rather
//! than reconstructed afterwards.
//!
//! A node whose exact rebuild fails is not closed. It is recorded as unproved
//! and left open, and a tree with an open node proves a gap rather than
//! optimality. That is the honest outcome and there is no other one available:
//! the alternative would be to close a node on a floating-point number, which
//! is the single thing this project exists to refuse.

use hc_core::{Model, Tol};
use hc_exact::Rational;
use hc_lp::exact::{Canonical, exact_multipliers, exact_point, resting_value};
use hc_lp::{Basis, Options as LpOptions, Outcome as LpOutcome, Problem, Working, solve};

use crate::incumbent::{Incumbent, most_fractional, screen};
use crate::tree::Tree;

/// How much searching to do.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The largest number of nodes to open.
    pub node_limit: u64,
    /// The deepest a node may be.
    pub depth_limit: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            node_limit: 100_000,
            depth_limit: 64,
        }
    }
}

/// What happened at one node.
#[derive(Clone, Debug)]
pub enum Node {
    /// The node's region is empty, and the basis witnesses it.
    Infeasible {
        /// The final basis of the node's relaxation.
        basis: Basis,
    },
    /// The node is closed by a bound no better than the incumbent.
    Bound {
        /// The final basis of the node's relaxation.
        basis: Basis,
        /// The exact lower bound the basis establishes.
        value: Rational,
    },
    /// The node is closed by an integral point attaining its own bound.
    Integral {
        /// The final basis of the node's relaxation.
        basis: Basis,
        /// The exact point, over the working shape's columns.
        point: Vec<Rational>,
        /// The exact bound, which the point's objective equals.
        value: Rational,
    },
    /// The node is still open and carries an exact bound.
    Frontier {
        /// The final basis of the node's relaxation.
        basis: Basis,
        /// The exact lower bound the basis establishes.
        value: Rational,
    },
    /// The node was split.
    Split {
        /// The column split on.
        column: usize,
        /// The integer the split was taken at.
        at: Rational,
        /// The child taking the at-most side.
        left: u64,
        /// The child taking the at-least side.
        right: u64,
    },
    /// The node could not be closed and could not be given a bound.
    ///
    /// Recorded rather than swallowed. A tree holding one of these proves
    /// nothing about the region it covers, and the search says so.
    Unproved {
        /// Why the exact rebuild declined.
        reason: String,
    },
}

/// How a search ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// Every node is closed and an incumbent exists.
    Optimal,
    /// Every node is closed and no incumbent exists.
    Infeasible,
    /// A relaxation was unbounded, which a mixed-integer claim cannot use.
    UnboundedRelaxation,
    /// Nodes remain open, and each open one carries an exact bound.
    Frontier,
    /// Nodes remain open and at least one of them has no bound.
    NoProgress,
}

/// What a search found.
#[derive(Clone, Debug)]
pub struct Search {
    /// The tree, in creation order.
    pub tree: Tree,
    /// What happened at each node, indexed by node identifier.
    pub nodes: Vec<Option<Node>>,
    /// The best point that survived exact screening.
    pub incumbent: Option<Incumbent>,
    /// How the search ended.
    pub ending: Ending,
    /// Nodes opened.
    pub opened: u64,
    /// Simplex iterations spent across every node.
    pub iterations: u64,
}

impl Search {
    /// The node identifiers still open, ascending.
    #[must_use]
    pub fn frontier(&self) -> Vec<u64> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                matches!(
                    node,
                    Some(Node::Frontier { .. }) | Some(Node::Unproved { .. })
                )
            })
            .map(|(id, _)| id as u64)
            .collect()
    }

    /// The exact global lower bound, when every open node has one.
    ///
    /// Absent when any open node is unproved, because a minimum over a set
    /// with an unknown member is not a bound on anything.
    #[must_use]
    pub fn lower_bound(&self) -> Option<Rational> {
        let mut best: Option<Rational> = None;
        for node in self.nodes.iter().flatten() {
            match node {
                Node::Unproved { .. } => return None,
                Node::Frontier { value, .. } => {
                    best = Some(match best {
                        None => value.clone(),
                        Some(seen) if value.cmp_value(&seen).is_lt() => value.clone(),
                        Some(seen) => seen,
                    });
                }
                _ => {}
            }
        }
        match (&best, &self.incumbent) {
            (None, Some(incumbent)) => Some(incumbent.objective.clone()),
            _ => best,
        }
    }
}

/// Runs the search.
#[must_use]
pub fn run(model: &Model, options: &Options) -> Search {
    let mut walk = Walk {
        working: Working::from_model(model),
        root_exact: Canonical::of(model),
        tree: Tree::new(),
        nodes: vec![None],
        incumbent: None,
        open: vec![0],
        opened: 0,
        iterations: 0,
        unbounded: false,
    };

    while let Some(id) = walk.open.iter().min().copied() {
        walk.open.retain(|other| *other != id);
        if walk.opened >= options.node_limit {
            // Whatever is left open stays open and keeps whatever bound it was
            // given. Nothing is closed on the way out.
            walk.open.push(id);
            break;
        }
        walk.opened += 1;
        let result = walk.visit(id, options);
        walk.record(id, result);
    }

    walk.finish()
}

/// The state one search walks over.
///
/// A struct rather than a fistful of parameters. Every field is something the
/// walk both reads and writes, and threading eight of them through a free
/// function was an invitation to pass two of them in the wrong order.
struct Walk {
    working: Working,
    root_exact: Canonical,
    tree: Tree,
    nodes: Vec<Option<Node>>,
    incumbent: Option<Incumbent>,
    open: Vec<u64>,
    opened: u64,
    iterations: u64,
    unbounded: bool,
}

impl Walk {
    /// Solves one node's relaxation and decides what it means.
    fn visit(&mut self, id: u64, options: &Options) -> Node {
        let depth = self.tree.get(id).expect("an open node exists").depth;
        let narrowed = narrow(&self.working, &self.tree, id);
        let problem = Problem::scaled(&narrowed, Tol::solver());
        let (outcome, effort) = solve(&problem, &LpOptions::default());
        self.iterations += effort.iterations;

        // The exact view of the same subproblem, narrowed from the same path,
        // so the search and the rebuild are looking at one region.
        let canonical = narrow_exact(&self.root_exact, &self.tree, id);

        match &outcome {
            LpOutcome::Infeasible { basis, .. } => Node::Infeasible {
                basis: basis.clone(),
            },
            LpOutcome::Unbounded { .. } => {
                self.unbounded = true;
                unproved(&"the relaxation is unbounded")
            }
            LpOutcome::Optimal { basis, .. } => self.close(&canonical, basis, id, depth, options),
            _ => unproved(&"the relaxation reached no conclusion"),
        }
    }

    /// Decides what an optimal node relaxation means, splitting it if it must.
    fn close(
        &mut self,
        canonical: &Canonical,
        basis: &Basis,
        id: u64,
        depth: u32,
        options: &Options,
    ) -> Node {
        let point = match exact_point(canonical, basis, &|column| {
            resting_value(canonical, basis, column)
        }) {
            Ok(point) => point,
            Err(reason) => return unproved(&reason),
        };

        // The multipliers are rebuilt here even though the bound below comes
        // from the vertex, because a node that cannot produce them cannot be
        // written into a proof. Finding that out now rather than at assembly
        // time is what keeps the tree honest about what it has established.
        let costs: Vec<Rational> = (0..canonical.columns())
            .map(|index| canonical.cost(index).clone())
            .collect();
        if let Err(reason) = exact_multipliers(canonical, basis, &costs) {
            return unproved(&reason);
        }

        let value = canonical.objective_at(&point);

        if let Some(best) = self.incumbent.as_ref() {
            if value.cmp_value(&best.objective).is_ge() {
                return Node::Bound {
                    basis: basis.clone(),
                    value,
                };
            }
        }

        match most_fractional(canonical, &point) {
            None => match screen(canonical, &point) {
                Ok(found) => {
                    let better = self
                        .incumbent
                        .as_ref()
                        .is_none_or(|best| found.objective.cmp_value(&best.objective).is_lt());
                    if better {
                        self.incumbent = Some(found);
                    }
                    Node::Integral {
                        basis: basis.clone(),
                        point,
                        value,
                    }
                }
                Err(reason) => unproved(&reason),
            },
            Some((column, at)) => {
                if depth >= options.depth_limit {
                    return Node::Frontier {
                        basis: basis.clone(),
                        value,
                    };
                }
                let (left, right) = self.tree.split(id, column, &at);
                self.open.push(left);
                self.open.push(right);
                Node::Split {
                    column,
                    at,
                    left,
                    right,
                }
            }
        }
    }

    /// Files what happened at a node, growing the result vector to match the
    /// tree the visit may have extended.
    fn record(&mut self, id: u64, result: Node) {
        let slot = usize::try_from(id).expect("a node identifier fits");
        while self.nodes.len() <= slot.max(self.tree.len().saturating_sub(1)) {
            self.nodes.push(None);
        }
        self.nodes[slot] = Some(result);
    }

    /// Turns the walk into what it found.
    fn finish(mut self) -> Search {
        while self.nodes.len() < self.tree.len() {
            self.nodes.push(None);
        }
        // Anything still queued is a node the search never opened. It is a
        // frontier with no bound, which is exactly what it is.
        for id in &self.open {
            let slot = usize::try_from(*id).expect("a node identifier fits");
            if self.nodes[slot].is_none() {
                self.nodes[slot] = Some(unproved(&"the node was never opened"));
            }
        }

        let unproved_any = self
            .nodes
            .iter()
            .flatten()
            .any(|node| matches!(node, Node::Unproved { .. }));
        let has_frontier = self
            .nodes
            .iter()
            .flatten()
            .any(|node| matches!(node, Node::Frontier { .. }));

        let ending = if self.unbounded {
            Ending::UnboundedRelaxation
        } else if unproved_any {
            Ending::NoProgress
        } else if has_frontier {
            Ending::Frontier
        } else if self.incumbent.is_some() {
            Ending::Optimal
        } else {
            Ending::Infeasible
        };

        Search {
            tree: self.tree,
            nodes: self.nodes,
            incumbent: self.incumbent,
            ending,
            opened: self.opened,
            iterations: self.iterations,
        }
    }
}

fn unproved(reason: &dyn core::fmt::Display) -> Node {
    Node::Unproved {
        reason: reason.to_string(),
    }
}

/// The working copy a node solves: the root's, narrowed by the path.
fn narrow(working: &Working, tree: &Tree, node: u64) -> Working {
    let mut bounds: Vec<(usize, f64, f64)> = Vec::new();
    for assumption in tree.path(node) {
        let value = assumption.value.to_binary64();
        if assumption.at_most {
            bounds.push((assumption.column, f64::NEG_INFINITY, value));
        } else {
            bounds.push((assumption.column, value, f64::INFINITY));
        }
    }
    working.narrowed(&bounds)
}

/// The exact view of the same subproblem, narrowed from the same path.
fn narrow_exact(root: &Canonical, tree: &Tree, node: u64) -> Canonical {
    let mut bounds: Vec<(usize, Option<Rational>, Option<Rational>)> = Vec::new();
    for assumption in tree.path(node) {
        if assumption.at_most {
            bounds.push((assumption.column, None, Some(assumption.value.clone())));
        } else {
            bounds.push((assumption.column, Some(assumption.value.clone()), None));
        }
    }
    root.narrowed(&bounds)
}
