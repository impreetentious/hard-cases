//! Writing a search tree down as a proof.
//!
//! The search already built its tree parent before child, so serializing it is
//! a walk rather than a sort. That is deliberate and it is load-bearing: a
//! certificate whose record order had to be repaired afterwards would be
//! evidence that the search and the proof disagree about what happened.
//!
//! Two things have to be got right here and both are about identifiers.
//!
//! A branch assumption is a canonical inequality — `x <= k` on one side and
//! `x >= k + 1` on the other — and it needs an identifier a later record can
//! cite. The specification puts those above the model's base block, assigned in
//! certificate record order, so a `Branch` record allocates two as it is
//! written and the verifier allocates the same two as it reads. Neither side
//! stores the assignment; both compute it, which is why they cannot drift.
//!
//! And an inference made at a node must cite the inequality the arithmetic
//! actually used. On a branched variable that is the assumption, not whatever
//! the file said the bound was — the file's bound is still true and still
//! visible, and citing it would be citing a weaker inequality than the one the
//! multipliers were computed against, which does not sum.

use hc_core::{Model, RowId, Side, VarId};
use hc_exact::{BigInt, BigUint, Rational};
use hc_lp::exact::{Canonical, exact_multipliers, resting_value};
use hc_mip::search::{Ending, Node, Search};
use hc_mip::tree::{Assumption, Tree};

use hc_presolve::chain::Chain;
use hc_presolve::encode;

use crate::assemble::{NoProof, model_hash, ray_direction, relative_gap};
use crate::hcc::{Certificate, ClaimKind, Record};
use crate::lp::{Axioms, Bounds, ModelBounds, Supply, assemble_with};

/// Assembles a certificate for a mixed-integer search.
///
/// # Errors
///
/// Returns [`NoProof`] naming what stopped the assembly.
pub fn assemble_mip(
    model: &Model,
    instance: &[u8],
    search: &Search,
) -> Result<Certificate, NoProof> {
    build(model, model, None, instance, search)
}

/// Assembles a certificate for a search over a reduced model.
///
/// The chain is written first, each record carrying the hash of the stage it
/// produces, and the root then names the model the chain ends at. The
/// incumbent carries both coordinate systems: the reduced point the tree is
/// about, and the original-space point it lifts to, which the verifier checks
/// against the original bytes independently.
///
/// # Errors
///
/// Returns [`NoProof`] naming what stopped the assembly.
pub fn assemble_presolved(
    original: &Model,
    chain: &Chain,
    instance: &[u8],
    search: &Search,
) -> Result<Certificate, NoProof> {
    // A chain that ended in a contradiction proves infeasibility by itself.
    // No root and no tree follow it, because they would imply support the
    // certificate does not have.
    if let Some(found) = &chain.contradiction {
        let (rule, parameters) = encode(&found.rule);
        let sources = parameters
            .iter()
            .take(match rule {
                7 => 3,
                _ => 2,
            })
            .map(|value| {
                value
                    .to_canonical_string()
                    .parse::<u64>()
                    .expect("an index parameter")
            })
            .collect();
        let mut records = prefix(chain);
        records.push(Record::TransformContradiction { rule, sources });
        records.push(Record::Claim {
            kind: ClaimKind::Infeasible,
            objective: None,
            lower: None,
            gap: None,
        });
        return Ok(Certificate {
            format_tag: original.format().byte(),
            instance_bytes: instance.len() as u64,
            digest: *original.digest(),
            claim: ClaimKind::Infeasible,
            records: Certificate::terminated(records),
        });
    }

    build(&chain.reduced, original, Some(chain), instance, search)
}

/// The transform records a chain contributes.
fn prefix(chain: &Chain) -> Vec<Record> {
    chain
        .steps
        .iter()
        .map(|step| {
            let (rule, parameters) = encode(&step.rule);
            Record::Transform {
                rule,
                parameters,
                result: step.result,
            }
        })
        .collect()
}

/// The body of both entry points.
fn build(
    model: &Model,
    original: &Model,
    chain: Option<&Chain>,
    instance: &[u8],
    search: &Search,
) -> Result<Certificate, NoProof> {
    // An unbounded relaxation never produces a tree. Its claim is checked
    // straight against the original bytes, so a root and a cover would be
    // support the certificate does not have and does not want.
    if matches!(search.ending, Ending::UnboundedRelaxation) {
        return unbounded_certificate(model, original, chain, instance, search);
    }

    // A search that left a node with no bound at all cannot present a tree:
    // its cover has a hole, and a gap claim needs every open frontier to prove
    // something. What it may still have is a point, and a point is worth
    // saying out loud on its own terms.
    if matches!(search.ending, Ending::NoProgress) {
        return point_only_certificate(model, original, chain, instance, search);
    }

    let axioms = Axioms::of(model);
    let base = model.inequalities().len() as u64;
    let root = Canonical::of(model);

    let mut records = chain.map(prefix).unwrap_or_default();
    records.push(Record::Root {
        node: 0,
        model: model_hash(model),
    });

    // The identifier a node's own assumption received, filled in as the branch
    // that created it is written.
    let mut assumption_ids: Vec<Option<u64>> = vec![None; search.tree.len()];
    let mut next_id = base;

    // Two passes, and the reason is the identifier rule. A branch allocates
    // the two identifiers its children's assumptions take, and the verifier
    // allocates them in record order — so the branches have to appear in the
    // order the children were *created*, which is not the order the nodes were
    // *visited* once the search stopped taking them lowest-first.
    //
    // Pass one writes every branch, ordered by the child it created. Pass two
    // writes each closed node's inference and its leaf. Both respect parent
    // before child and both keep every citation backwards, which is what the
    // format requires; the grouping is one legal interleaving of many.
    let mut splits: Vec<(u64, u64)> = Vec::new();
    for entry in search.tree.entries() {
        let slot = usize::try_from(entry.id).expect("a node identifier fits");
        let Some(node) = search.nodes.get(slot).and_then(Option::as_ref) else {
            return Err(NoProof::NothingToCertify);
        };
        if let Node::Split { left, .. } = node {
            splits.push((*left, entry.id));
        }
    }
    splits.sort_unstable();

    for (_, id) in &splits {
        let slot = usize::try_from(*id).expect("a node identifier fits");
        let Some(Node::Split {
            column,
            at,
            left,
            right,
        }) = search.nodes.get(slot).and_then(Option::as_ref)
        else {
            return Err(NoProof::NothingToCertify);
        };
        let variable = u64::try_from(*column).expect("a column index fits");
        records.push(Record::Branch {
            parent: *id,
            variable,
            split: at.clone(),
            left: *left,
            right: *right,
        });
        // Left first, then right, which is the order the record names them and
        // the order the verifier allocates them in.
        assumption_ids[usize::try_from(*left).expect("fits")] = Some(next_id);
        assumption_ids[usize::try_from(*right).expect("fits")] = Some(next_id + 1);
        next_id += 2;
    }

    // Then the cuts, in the order the pool holds them, each naming the node
    // whose subtree it is true of. Their identifiers continue where the
    // assumptions left off, and the verifier allocates them in the same order
    // because it is reading the same stream.
    let mut cut_ids: Vec<u64> = Vec::new();
    for entry in search.pool.entries() {
        let record = match &entry.derivation {
            hc_cuts::pool::Derivation::Gomory {
                combination,
                isolated,
            } => Record::GmiCut {
                scope: entry.scope,
                combination: combination
                    .iter()
                    .map(|(row, value)| (*row as u64, value.clone()))
                    .collect(),
                isolated: *isolated as u64,
                coefficients: entry
                    .coefficients
                    .iter()
                    .map(|(column, value)| (*column as u64, value.clone()))
                    .collect(),
                right: entry.right.clone(),
            },
            hc_cuts::pool::Derivation::Cover {
                row,
                members,
                order,
            } => Record::CoverCut {
                scope: entry.scope,
                source: *row as u64,
                cover: members.iter().map(|column| *column as u64).collect(),
                lift_order: order.iter().map(|column| *column as u64).collect(),
                coefficients: entry
                    .coefficients
                    .iter()
                    .map(|(column, value)| (*column as u64, value.clone()))
                    .collect(),
            },
        };
        records.push(record);
        cut_ids.push(next_id);
        next_id += 1;
    }

    for entry in search.tree.entries() {
        let id = entry.id;
        let slot = usize::try_from(id).expect("a node identifier fits");
        let Some(node) = search.nodes.get(slot).and_then(Option::as_ref) else {
            return Err(NoProof::NothingToCertify);
        };

        match node {
            Node::Split { .. } => {}
            Node::Infeasible { basis } => {
                let (canonical, cuts) = at_node(&root, search, id);
                let mut bounds = node_bounds(model, &axioms, &search.tree, &assumption_ids, id);
                bounds.cuts = cut_supply(search, &cut_ids, id);
                let inference = contradiction_at(&canonical, basis, model, &bounds, &cuts)?;
                let record = records.len() as u64;
                records.push(Record::Contradiction {
                    node: id,
                    terms: inference,
                });
                records.push(Record::LeafInfeas {
                    node: id,
                    contradiction: record,
                });
            }
            Node::Bound { basis, value } | Node::Frontier { basis, value } => {
                let (canonical, cuts) = at_node(&root, search, id);
                let mut bounds = node_bounds(model, &axioms, &search.tree, &assumption_ids, id);
                bounds.cuts = cut_supply(search, &cut_ids, id);
                let terms = bound_at(&canonical, basis, model, &bounds, &cuts, value)?;
                let record = records.len() as u64;
                records.push(Record::DualBound { node: id, terms });
                records.push(if matches!(node, Node::Bound { .. }) {
                    Record::LeafBound {
                        node: id,
                        bound: record,
                    }
                } else {
                    Record::LeafFrontier {
                        node: id,
                        bound: record,
                    }
                });
            }
            Node::Integral {
                basis,
                point,
                value,
            } => {
                let (canonical, cuts) = at_node(&root, search, id);
                let mut bounds = node_bounds(model, &axioms, &search.tree, &assumption_ids, id);
                bounds.cuts = cut_supply(search, &cut_ids, id);
                let terms = bound_at(&canonical, basis, model, &bounds, &cuts, value)?;
                let record = records.len() as u64;
                records.push(Record::DualBound { node: id, terms });
                records.push(Record::LeafIntegral {
                    node: id,
                    point: canonical.structural_part(point).to_vec(),
                    bound: record,
                });
            }
            Node::Unproved { reason } => {
                return Err(NoProof::Unproved {
                    node: id,
                    reason: reason.clone(),
                });
            }
            Node::Suspect { value, parent } => {
                // A bound that came out below its parent's is not something to
                // write down more carefully; it is two numbers this side
                // computed that cannot both be right.
                return Err(NoProof::Unproved {
                    node: id,
                    reason: format!("its bound {value} is below its parent's {parent}"),
                });
            }
        }
    }

    let claim = close(search, &root, original, chain, &mut records)?;
    Ok(Certificate {
        format_tag: original.format().byte(),
        instance_bytes: instance.len() as u64,
        digest: *original.digest(),
        claim,
        records: Certificate::terminated(records),
    })
}

/// Assembles the integer-compatible ray certificate for an unbounded search.
///
/// A relaxation that runs away says nothing about the mixed-integer model by
/// itself: the integer points inside that region may be bounded, and there may
/// be none at all. What closes the gap is a lattice step — a positive integer
/// `q` for which every integer component of `q d` is a whole number. Then from
/// an integral feasible point `x0`, every `x0 + n q d` is feasible and integral
/// for every nonnegative integer `n`, the objective falls without limit along
/// them, and the model is unbounded over its own integer points rather than
/// over a relaxation of them.
///
/// Three things are needed and each may be absent, in which case there is no
/// claim to make and the search reports no progress rather than a weaker
/// statement dressed up as this one:
///
/// - **A ray.** Kept by the search from the node whose relaxation ran away.
///   The node's region is inside the model's, so a direction that recedes there
///   recedes here too; branch assumptions and cuts only ever remove directions.
/// - **An integral base point.** The incumbent, which reached that status by
///   passing the same exact screening every candidate does. The relaxation's own
///   vertex will not do — it is feasible but generally fractional, and a ray
///   walked from a fractional point visits no integer point at all.
/// - **No presolve.** A point lifts through the transform chain and a direction
///   does not: lifting is defined on points, and a direction is the homogeneous
///   part of one. Rather than invent a lift, a presolved search declines the
///   claim. The ray form forbids transform records for the same reason, so
///   there would be nowhere to record the chain even if it were lifted.
fn unbounded_certificate(
    model: &Model,
    original: &Model,
    chain: Option<&Chain>,
    instance: &[u8],
    search: &Search,
) -> Result<Certificate, NoProof> {
    if chain.is_some() {
        return Err(NoProof::NothingToCertify);
    }
    let witness = search.unbounded.as_ref().ok_or(NoProof::NothingToCertify)?;
    let incumbent = search.incumbent.as_ref().ok_or(NoProof::NothingToCertify)?;

    let root = Canonical::of(model);
    let (at, _) = at_node(&root, search, witness.node);
    let ray = ray_direction(&at, &witness.basis, witness.entering, witness.direction)?;
    let direction = at.structural_part(&ray).to_vec();

    // The step is the least common multiple of the denominators the direction
    // holds on integer columns, which is the smallest positive integer that
    // clears every one of them at once. A column the model leaves continuous
    // contributes nothing: it may move by any rational it likes.
    let mut step = BigUint::from_u64(1);
    for (column, value) in direction.iter().enumerate() {
        if root.is_integral(column) {
            step = step.lcm(value.denominator());
        }
    }

    let records = vec![
        Record::Ray {
            base: structural(&root, &incumbent.point),
            direction,
            step: Some(Rational::from_int(BigInt::from_magnitude(step))),
        },
        Record::Claim {
            kind: ClaimKind::Unbounded,
            objective: None,
            lower: None,
            gap: None,
        },
    ];
    Ok(Certificate {
        format_tag: original.format().byte(),
        instance_bytes: instance.len() as u64,
        digest: *original.digest(),
        claim: ClaimKind::Unbounded,
        records: Certificate::terminated(records),
    })
}

/// Assembles the point-only certificate for a search that proved no bound.
///
/// A tree with an uncertified node covers nothing: the region that node stood
/// for is unaccounted for, so no bound follows from the rest and a gap claim is
/// unavailable. The honest remainder is the incumbent itself — a point that is
/// feasible in the original model, integral where declared, and attains the
/// objective it states. That is a true thing to say, and it is the only true
/// thing this search has.
///
/// The form carries the point and nothing else. Transform and tree records are
/// forbidden in it, because a reader who saw a tree would reasonably infer that
/// a bound existed somewhere in it, and none does. The verifier reparses the
/// original bytes and checks the point directly.
///
/// With no incumbent there is nothing to say at all, and the search reports no
/// progress — which is what the claim vocabulary reserves that status for.
fn point_only_certificate(
    model: &Model,
    original: &Model,
    chain: Option<&Chain>,
    instance: &[u8],
    search: &Search,
) -> Result<Certificate, NoProof> {
    let incumbent = search.incumbent.as_ref().ok_or(NoProof::NothingToCertify)?;
    let root = Canonical::of(model);
    let reduced = structural(&root, &incumbent.point);

    // Unlike a direction, a point lifts through the chain, so a presolved
    // search says this as readily as an unpresolved one. What travels is the
    // original-space point, because that is the model the claim is about and
    // the only one the verifier will have parsed.
    let point = lifted(original, chain, &reduced);
    let objective = original.objective_at(&point);

    let records = vec![
        Record::FeasiblePoint {
            point,
            objective: objective.clone(),
        },
        Record::Claim {
            kind: ClaimKind::FeasibleNoBound,
            objective: Some(objective),
            lower: None,
            gap: None,
        },
    ];
    Ok(Certificate {
        format_tag: original.format().byte(),
        instance_bytes: instance.len() as u64,
        digest: *original.digest(),
        claim: ClaimKind::FeasibleNoBound,
        records: Certificate::terminated(records),
    })
}

/// Adds the incumbent and the claim, and says which claim it is.
fn close(
    search: &Search,
    root: &Canonical,
    original: &Model,
    chain: Option<&Chain>,
    records: &mut Vec<Record>,
) -> Result<ClaimKind, NoProof> {
    match search.ending {
        Ending::Infeasible => {
            records.push(Record::Claim {
                kind: ClaimKind::Infeasible,
                objective: None,
                lower: None,
                gap: None,
            });
            Ok(ClaimKind::Infeasible)
        }
        Ending::Optimal => {
            let incumbent = search.incumbent.as_ref().ok_or(NoProof::NothingToCertify)?;
            let point = structural(root, &incumbent.point);
            records.push(Record::Incumbent {
                original: lifted(original, chain, &point),
                reduced: point,
                objective: incumbent.objective.clone(),
            });
            records.push(Record::Claim {
                kind: ClaimKind::Optimal,
                objective: Some(incumbent.objective.clone()),
                lower: None,
                gap: None,
            });
            Ok(ClaimKind::Optimal)
        }
        Ending::Frontier => {
            let incumbent = search.incumbent.as_ref().ok_or(NoProof::NothingToCertify)?;
            let lower = search.lower_bound().ok_or(NoProof::NothingToCertify)?;
            if lower.cmp_value(&incumbent.objective).is_ge() {
                // Every open node is already no better than the incumbent, so
                // the tree closes and the certificate should have said so.
                // Reporting it rather than relabelling it, because two things
                // this crate computed disagree.
                return Err(NoProof::BoundAboveObjective {
                    bound: lower,
                    objective: incumbent.objective.clone(),
                });
            }
            let point = structural(root, &incumbent.point);
            records.push(Record::Incumbent {
                original: lifted(original, chain, &point),
                reduced: point,
                objective: incumbent.objective.clone(),
            });
            records.push(Record::Claim {
                kind: ClaimKind::FeasibleWithGap,
                objective: Some(incumbent.objective.clone()),
                lower: Some(lower.clone()),
                gap: Some(relative_gap(&incumbent.objective, &lower)),
            });
            Ok(ClaimKind::FeasibleWithGap)
        }
        // Neither of these reaches here: both are answered before the tree is
        // built, because neither claim carries a tree.
        Ending::UnboundedRelaxation | Ending::NoProgress => Err(NoProof::NothingToCertify),
    }
}

/// The structural half of a point over the working shape.
///
/// A point the search produced runs over every column, the logical variables
/// standing for row activities included. A certificate's witnesses are over the
/// model's own variables, because those are the ones the file declares and the
/// ones an independent parser will have built.
fn structural(root: &Canonical, point: &[Rational]) -> Vec<Rational> {
    root.structural_part(point).to_vec()
}

/// The original-space point a reduced one lifts to.
///
/// With no chain the two coordinate systems are one and the point is itself.
fn lifted(original: &Model, chain: Option<&Chain>, point: &[Rational]) -> Vec<Rational> {
    match chain {
        None => point.to_vec(),
        Some(chain) => chain.lift(original, point),
    }
}

/// The exact view of a node's subproblem.
fn at_node(
    root: &Canonical,
    search: &Search,
    node: u64,
) -> (Canonical, Vec<Vec<(usize, Rational)>>) {
    let tree = &search.tree;
    let mut bounds: Vec<(usize, Option<Rational>, Option<Rational>)> = Vec::new();
    for assumption in tree.path(node) {
        if assumption.at_most {
            bounds.push((assumption.column, None, Some(assumption.value.clone())));
        } else {
            bounds.push((assumption.column, Some(assumption.value.clone()), None));
        }
    }
    let narrowed = root.narrowed(&bounds);

    // The cuts visible at this node, in the order the search added them as
    // rows. The exact rebuild has to see exactly the rows the search solved,
    // or the multipliers it produces are about a different problem.
    let visible = search.pool.visible(&lineage(tree, node));
    let extra: Vec<hc_lp::exact::ExactRow> = visible
        .iter()
        .map(|entry| hc_lp::exact::ExactRow {
            terms: entry.coefficients.clone(),
            low: Some(entry.right.clone()),
            high: None,
        })
        .collect();
    let terms = visible
        .iter()
        .map(|entry| entry.coefficients.clone())
        .collect();
    (narrowed.with_rows(&extra), terms)
}

/// A node and every ancestor of it, the node first.
fn lineage(tree: &Tree, node: u64) -> Vec<u64> {
    let mut out = Vec::new();
    let mut at = Some(node);
    while let Some(id) = at {
        out.push(id);
        at = tree.get(id).and_then(|entry| entry.parent);
    }
    out
}

/// The identifier and right-hand side of each cut visible at a node, in the
/// order the search added them as rows.
fn cut_supply(search: &Search, ids: &[u64], node: u64) -> Vec<(u64, Rational)> {
    let lineage = lineage(&search.tree, node);
    search
        .pool
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, entry)| lineage.contains(&entry.scope))
        .map(|(position, entry)| {
            (
                ids.get(position).copied().unwrap_or_default(),
                entry.right.clone(),
            )
        })
        .collect()
}

/// The bounds an inference at a node may cite.
struct NodeBounds<'a> {
    model: ModelBounds<'a>,
    /// The tightest assumption on each side of each column, with its
    /// identifier.
    lower: Vec<Option<(u64, Rational)>>,
    upper: Vec<Option<(u64, Rational)>>,
    /// The identifier and right-hand side of each cut row, in row order after
    /// the model's own.
    cuts: Vec<(u64, Rational)>,
}

impl Bounds for NodeBounds<'_> {
    fn row(&self, row: RowId, side: Side) -> Option<Supply> {
        // Rows past the model's own are cuts. A cut is written `a . x >= b`,
        // so it has a lower side and no upper one, and a multiplier that
        // wanted the other side is asking for an inequality that does not
        // exist.
        let model_rows = self.model.model.rows().len();
        if row.index() >= model_rows {
            if side != Side::Lower {
                return None;
            }
            let (id, right) = self.cuts.get(row.index() - model_rows)?;
            return Some(Supply {
                id: *id,
                bound: right.clone(),
            });
        }
        // Branching never touches a row, so a row's sides are the model's.
        self.model.row(row, side)
    }

    fn var(&self, variable: VarId, side: Side) -> Option<Supply> {
        let overlay = match side {
            Side::Lower => self.lower.get(variable.index()),
            Side::Upper => self.upper.get(variable.index()),
        }
        .and_then(Option::as_ref);
        match overlay {
            Some((id, bound)) => Some(Supply {
                id: *id,
                bound: bound.clone(),
            }),
            None => self.model.var(variable, side),
        }
    }
}

/// Builds the overlay for one node from the assumptions on its path.
fn node_bounds<'a>(
    model: &'a Model,
    axioms: &'a Axioms,
    tree: &Tree,
    ids: &[Option<u64>],
    node: u64,
) -> NodeBounds<'a> {
    let width = model.variables().len();
    let mut lower: Vec<Option<(u64, Rational)>> = vec![None; width];
    let mut upper: Vec<Option<(u64, Rational)>> = vec![None; width];

    // Walk the path root end first, keeping the tightest assumption on each
    // side. A column may be branched more than once, and only the tightest is
    // the bound the arithmetic used.
    let mut at = Some(node);
    let mut chain: Vec<(u64, &Assumption)> = Vec::new();
    while let Some(current) = at {
        let entry = match tree.get(current) {
            Some(entry) => entry,
            None => break,
        };
        if let Some(assumption) = &entry.assumption
            && let Some(Some(id)) = ids.get(usize::try_from(current).unwrap_or(usize::MAX))
        {
            chain.push((*id, assumption));
        }
        at = entry.parent;
    }

    for (id, assumption) in chain {
        if assumption.column >= width {
            continue;
        }
        let slot = if assumption.at_most {
            &mut upper[assumption.column]
        } else {
            &mut lower[assumption.column]
        };
        let tighter = match slot.as_ref() {
            None => true,
            Some((_, seen)) => {
                if assumption.at_most {
                    assumption.value.cmp_value(seen).is_lt()
                } else {
                    assumption.value.cmp_value(seen).is_gt()
                }
            }
        };
        if tighter {
            *slot = Some((id, assumption.value.clone()));
        }
    }

    NodeBounds {
        model: ModelBounds { model, axioms },
        lower,
        upper,
        cuts: Vec::new(),
    }
}

/// The exact lower bound a node's basis establishes, as cited terms.
fn bound_at(
    canonical: &Canonical,
    basis: &hc_lp::Basis,
    model: &Model,
    bounds: &dyn Bounds,
    cuts: &[Vec<(usize, Rational)>],
    expected: &Rational,
) -> Result<Vec<(u64, Rational)>, NoProof> {
    let costs: Vec<Rational> = (0..canonical.columns())
        .map(|index| canonical.cost(index).clone())
        .collect();
    let multipliers = exact_multipliers(canonical, basis, &costs).map_err(NoProof::Rebuild)?;
    let target: Vec<Rational> = model
        .variables()
        .iter()
        .map(|variable| variable.objective.clone())
        .collect();
    let inference = assemble_with(
        bounds,
        model.rows().len(),
        model.variables().len(),
        model.matrix(),
        cuts,
        &multipliers,
        &target,
        model.objective_constant(),
    )
    .map_err(NoProof::Rebuild)?;

    // A self-check, diagnostic only: the bound the cited inequalities
    // establish must be the objective the node's own vertex attains. It is not
    // a substitute for the independent check — it shares this crate's
    // arithmetic — but a disagreement here is a defect on this side and
    // finding it now costs nothing.
    match inference.value.cmp_value(expected) {
        core::cmp::Ordering::Equal => Ok(inference.terms),
        core::cmp::Ordering::Less => Err(NoProof::BoundBelowObjective {
            bound: inference.value,
            objective: expected.clone(),
        }),
        core::cmp::Ordering::Greater => Err(NoProof::BoundAboveObjective {
            bound: inference.value,
            objective: expected.clone(),
        }),
    }
}

/// The contradiction a node's infeasible basis establishes, as cited terms.
fn contradiction_at(
    canonical: &Canonical,
    basis: &hc_lp::Basis,
    model: &Model,
    bounds: &dyn Bounds,
    cuts: &[Vec<(usize, Rational)>],
) -> Result<Vec<(u64, Rational)>, NoProof> {
    let point = hc_lp::exact::exact_point(canonical, basis, &|column| {
        resting_value(canonical, basis, column)
    })
    .map_err(NoProof::Rebuild)?;

    // The first phase's objective, recomputed exactly: which basic columns sit
    // outside their bounds, and which way.
    let mut costs = vec![Rational::zero(); canonical.columns()];
    for column in basis.basic() {
        let value = &point[*column];
        if canonical
            .lower(*column)
            .is_some_and(|bound| value.cmp_value(bound).is_lt())
        {
            costs[*column] = Rational::from_i64(-1);
        } else if canonical
            .upper(*column)
            .is_some_and(|bound| value.cmp_value(bound).is_gt())
        {
            costs[*column] = Rational::one();
        }
    }

    let multipliers = exact_multipliers(canonical, basis, &costs).map_err(NoProof::Rebuild)?;
    let zero = vec![Rational::zero(); model.variables().len()];
    let inference = assemble_with(
        bounds,
        model.rows().len(),
        model.variables().len(),
        model.matrix(),
        cuts,
        &multipliers,
        &zero,
        &Rational::zero(),
    )
    .map_err(NoProof::Rebuild)?;

    if !inference.value.is_positive() {
        return Err(NoProof::ContradictionNotPositive {
            value: inference.value,
        });
    }
    Ok(inference.terms)
}
