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
use hc_cuts::pool::{Entry, Pool};
use hc_cuts::separate;
use hc_exact::Rational;
use hc_lp::dual::solve_from as solve_dual;
use hc_lp::exact::ExactRow;
use hc_lp::exact::{Canonical, exact_multipliers, exact_point, resting_value};
use hc_lp::working::ExtraRow;
use hc_lp::{Basis, Options as LpOptions, Outcome as LpOutcome, Problem, Working, solve};

use crate::branching::{
    Branching, Candidate, Pseudocosts, by_pseudocost, candidates, most_fractional,
};
use crate::incumbent::{Incumbent, screen};
use crate::tree::Tree;

/// How much searching to do.
#[derive(Clone, Debug)]
pub struct Options {
    /// The largest number of nodes to open.
    pub node_limit: u64,
    /// The deepest a node may be.
    pub depth_limit: u32,
    /// How deep a dive may go before the search returns to best-first.
    ///
    /// Plunging finds incumbents early, and an incumbent is what makes every
    /// later bound able to close a node. Diving forever would be depth-first
    /// search wearing a different name, so the dive is capped and the search
    /// returns to the open node with the most promising bound.
    pub plunge_depth: u32,
    /// How many node bases to keep for warm starts.
    ///
    /// A basis is large and a tree is not. Keeping every one would make memory
    /// grow with the search rather than with the model, so the oldest are
    /// dropped and the nodes that lose theirs are solved from cold — which is
    /// slower and reaches the same answers, as the warm-start trap requires.
    pub basis_cache: usize,
    /// How many rounds of cut separation to run at a node.
    ///
    /// Zero turns cuts off entirely, which is how the search runs while the
    /// certificate machinery for them is being built and how the campaign runs
    /// the comparison that shows cuts change nothing about the answer.
    pub cut_rounds: usize,
    /// How deep in the tree cuts are still separated.
    ///
    /// A cut derived at a node is true of that node's region and of nothing
    /// outside it, so a deep cut helps a small part of the tree and costs a
    /// derivation everywhere it is carried. The cap is where that trade stops
    /// paying.
    pub cut_depth: u32,
    /// A candidate solution to start from.
    ///
    /// It goes through the same exact screening every other candidate does.
    /// A supplied point that is not genuinely feasible is refused rather than
    /// rounded into place, because the incumbent is what every prune is
    /// measured against and a wrong one discards the answer.
    pub start: Option<Vec<Rational>>,
    /// Which rule chooses the branching variable.
    ///
    /// It changes the shape of the tree and nothing about what the tree
    /// proves, which is why both rules exist and why a campaign runs the
    /// search under each and requires the same answers.
    pub branching: Branching,
    /// Whether a child starts from its parent's basis.
    ///
    /// A branch changes one bound and nothing else, so the parent's optimal
    /// basis still prices every column correctly and the child is a dual
    /// problem with a little primal infeasibility. Turning this off solves
    /// every node from cold, which is slower and must reach the same answers —
    /// that equivalence is what the warm-start trap checks.
    pub warm_start: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            node_limit: 100_000,
            depth_limit: 64,
            plunge_depth: 32,
            basis_cache: 256,
            cut_rounds: 0,
            cut_depth: 4,
            start: None,
            branching: Branching::ReliabilityPseudocost,
            warm_start: true,
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
    /// A node whose bound came out below its parent's.
    ///
    /// A child's region is a subset of its parent's, so its bound cannot be
    /// weaker. When it is, the arithmetic is describing something other than
    /// the problem, and the node is kept open rather than closed on numbers
    /// that have stopped meaning anything.
    Suspect {
        /// The bound the node produced.
        value: Rational,
        /// The bound its parent had.
        parent: Rational,
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
    /// The cuts the search derived, with the subtree each is true of.
    pub pool: Pool,
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
                Node::Unproved { .. } | Node::Suspect { .. } => return None,
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
    let root_exact = Canonical::of(model);
    // A supplied solution is screened before the search begins, so the first
    // node already has something to prune against — and so that a point which
    // is not genuinely feasible is refused here rather than believed.
    let incumbent = options
        .start
        .as_ref()
        .and_then(|point| screen(&root_exact, point).ok());

    let mut walk = Walk {
        working: Working::from_model(model),
        root_exact,
        tree: Tree::new(),
        bases: vec![None],
        cached: Vec::new(),
        bounds: vec![None],
        costs: Pseudocosts::new(root_exact_columns(model)),
        pool: Pool::new(),
        nodes: vec![None],
        incumbent,
        open: vec![0],
        diving: None,
        bounding: false,
        opened: 0,
        iterations: 0,
        unbounded: false,
    };

    while let Some(id) = walk.pick() {
        walk.open.retain(|other| *other != id);
        if walk.opened >= options.node_limit {
            // The budget is spent. What remains is bounded rather than split,
            // so every node the search leaves open leaves with an exact bound
            // and the result is a certified gap rather than no progress.
            walk.bounding = true;
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
    /// The basis each node finished on, for its children to start from.
    bases: Vec<Option<Basis>>,
    /// The order bases were filed in, oldest first, for eviction.
    cached: Vec<u64>,
    /// The exact bound each node established, for its children to beat.
    bounds: Vec<Option<Rational>>,
    /// What splitting each column has cost, so far.
    costs: Pseudocosts,
    /// The cuts derived so far, each carrying the subtree it is true of.
    pool: Pool,
    nodes: Vec<Option<Node>>,
    incumbent: Option<Incumbent>,
    open: Vec<u64>,
    /// The node a dive is currently following, when one is.
    diving: Option<u64>,
    /// Whether the search has stopped splitting and is only bounding.
    ///
    /// When the node budget runs out, the nodes still queued are each given a
    /// bound rather than abandoned. An open node with an exact bound is a
    /// frontier and a certified gap can be built from it; an open node with no
    /// bound is a hole, and a tree with a hole proves nothing.
    bounding: bool,
    opened: u64,
    iterations: u64,
    unbounded: bool,
}

impl Walk {
    /// The next node to open.
    ///
    /// A dive in progress takes its own child, which is what plunging means:
    /// follow one branch down until it closes, because a leaf reached quickly
    /// is an incumbent, and an incumbent is what lets every other bound close
    /// something. When the dive ends, the search returns to the open node
    /// whose parent bound is most promising, with the lowest identifier
    /// breaking ties.
    fn pick(&mut self) -> Option<u64> {
        if let Some(diving) = self.diving {
            if self.open.contains(&diving) {
                return Some(diving);
            }
        }
        self.diving = None;

        let mut best: Option<(u64, Option<Rational>)> = None;
        for id in &self.open {
            let bound = self
                .bounds
                .get(usize::try_from(*id).ok()?)
                .cloned()
                .flatten();
            let better = match (&best, &bound) {
                (None, _) => true,
                // A node with no bound yet is as promising as anything can be:
                // nothing has established it cannot hold the answer.
                (Some((_, None)), _) => false,
                (Some((_, Some(_))), None) => true,
                (Some((_, Some(seen))), Some(value)) => value.cmp_value(seen).is_lt(),
            };
            if better {
                best = Some((*id, bound));
            }
        }
        best.map(|(id, _)| id)
    }

    /// Chooses the branching variable, measuring where the estimates are thin.
    ///
    /// Strong branching is trial solves: fix the variable's two sides in turn
    /// and see how far each pushes the relaxation. Those are ordinary
    /// relaxations solved at a small iteration cap, and nothing they produce
    /// reaches a conclusion — they order candidates and stop.
    fn branch_on(
        &mut self,
        canonical: &Canonical,
        list: &[Candidate],
        options: &Options,
    ) -> Option<Candidate> {
        if list.is_empty() {
            return None;
        }
        match options.branching {
            Branching::MostFractional => most_fractional(list).cloned(),
            Branching::ReliabilityPseudocost => {
                let working = self.working.clone();
                let parent = canonical;
                let mut costs = core::mem::take(&mut self.costs);
                let picked = by_pseudocost(list, &mut costs, &mut |candidate| {
                    trial(&working, parent, candidate)
                });
                self.costs = costs;
                picked
            }
        }
    }

    /// A node and every ancestor of it, the node first.
    fn lineage(&self, node: u64) -> Vec<u64> {
        let mut out = Vec::new();
        let mut at = Some(node);
        while let Some(id) = at {
            out.push(id);
            at = self.tree.get(id).and_then(|entry| entry.parent);
        }
        out
    }

    /// The floating relaxation a node solves: the model, narrowed by the path
    /// and carrying the cuts visible there.
    fn relaxation(&self, node: u64, lineage: &[u64]) -> Working {
        let narrowed = narrow(&self.working, &self.tree, node);
        let extra: Vec<ExtraRow> = self
            .pool
            .visible(lineage)
            .into_iter()
            .map(|entry| ExtraRow {
                terms: entry
                    .coefficients
                    .iter()
                    .map(|(column, value)| (*column, value.to_binary64()))
                    .collect(),
                low: entry.right.to_binary64(),
                high: f64::INFINITY,
            })
            .collect();
        narrowed.with_rows(&extra)
    }

    /// The exact view of the same relaxation.
    fn exact_at(&self, node: u64, lineage: &[u64]) -> Canonical {
        let narrowed = narrow_exact(&self.root_exact, &self.tree, node);
        let extra: Vec<ExactRow> = self
            .pool
            .visible(lineage)
            .into_iter()
            .map(|entry| ExactRow {
                terms: entry.coefficients.clone(),
                low: Some(entry.right.clone()),
                high: None,
            })
            .collect();
        narrowed.with_rows(&extra)
    }

    /// Runs cut rounds at a node.
    fn separate(&mut self, node: u64, lineage: &[u64], options: &Options) {
        for _ in 0..options.cut_rounds {
            let narrowed = self.relaxation(node, lineage);
            let problem = Problem::scaled(&narrowed, Tol::solver());
            let (outcome, effort) = solve(&problem, &LpOptions::default());
            self.iterations += effort.iterations;
            let LpOutcome::Optimal { basis, .. } = &outcome else {
                return;
            };
            let canonical = self.exact_at(node, lineage);
            let Ok(point) = exact_point(&canonical, basis, &|column| {
                resting_value(&canonical, basis, column)
            }) else {
                return;
            };

            let mut found: Vec<Entry> = separate::gomory(&canonical, basis, &point, node);
            found.extend(separate::cover(&canonical, &point, node));
            let mut added = false;
            for entry in found {
                added |= self.pool.add(entry, lineage);
            }
            if !added {
                return;
            }
        }
    }

    /// Files a basis, dropping the oldest when the cache is full.
    fn cache(&mut self, id: u64, basis: Option<Basis>, limit: usize) {
        self.remember(id, basis);
        self.cached.retain(|other| *other != id);
        self.cached.push(id);
        while self.cached.len() > limit.max(1) {
            let oldest = self.cached.remove(0);
            let slot = usize::try_from(oldest).expect("a node identifier fits");
            if let Some(entry) = self.bases.get_mut(slot) {
                *entry = None;
            }
        }
    }

    /// Solves one node's relaxation and decides what it means.
    fn visit(&mut self, id: u64, options: &Options) -> Node {
        let entry = self.tree.get(id).expect("an open node exists");
        let depth = entry.depth;
        let parent = entry.parent;
        let lineage = self.lineage(id);

        // Cut rounds first, if this node still separates. Each round solves
        // the relaxation as it stands, looks for cuts that exclude the point
        // it found, and adds the ones whose derivation survives.
        if options.cut_rounds > 0 && depth <= options.cut_depth {
            self.separate(id, &lineage, options);
        }

        let narrowed = self.relaxation(id, &lineage);
        let problem = Problem::scaled(&narrowed, Tol::solver());

        // The warm start, when there is one to use. A refusal is not a
        // failure: it means the parent's basis does not satisfy the dual
        // method's precondition against this child, and the honest response is
        // to solve from cold rather than to repair a start into something the
        // caller did not ask for.
        //
        // Shape is part of that precondition. A cut round at this node adds
        // rows the parent's relaxation never had, so the remembered basis
        // carries one column per row of a smaller problem. It describes a
        // different subproblem rather than a start for this one, and padding
        // it out to fit would be inventing basic columns nothing solved for.
        let warm = if options.warm_start {
            parent
                .and_then(|at| self.bases.get(usize::try_from(at).ok()?).cloned().flatten())
                .filter(|start| {
                    start.basic().len() == problem.rows() && start.columns() == problem.columns()
                })
                .and_then(|start| solve_dual(&problem, start, &LpOptions::default()).ok())
        } else {
            None
        };
        let (outcome, effort) = match warm {
            Some(result) => result,
            None => solve(&problem, &LpOptions::default()),
        };
        self.iterations += effort.iterations;

        // Whatever basis the node finished on is what its children start from.
        let finished = match &outcome {
            LpOutcome::Optimal { basis, .. }
            | LpOutcome::Infeasible { basis, .. }
            | LpOutcome::Unbounded { basis, .. }
            | LpOutcome::EffortLimit { basis, .. }
            | LpOutcome::Numerical { basis, .. } => Some(basis.clone()),
        };
        self.cache(id, finished, options.basis_cache);

        // The exact view of the same subproblem, narrowed from the same path
        // and carrying the same cuts, so the search and the rebuild are
        // looking at one region.
        let canonical = self.exact_at(id, &lineage);

        match &outcome {
            LpOutcome::Infeasible { basis, .. } => Node::Infeasible {
                basis: basis.clone(),
            },
            LpOutcome::Unbounded { .. } => {
                self.unbounded = true;
                unproved(&"the relaxation is unbounded")
            }
            LpOutcome::Optimal { basis, .. } => {
                let result = self.close(&canonical, basis, id, depth, options);
                // A child's region is inside its parent's, so its bound cannot
                // be weaker. A bound that regressed is reported and the node
                // stays open; it is never used to close anything.
                if let Some(value) = bound_of(&result) {
                    if let Some(parent) = parent {
                        if let Some(Some(before)) = self
                            .bounds
                            .get(usize::try_from(parent).unwrap_or(usize::MAX))
                        {
                            if value.cmp_value(before).is_lt() {
                                return Node::Suspect {
                                    value: value.clone(),
                                    parent: before.clone(),
                                };
                            }
                        }
                    }
                }
                let slot = usize::try_from(id).expect("a node identifier fits");
                while self.bounds.len() <= slot.max(self.tree.len().saturating_sub(1)) {
                    self.bounds.push(None);
                }
                self.bounds[slot] = bound_of(&result).cloned();
                // A split starts or continues a dive down its left child.
                if let Node::Split { left, .. } = &result {
                    self.diving = if depth + 1 < options.plunge_depth {
                        Some(*left)
                    } else {
                        None
                    };
                }
                result
            }
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

        let list = candidates(canonical, &point);
        match self.branch_on(canonical, &list, options) {
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
            Some(candidate) => {
                if self.bounding || depth >= options.depth_limit {
                    return Node::Frontier {
                        basis: basis.clone(),
                        value,
                    };
                }
                let (column, at) = (candidate.column, candidate.at);
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

    /// Files the basis a node finished on.
    fn remember(&mut self, id: u64, basis: Option<Basis>) {
        let slot = usize::try_from(id).expect("a node identifier fits");
        while self.bases.len() <= slot {
            self.bases.push(None);
        }
        self.bases[slot] = basis;
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
            .any(|node| matches!(node, Node::Unproved { .. } | Node::Suspect { .. }));
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
            pool: self.pool,
            ending,
            opened: self.opened,
            iterations: self.iterations,
        }
    }
}

/// The exact bound a node result carries, when it carries one.
fn bound_of(node: &Node) -> Option<&Rational> {
    match node {
        Node::Bound { value, .. } | Node::Frontier { value, .. } | Node::Integral { value, .. } => {
            Some(value)
        }
        _ => None,
    }
}

/// How much each side of a candidate would degrade the relaxation.
///
/// Two capped solves. A side that fails to solve within the cap contributes
/// nothing rather than a guess, because an estimate invented where a
/// measurement failed is worse than no estimate.
fn trial(working: &Working, parent: &Canonical, candidate: &Candidate) -> Option<(f64, f64)> {
    let base = Problem::scaled(working, Tol::solver());
    let (before, _) = solve(
        &base,
        &LpOptions {
            iteration_limit: crate::branching::STRONG_ITERATIONS,
            ..LpOptions::default()
        },
    );
    let LpOutcome::Optimal { objective, .. } = before else {
        return None;
    };

    let floor = crate::branching::to_f64(&candidate.at);
    let side = |low: f64, high: f64| -> Option<f64> {
        let narrowed = working.narrowed(&[(candidate.column, low, high)]);
        let problem = Problem::scaled(&narrowed, Tol::solver());
        let (outcome, _) = solve(
            &problem,
            &LpOptions {
                iteration_limit: crate::branching::STRONG_ITERATIONS,
                ..LpOptions::default()
            },
        );
        match outcome {
            LpOutcome::Optimal {
                objective: after, ..
            } => Some((after - objective).max(0.0)),
            // An infeasible side is the most informative outcome there is, and
            // it has no finite degradation. It is reported as a large one
            // rather than as nothing, because the variable is worth splitting.
            LpOutcome::Infeasible { .. } => Some(objective.abs() + 1.0),
            _ => None,
        }
    };

    // The node's exact view is not consulted: a trial solve is a floating
    // estimate and is allowed to be one, because nothing it produces reaches a
    // conclusion.
    let _ = parent;
    let down = side(f64::NEG_INFINITY, floor)?;
    let up = side(floor + 1.0, f64::INFINITY)?;
    Some((down, up))
}

/// How many columns the working shape has, for sizing the estimates.
fn root_exact_columns(model: &Model) -> usize {
    model.variables().len() + model.rows().len()
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
