//! Closure: deciding whether the records present prove the status claimed.
//!
//! The rules in `vrules` establish facts about a model — this combination is a
//! valid lower bound, that point is feasible. None of them says anything about
//! what the certificate as a whole is entitled to claim. That is this file's
//! job, and it is separate for a reason: almost every way a proof system goes
//! wrong in practice is not a wrong inference but a correct inference used to
//! support a statement it does not support.
//!
//! Two rules run through everything here. The first is that recomputation
//! decides. A claim's stated objective, stated bound, and stated gap are
//! compared against values computed from the model and the witnesses, and the
//! stated ones are never adopted. The second is that a claim is entitled to
//! exactly the records it needs: a certificate proving a point and nothing
//! else may not carry a root, because carrying one would suggest support the
//! claim does not have, and a reader would be right to infer it.
//!
//! What is implemented here is the linear-program half of the calculus: the
//! shapes in the specification's record-order table that have no tree. A
//! certificate carrying branch or leaf records is refused, and it is refused
//! as a claim this implementation does not close rather than as a defect in
//! the certificate. That distinction is why the refusal names the closure rule
//! and says what it saw.

use crate::vaxiom::Axioms;
use crate::vhcc::{Claim, Proof, Step};
use crate::vmodel::Instance;
use crate::vparse::{Layout, read as read_instance};
use crate::vrat::Frac;
use crate::vreject::{Code, Rejected};
use crate::vrules::{
    bound_is, contradiction, dual_bound, everything, objective_is, point_is_feasible, relative_gap,
};
use crate::vwork::Work;

/// What a verification concluded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// The status the certificate claimed and this crate accepted.
    pub claim: Claim,
    /// The objective a witness attains, where the claim has one.
    pub objective: Option<Frac>,
    /// The lower bound established, where the claim has one.
    pub lower: Option<Frac>,
    /// The exact relative gap, where the claim has one.
    pub gap: Option<Frac>,
    /// The model's canonical hash, as the verifier recomputed it.
    pub model: [u8; 32],
    /// How many bytes the certificate occupied.
    pub bytes: usize,
    /// The verifier work units the check consumed, as an upper bound.
    pub work: u64,
    /// The widest rational the check handled, in bits.
    pub peak_bits: u64,
}

/// The claim record's contents.
///
/// A small named thing rather than a tuple, because three of its four fields
/// are optional rationals and a caller reading them positionally would get
/// them in the wrong order exactly once.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stated {
    kind: Claim,
    objective: Option<Frac>,
    lower: Option<Frac>,
    gap: Option<Frac>,
}

/// The layout a format tag names.
fn layout_of(tag: u8) -> Result<Layout, Rejected> {
    match tag {
        1 => Ok(Layout::Fixed),
        2 => Ok(Layout::Free),
        other => Err(Rejected::new(
            Code::PARSE_NOT_THIS_INSTANCE,
            format!("format tag {other} names a format this crate does not read"),
        )),
    }
}

/// The canonical hash of a model: the digest of its canonical dump.
///
/// Defined over the dump rather than over any implementation's data structure,
/// because the dump is the one rendering the two implementations already agree
/// on byte for byte. A hash over a struct would be a hash over a layout
/// decision.
#[must_use]
pub fn model_hash(instance: &Instance) -> [u8; 32] {
    crate::vhash::digest(instance.dump().as_bytes())
}

/// Reads a certificate, reparses the instance, and decides.
///
/// The instance is parsed here, by this crate's parser, from the bytes the
/// caller supplied. It is never taken from the certificate: a certificate that
/// carried its own model would be checked against its own account of the
/// problem, which is not checking.
///
/// # Errors
///
/// Returns [`Rejected`] naming the earliest rule that refused.
pub fn verify(certificate: &[u8], instance: &[u8]) -> Result<Verdict, Rejected> {
    let mut work = Work::new();
    // One unit per parsed byte, charged for both files before either is
    // interpreted. Both are parsed; both are paid for.
    work.read(certificate.len() as u64);
    work.read(instance.len() as u64);

    let proof = crate::vhcc::read(certificate, instance)?;
    let layout = layout_of(proof.tag)?;
    let model = read_instance(instance, layout)?;
    close(&mut work, &proof, &model)
}

/// Decides a read certificate against a parsed model.
///
/// Split from [`verify`] so that a caller holding both already — a differential
/// harness, say — does not have to reparse to ask the same question.
///
/// # Errors
///
/// Returns [`Rejected`] naming the earliest rule that refused.
pub fn close(work: &mut Work, proof: &Proof, model: &Instance) -> Result<Verdict, Rejected> {
    let axioms = Axioms::of(model);
    let hash = model_hash(model);

    // The claim record and the header must agree before either is used. They
    // are two statements of the same thing, and a disagreement means one of
    // them was edited.
    let Stated {
        kind,
        objective: stated_objective,
        lower: stated_lower,
        gap: stated_gap,
    } = assertion(proof)?;
    if kind != proof.claim {
        return Err(Rejected::new(
            Code::CLAIM_HEADER,
            format!(
                "the header claims {} and the claim record claims {}",
                proof.claim.word(),
                kind.word()
            ),
        ));
    }

    match kind {
        Claim::PointOnly => {
            direct_only(proof, Code::POINT_ONLY_EXTRA_RECORDS)?;
            let (point, stated) = feasible_point(proof)?;
            point_is_feasible(work, &axioms, point)?;
            let objective = objective_is(work, &axioms, point, stated)?;
            if let Some(claimed) = &stated_objective {
                objective_is(work, &axioms, point, claimed)?;
            }
            forbid(stated_lower.as_ref(), "a lower bound")?;
            forbid(stated_gap.as_ref(), "a gap")?;
            Ok(Verdict {
                claim: kind,
                objective: Some(objective),
                lower: None,
                gap: None,
                model: hash,
                bytes: proof.bytes,
                work: work.units(),
                peak_bits: work.peak_bits(),
            })
        }
        Claim::Unbounded => {
            direct_only(proof, Code::RAY_EXTRA_RECORDS)?;
            ray(work, proof, &axioms, model)?;
            forbid(stated_lower.as_ref(), "a lower bound")?;
            forbid(stated_gap.as_ref(), "a gap")?;
            Ok(Verdict {
                claim: kind,
                objective: None,
                lower: None,
                gap: None,
                model: hash,
                bytes: proof.bytes,
                work: work.units(),
                peak_bits: work.peak_bits(),
            })
        }
        Claim::Infeasible => {
            rooted(proof, &hash)?;
            let terms = single_contradiction(proof)?;
            contradiction(work, &axioms, terms, &everything)?;
            forbid(stated_objective.as_ref(), "an objective")?;
            forbid(stated_lower.as_ref(), "a lower bound")?;
            forbid(stated_gap.as_ref(), "a gap")?;
            Ok(Verdict {
                claim: kind,
                objective: None,
                lower: None,
                gap: None,
                model: hash,
                bytes: proof.bytes,
                work: work.units(),
                peak_bits: work.peak_bits(),
            })
        }
        Claim::Optimal | Claim::Gap => {
            rooted(proof, &hash)?;
            let terms = single_dual_bound(proof)?;
            let bound = dual_bound(work, &axioms, terms, &everything)?;

            // Recompute, then compare, in that order and at the first moment
            // both numbers exist. A stated bound that the combination does not
            // give is wrong however the rest of the certificate turns out, and
            // saying so here is what keeps the identifier the earliest one.
            let claimed_lower = match kind {
                Claim::Optimal => {
                    forbid(stated_lower.as_ref(), "a separate lower bound")?;
                    forbid(stated_gap.as_ref(), "a gap")?;
                    None
                }
                _ => {
                    let Some(claimed) = stated_lower.clone() else {
                        return Err(Rejected::new(
                            Code::CLAIM_WITNESS_MISSING,
                            "a gap claim states no lower bound",
                        ));
                    };
                    bound_is(&bound, &claimed)?;
                    Some(claimed)
                }
            };

            let (original, stated) = incumbent(proof)?;
            point_is_feasible(work, &axioms, original)?;
            let upper = objective_is(work, &axioms, original, stated)?;
            if let Some(claimed) = &stated_objective {
                objective_is(work, &axioms, original, claimed)?;
            }

            // A bound above the objective it is supposed to bound is not a
            // weak proof; it is a false one, and it is the shape a sign error
            // in the multipliers takes.
            match work.compare(&bound.value, &upper) {
                core::cmp::Ordering::Greater => {
                    return Err(Rejected::new(
                        Code::CLAIM_NOT_CLOSED,
                        format!(
                            "the lower bound {} exceeds the objective {upper} it bounds",
                            bound.value
                        ),
                    ));
                }
                core::cmp::Ordering::Equal if kind == Claim::Gap => {
                    // A gap of zero is optimality, and it has one spelling.
                    return Err(Rejected::new(
                        Code::CLAIM_BOUND_NOT_BELOW,
                        format!(
                            "the bound {} equals the objective and closes it",
                            bound.value
                        ),
                    ));
                }
                core::cmp::Ordering::Less if kind == Claim::Optimal => {
                    return Err(Rejected::new(
                        Code::CLAIM_NOT_CLOSED,
                        format!(
                            "the bound {} is below the objective {upper}, which is a gap",
                            bound.value
                        ),
                    ));
                }
                _ => {}
            }

            if kind == Claim::Optimal {
                return Ok(Verdict {
                    claim: kind,
                    objective: Some(upper),
                    lower: Some(bound.value),
                    gap: Some(Frac::zero()),
                    model: hash,
                    bytes: proof.bytes,
                    work: work.units(),
                    peak_bits: work.peak_bits(),
                });
            }

            debug_assert!(
                claimed_lower.is_some(),
                "a gap claim reached here with a bound"
            );
            let gap = relative_gap(work, &upper, &bound.value);
            let Some(claimed_gap) = stated_gap else {
                return Err(Rejected::new(
                    Code::CLAIM_WITNESS_MISSING,
                    "a gap claim states no gap",
                ));
            };
            if gap != claimed_gap {
                return Err(Rejected::new(
                    Code::CLAIM_GAP,
                    format!("the recomputed gap is {gap} and the certificate states {claimed_gap}"),
                ));
            }
            Ok(Verdict {
                claim: kind,
                objective: Some(upper),
                lower: Some(bound.value),
                gap: Some(gap),
                model: hash,
                bytes: proof.bytes,
                work: work.units(),
                peak_bits: work.peak_bits(),
            })
        }
    }
}

/// Refuses a value a claim is not entitled to state.
fn forbid(value: Option<&Frac>, what: &str) -> Result<(), Rejected> {
    match value {
        None => Ok(()),
        Some(present) => Err(Rejected::new(
            Code::CLAIM_KIND_FORBIDDEN,
            format!("the claim states {what}, {present}, which its kind does not carry"),
        )),
    }
}

/// The single claim record, and the numbers it states.
fn assertion(proof: &Proof) -> Result<Stated, Rejected> {
    let mut found = None;
    for step in &proof.steps {
        if let Step::Assertion {
            kind,
            stated_objective,
            stated_lower,
            stated_gap,
        } = step
        {
            if found.is_some() {
                return Err(Rejected::new(
                    Code::CLAIM_NOT_CLOSED,
                    "the certificate carries more than one claim record",
                ));
            }
            found = Some(Stated {
                kind: *kind,
                objective: stated_objective.clone(),
                lower: stated_lower.clone(),
                gap: stated_gap.clone(),
            });
        }
    }
    found.ok_or_else(|| {
        Rejected::new(
            Code::CLAIM_WITNESS_MISSING,
            "the certificate carries no claim record",
        )
    })
}

/// Refuses any record a direct original-space claim may not carry.
///
/// A point-only or ray certificate proves its statement against the original
/// model with no intermediate machinery. Every structural record is therefore
/// not merely unnecessary but misleading, and the specification forbids them
/// rather than ignoring them.
fn direct_only(proof: &Proof, code: Code) -> Result<(), Rejected> {
    for step in &proof.steps {
        if step.is_structural() {
            return Err(Rejected::new(
                code,
                format!(
                    "a direct claim carries a record of type {}, which implies support it does not have",
                    step.code()
                ),
            ));
        }
    }
    Ok(())
}

/// Requires a root whose stated hash is the model this crate recomputed, and
/// no transform records, which this implementation does not yet replay.
fn rooted(proof: &Proof, hash: &[u8; 32]) -> Result<(), Rejected> {
    for step in &proof.steps {
        if matches!(
            step,
            Step::Transform { .. }
                | Step::TransformContradiction { .. }
                | Step::Branch { .. }
                | Step::LeafBound { .. }
                | Step::LeafInfeas { .. }
                | Step::LeafIntegral { .. }
                | Step::LeafFrontier { .. }
                | Step::GmiCut { .. }
                | Step::CoverCut { .. }
        ) {
            return Err(Rejected::new(
                Code::CLAIM_NOT_CLOSED,
                format!(
                    "the certificate carries a record of type {}, which this implementation does not close",
                    step.code()
                ),
            ));
        }
    }

    let Some(Step::Root { node, stated_hash }) = proof.at(0) else {
        return Err(Rejected::new(
            Code::CLAIM_WITNESS_MISSING,
            "the certificate has no root record",
        ));
    };
    if *node != 0 {
        return Err(Rejected::new(
            Code::TREE_ROOT_HASH,
            format!("the root record names node {node}"),
        ));
    }
    if stated_hash != hash {
        return Err(Rejected::new(
            Code::TREE_ROOT_HASH,
            "the root names a model this crate did not build from the instance",
        ));
    }
    Ok(())
}

/// The one dual bound a linear-program claim rests on.
fn single_dual_bound(proof: &Proof) -> Result<&[(u64, Frac)], Rejected> {
    let mut found = None;
    for step in &proof.steps {
        if let Step::DualBound { node, terms } = step {
            if *node != 0 {
                return Err(Rejected::new(
                    Code::SCOPE_NOT_VISIBLE,
                    format!("a linear program's inference names node {node}"),
                ));
            }
            if found.is_some() {
                return Err(Rejected::new(
                    Code::CLAIM_NOT_CLOSED,
                    "a linear program's claim rests on more than one bound",
                ));
            }
            found = Some(terms.as_slice());
        }
        if matches!(step, Step::Contradiction { .. }) {
            return Err(Rejected::new(
                Code::CLAIM_KIND_FORBIDDEN,
                "a bounded claim carries a contradiction",
            ));
        }
    }
    found.ok_or_else(|| {
        Rejected::new(
            Code::CLAIM_WITNESS_MISSING,
            "the claim needs a lower bound and the certificate has none",
        )
    })
}

/// The one contradiction an infeasibility claim rests on.
fn single_contradiction(proof: &Proof) -> Result<&[(u64, Frac)], Rejected> {
    let mut found = None;
    for step in &proof.steps {
        if let Step::Contradiction { node, terms } = step {
            if *node != 0 {
                return Err(Rejected::new(
                    Code::SCOPE_NOT_VISIBLE,
                    format!("a linear program's inference names node {node}"),
                ));
            }
            if found.is_some() {
                return Err(Rejected::new(
                    Code::CLAIM_NOT_CLOSED,
                    "an infeasibility rests on more than one contradiction",
                ));
            }
            found = Some(terms.as_slice());
        }
        if matches!(step, Step::DualBound { .. } | Step::Incumbent { .. }) {
            return Err(Rejected::new(
                Code::CLAIM_KIND_FORBIDDEN,
                "an infeasible claim carries a bound or an incumbent",
            ));
        }
    }
    found.ok_or_else(|| {
        Rejected::new(
            Code::CLAIM_WITNESS_MISSING,
            "an infeasible claim carries no contradiction",
        )
    })
}

/// The incumbent's original-space point and the objective it states.
fn incumbent(proof: &Proof) -> Result<(&[Frac], &Frac), Rejected> {
    let mut found = None;
    for step in &proof.steps {
        if let Step::Incumbent {
            reduced,
            original,
            stated_objective,
        } = step
        {
            if found.is_some() {
                return Err(Rejected::new(
                    Code::CLAIM_NOT_CLOSED,
                    "the certificate carries more than one incumbent",
                ));
            }
            // With no transforms replayed the reduced model is the original
            // one, so the two witnesses must be the same vector. When presolve
            // arrives this becomes a mapping through the chain; today it is an
            // equality, and an equality is what it must be.
            if reduced != original {
                return Err(Rejected::new(
                    Code::POINT_CORRESPONDENCE,
                    "the reduced and original witnesses differ with no transform between them",
                ));
            }
            found = Some((original.as_slice(), stated_objective));
        }
    }
    found.ok_or_else(|| {
        Rejected::new(
            Code::CLAIM_WITNESS_MISSING,
            "the claim needs a point and the certificate has none",
        )
    })
}

/// The point-only claim's witness.
fn feasible_point(proof: &Proof) -> Result<(&[Frac], &Frac), Rejected> {
    let mut found = None;
    for step in &proof.steps {
        if let Step::FeasiblePoint {
            point,
            stated_objective,
        } = step
        {
            if found.is_some() {
                return Err(Rejected::new(
                    Code::CLAIM_NOT_CLOSED,
                    "the certificate carries more than one point",
                ));
            }
            found = Some((point.as_slice(), stated_objective));
        }
    }
    found.ok_or_else(|| {
        Rejected::new(
            Code::CLAIM_WITNESS_MISSING,
            "a point-only claim carries no point",
        )
    })
}

/// Checks an unboundedness witness against the original model.
///
/// Three separate things have to hold and each has its own rule, because a
/// report that said only "the ray is bad" would leave the reader to work out
/// which of them failed.
fn ray(
    work: &mut Work,
    proof: &Proof,
    axioms: &Axioms<'_>,
    model: &Instance,
) -> Result<(), Rejected> {
    let mut found = None;
    for step in &proof.steps {
        if let Step::Ray {
            base,
            direction,
            step: size,
        } = step
        {
            if found.is_some() {
                return Err(Rejected::new(
                    Code::CLAIM_NOT_CLOSED,
                    "the certificate carries more than one ray",
                ));
            }
            found = Some((base, direction, size));
        }
    }
    let Some((base, direction, size)) = found else {
        return Err(Rejected::new(
            Code::CLAIM_WITNESS_MISSING,
            "an unbounded claim carries no ray",
        ));
    };

    if direction.len() != model.columns.len() {
        return Err(Rejected::new(
            Code::POINT_DIMENSION,
            format!(
                "the direction has {} components and the model has {} columns",
                direction.len(),
                model.columns.len()
            ),
        ));
    }

    // The base point first: a direction that improves forever from a point
    // outside the feasible region proves nothing at all.
    point_is_feasible(work, axioms, base).map_err(|refusal| {
        let code = match refusal.code {
            Code::POINT_INTEGRALITY => Code::RAY_BASE_INTEGRALITY,
            Code::POINT_DIMENSION => Code::POINT_DIMENSION,
            _ => Code::RAY_BASE_INFEASIBLE,
        };
        Rejected::new(code, refusal.detail)
    })?;

    // The direction must lie in the recession cone: no row may move the wrong
    // way, and no finite bound may be left.
    for (index, row) in model.rows.iter().enumerate() {
        let change = model.activity_at(index as u32, direction).ok_or_else(|| {
            Rejected::new(Code::POINT_DIMENSION, "the direction is the wrong length")
        })?;
        if row.low.is_some() && change.is_minus() {
            return Err(Rejected::new(
                Code::RAY_ROW,
                format!(
                    "row {} decreases along the direction and has a lower side",
                    row.name
                ),
            ));
        }
        if row.high.is_some() && change.is_plus() {
            return Err(Rejected::new(
                Code::RAY_ROW,
                format!(
                    "row {} increases along the direction and has an upper side",
                    row.name
                ),
            ));
        }
    }
    for (column, change) in model.columns.iter().zip(direction) {
        if column.low.is_some() && change.is_minus() {
            return Err(Rejected::new(
                Code::RAY_BOUND,
                format!(
                    "column {} decreases along the direction and has a lower bound",
                    column.name
                ),
            ));
        }
        if column.high.is_some() && change.is_plus() {
            return Err(Rejected::new(
                Code::RAY_BOUND,
                format!(
                    "column {} increases along the direction and has an upper bound",
                    column.name
                ),
            ));
        }
    }

    // And the objective must strictly improve, or the direction is merely one
    // the region happens to extend in.
    let mut slope = Frac::zero();
    for (column, change) in model.columns.iter().zip(direction) {
        if column.cost.is_zero() || change.is_zero() {
            continue;
        }
        let term = work.multiply(&column.cost, change);
        slope = work.add(&slope, &term);
    }
    if !slope.is_minus() {
        return Err(Rejected::new(
            Code::RAY_NOT_IMPROVING,
            format!("the objective changes by {slope} along the direction"),
        ));
    }

    // A model with an integer column needs a step that keeps the walk on the
    // lattice. A model without one needs no step, and carrying a meaningless
    // step is not an error.
    let integral = model.columns.iter().any(|column| column.whole);
    match size {
        None if integral => Err(Rejected::new(
            Code::RAY_NO_STEP,
            "the model declares integrality and the ray carries no step",
        )),
        None => Ok(()),
        Some(step) => {
            if !step.is_plus() || !step.is_whole() {
                return Err(Rejected::new(
                    Code::RAY_NO_STEP,
                    format!("the ray's step is {step}"),
                ));
            }
            for (column, change) in model.columns.iter().zip(direction) {
                if column.whole && !work.multiply(step, change).is_whole() {
                    return Err(Rejected::new(
                        Code::RAY_STEP_INTEGRALITY,
                        format!(
                            "column {} is integer and the stepped direction is {}",
                            column.name,
                            work.multiply(step, change)
                        ),
                    ));
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vhcc::from_annotated_hex;
    use crate::vint::Int;
    use crate::vmodel::whole;

    const VECTOR: &str = include_str!("../../../spec/v1/vectors/hcc/tiny-optimal.hex");
    const TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

    fn frozen() -> Vec<u8> {
        from_annotated_hex(VECTOR).expect("the frozen vector is hexadecimal")
    }

    fn ratio(top: i64, bottom: i64) -> Frac {
        Frac::ratio(&Int::from_i64(top), &Int::from_i64(bottom)).expect("a nonzero denominator")
    }

    fn model() -> Instance {
        read_instance(TINY, Layout::Fixed).expect("the frozen instance parses")
    }

    /// A certificate shaped by hand, so that a test can say exactly what it is
    /// changing. Building the records rather than the bytes keeps the closure
    /// tests about closure; the byte level has its own vectors.
    fn proof(claim: Claim, steps: Vec<Step>) -> Proof {
        let mut steps = steps;
        let mut counts: Vec<(u8, u64)> = Vec::new();
        for step in &steps {
            match counts.iter_mut().find(|(code, _)| *code == step.code()) {
                Some((_, tally)) => *tally += 1,
                None => counts.push((step.code(), 1)),
            }
        }
        counts.push((127, 1));
        counts.sort_by_key(|(code, _)| *code);
        steps.push(Step::Terminator { counts });
        Proof {
            tag: 1,
            instance_bytes: TINY.len() as u64,
            digest: crate::vhash::instance_digest(1, TINY),
            claim,
            bytes: 0,
            steps,
        }
    }

    fn root() -> Step {
        Step::Root {
            node: 0,
            stated_hash: model_hash(&model()),
        }
    }

    fn optimal_steps() -> Vec<Step> {
        vec![
            root(),
            Step::DualBound {
                node: 0,
                terms: vec![(0, Frac::one())],
            },
            Step::Incumbent {
                reduced: vec![whole(4)],
                original: vec![whole(4)],
                stated_objective: whole(-4),
            },
            Step::Assertion {
                kind: Claim::Optimal,
                stated_objective: Some(whole(-4)),
                stated_lower: None,
                stated_gap: None,
            },
        ]
    }

    #[test]
    fn the_frozen_vector_verifies_end_to_end() {
        let verdict = verify(&frozen(), TINY).expect("the frozen certificate is accepted");
        assert_eq!(verdict.claim, Claim::Optimal);
        assert_eq!(
            verdict.objective.map(|value| value.to_canonical()),
            Some("-4".to_owned())
        );
        assert_eq!(
            verdict.lower.map(|value| value.to_canonical()),
            Some("-4".to_owned())
        );
        assert_eq!(
            verdict.gap.map(|value| value.to_canonical()),
            Some("0".to_owned())
        );
        assert_eq!(verdict.model, model_hash(&model()));
        assert_eq!(verdict.bytes, 150);
    }

    #[test]
    fn a_header_that_disagrees_with_the_claim_record_is_refused() {
        let refusal = close(
            &mut Work::new(),
            &proof(Claim::Gap, optimal_steps()),
            &model(),
        )
        .expect_err("the two must agree");
        assert_eq!(refusal.code, Code::CLAIM_HEADER);
    }

    #[test]
    fn a_root_naming_another_model_is_refused() {
        let mut steps = optimal_steps();
        steps[0] = Step::Root {
            node: 0,
            stated_hash: [7; 32],
        };
        let refusal = close(&mut Work::new(), &proof(Claim::Optimal, steps), &model())
            .expect_err("the hash must match");
        assert_eq!(refusal.code, Code::TREE_ROOT_HASH);
    }

    #[test]
    fn an_optimal_claim_whose_bound_falls_short_is_a_gap_and_is_refused_as_one() {
        let mut steps = optimal_steps();
        // The variable's upper bound also sums to the objective and proves the
        // weaker bound of -10. The derivation is sound; the claim is not.
        steps[1] = Step::DualBound {
            node: 0,
            terms: vec![(2, Frac::one())],
        };
        let refusal = close(&mut Work::new(), &proof(Claim::Optimal, steps), &model())
            .expect_err("optimality is not closed");
        assert_eq!(refusal.code, Code::CLAIM_NOT_CLOSED);
    }

    #[test]
    fn the_same_records_close_a_gap_claim_when_it_states_the_right_numbers() {
        let steps = vec![
            root(),
            Step::DualBound {
                node: 0,
                terms: vec![(2, Frac::one())],
            },
            Step::Incumbent {
                reduced: vec![whole(4)],
                original: vec![whole(4)],
                stated_objective: whole(-4),
            },
            Step::Assertion {
                kind: Claim::Gap,
                stated_objective: Some(whole(-4)),
                stated_lower: Some(whole(-10)),
                stated_gap: Some(ratio(6, 4)),
            },
        ];
        let verdict =
            close(&mut Work::new(), &proof(Claim::Gap, steps), &model()).expect("a real gap");
        assert_eq!(
            verdict.gap.map(|value| value.to_canonical()),
            Some("3/2".to_owned())
        );
    }

    #[test]
    fn a_gap_claim_that_misstates_its_gap_is_refused() {
        let steps = vec![
            root(),
            Step::DualBound {
                node: 0,
                terms: vec![(2, Frac::one())],
            },
            Step::Incumbent {
                reduced: vec![whole(4)],
                original: vec![whole(4)],
                stated_objective: whole(-4),
            },
            Step::Assertion {
                kind: Claim::Gap,
                stated_objective: Some(whole(-4)),
                stated_lower: Some(whole(-10)),
                stated_gap: Some(Frac::one()),
            },
        ];
        let refusal = close(&mut Work::new(), &proof(Claim::Gap, steps), &model())
            .expect_err("the gap is wrong");
        assert_eq!(refusal.code, Code::CLAIM_GAP);
    }

    #[test]
    fn a_gap_claim_stating_a_bound_the_combination_does_not_give_is_refused() {
        let steps = vec![
            root(),
            Step::DualBound {
                node: 0,
                terms: vec![(0, Frac::one())],
            },
            Step::Incumbent {
                reduced: vec![whole(4)],
                original: vec![whole(4)],
                stated_objective: whole(-4),
            },
            Step::Assertion {
                kind: Claim::Gap,
                stated_objective: Some(whole(-4)),
                stated_lower: Some(whole(-3)),
                stated_gap: Some(Frac::zero()),
            },
        ];
        let refusal = close(&mut Work::new(), &proof(Claim::Gap, steps), &model())
            .expect_err("the bound is wrong");
        assert_eq!(refusal.code, Code::DUAL_BOUND_VALUE);
    }

    #[test]
    fn a_zero_gap_is_optimality_and_has_one_spelling() {
        let mut steps = optimal_steps();
        steps[3] = Step::Assertion {
            kind: Claim::Gap,
            stated_objective: Some(whole(-4)),
            stated_lower: Some(whole(-4)),
            stated_gap: Some(Frac::zero()),
        };
        let refusal = close(&mut Work::new(), &proof(Claim::Gap, steps), &model())
            .expect_err("it is optimal");
        assert_eq!(refusal.code, Code::CLAIM_BOUND_NOT_BELOW);
    }

    #[test]
    fn an_incumbent_that_is_not_feasible_is_refused_before_anything_is_concluded() {
        let mut steps = optimal_steps();
        steps[2] = Step::Incumbent {
            reduced: vec![whole(9)],
            original: vec![whole(9)],
            stated_objective: whole(-9),
        };
        let refusal = close(&mut Work::new(), &proof(Claim::Optimal, steps), &model())
            .expect_err("the point breaks the row");
        assert_eq!(refusal.code, Code::POINT_ROW);
    }

    #[test]
    fn an_incumbent_whose_objective_is_misstated_is_refused() {
        let mut steps = optimal_steps();
        steps[2] = Step::Incumbent {
            reduced: vec![whole(4)],
            original: vec![whole(4)],
            stated_objective: whole(-5),
        };
        let refusal = close(&mut Work::new(), &proof(Claim::Optimal, steps), &model())
            .expect_err("the objective is recomputed");
        assert_eq!(refusal.code, Code::POINT_OBJECTIVE);
    }

    #[test]
    fn a_point_only_claim_may_not_carry_a_root() {
        let steps = vec![
            root(),
            Step::FeasiblePoint {
                point: vec![whole(4)],
                stated_objective: whole(-4),
            },
            Step::Assertion {
                kind: Claim::PointOnly,
                stated_objective: Some(whole(-4)),
                stated_lower: None,
                stated_gap: None,
            },
        ];
        let refusal = close(&mut Work::new(), &proof(Claim::PointOnly, steps), &model())
            .expect_err("the root implies support it does not have");
        assert_eq!(refusal.code, Code::POINT_ONLY_EXTRA_RECORDS);
    }

    #[test]
    fn a_point_only_claim_proves_its_point_and_nothing_more() {
        let steps = vec![
            Step::FeasiblePoint {
                point: vec![whole(3)],
                stated_objective: whole(-3),
            },
            Step::Assertion {
                kind: Claim::PointOnly,
                stated_objective: Some(whole(-3)),
                stated_lower: None,
                stated_gap: None,
            },
        ];
        let verdict = close(&mut Work::new(), &proof(Claim::PointOnly, steps), &model())
            .expect("a feasible point");
        assert_eq!(
            verdict.objective.map(|value| value.to_canonical()),
            Some("-3".to_owned())
        );
        assert_eq!(verdict.lower, None);
        assert_eq!(verdict.gap, None);
    }

    #[test]
    fn a_point_only_claim_that_states_a_lower_bound_is_refused() {
        let steps = vec![
            Step::FeasiblePoint {
                point: vec![whole(3)],
                stated_objective: whole(-3),
            },
            Step::Assertion {
                kind: Claim::PointOnly,
                stated_objective: Some(whole(-3)),
                stated_lower: Some(whole(-10)),
                stated_gap: None,
            },
        ];
        let refusal = close(&mut Work::new(), &proof(Claim::PointOnly, steps), &model())
            .expect_err("the kind carries no bound");
        assert_eq!(refusal.code, Code::CLAIM_KIND_FORBIDDEN);
    }

    #[test]
    fn an_infeasibility_needs_the_bound_multiplier_and_no_incumbent() {
        // Minimize nothing subject to x <= 2 with x at least 3.
        let mut instance = model();
        instance.columns[0].low = Some(whole(3));
        instance.columns[0].high = None;
        instance.columns[0].cost = Frac::zero();
        instance.columns[0].whole = false;
        instance.rows[0].high = Some(whole(2));

        let complete = vec![
            Step::Root {
                node: 0,
                stated_hash: model_hash(&instance),
            },
            Step::Contradiction {
                node: 0,
                terms: vec![(0, Frac::one()), (1, Frac::one())],
            },
            Step::Assertion {
                kind: Claim::Infeasible,
                stated_objective: None,
                stated_lower: None,
                stated_gap: None,
            },
        ];
        let verdict = close(
            &mut Work::new(),
            &proof(Claim::Infeasible, complete),
            &instance,
        )
        .expect("infeasible");
        assert_eq!(verdict.claim, Claim::Infeasible);

        let partial = vec![
            Step::Root {
                node: 0,
                stated_hash: model_hash(&instance),
            },
            Step::Contradiction {
                node: 0,
                terms: vec![(0, Frac::one())],
            },
            Step::Assertion {
                kind: Claim::Infeasible,
                stated_objective: None,
                stated_lower: None,
                stated_gap: None,
            },
        ];
        let refusal = close(
            &mut Work::new(),
            &proof(Claim::Infeasible, partial),
            &instance,
        )
        .expect_err("the omission must be caught");
        assert_eq!(refusal.code, Code::DUAL_NOT_ZERO_SUM);
    }

    #[test]
    fn a_ray_is_checked_at_its_base_in_its_direction_and_against_the_objective() {
        // Minimize -x with x at least zero and no upper bound: unbounded, and
        // the whole point is that the model has an integer column, so the ray
        // needs a step that keeps the walk on the lattice.
        let mut instance = model();
        instance.columns[0].high = None;
        instance.rows.clear();
        instance.cells.clear();

        let good = vec![
            Step::Ray {
                base: vec![whole(0)],
                direction: vec![Frac::one()],
                step: Some(Frac::one()),
            },
            Step::Assertion {
                kind: Claim::Unbounded,
                stated_objective: None,
                stated_lower: None,
                stated_gap: None,
            },
        ];
        close(&mut Work::new(), &proof(Claim::Unbounded, good), &instance).expect("a real ray");

        let no_step = vec![
            Step::Ray {
                base: vec![whole(0)],
                direction: vec![Frac::one()],
                step: None,
            },
            Step::Assertion {
                kind: Claim::Unbounded,
                stated_objective: None,
                stated_lower: None,
                stated_gap: None,
            },
        ];
        assert_eq!(
            close(
                &mut Work::new(),
                &proof(Claim::Unbounded, no_step),
                &instance
            )
            .expect_err("an integer model needs a step")
            .code,
            Code::RAY_NO_STEP
        );

        let fractional = vec![
            Step::Ray {
                base: vec![whole(0)],
                direction: vec![ratio(1, 3)],
                step: Some(Frac::one()),
            },
            Step::Assertion {
                kind: Claim::Unbounded,
                stated_objective: None,
                stated_lower: None,
                stated_gap: None,
            },
        ];
        assert_eq!(
            close(
                &mut Work::new(),
                &proof(Claim::Unbounded, fractional),
                &instance
            )
            .expect_err("the stepped direction must be integral")
            .code,
            Code::RAY_STEP_INTEGRALITY
        );

        let wrong_way = vec![
            Step::Ray {
                base: vec![whole(0)],
                direction: vec![Frac::one().negated()],
                step: Some(Frac::one()),
            },
            Step::Assertion {
                kind: Claim::Unbounded,
                stated_objective: None,
                stated_lower: None,
                stated_gap: None,
            },
        ];
        assert_eq!(
            close(
                &mut Work::new(),
                &proof(Claim::Unbounded, wrong_way),
                &instance
            )
            .expect_err("the direction leaves the lower bound")
            .code,
            Code::RAY_BOUND
        );

        let infeasible_base = vec![
            Step::Ray {
                base: vec![whole(-1)],
                direction: vec![Frac::one()],
                step: Some(Frac::one()),
            },
            Step::Assertion {
                kind: Claim::Unbounded,
                stated_objective: None,
                stated_lower: None,
                stated_gap: None,
            },
        ];
        assert_eq!(
            close(
                &mut Work::new(),
                &proof(Claim::Unbounded, infeasible_base),
                &instance
            )
            .expect_err("the base must be feasible")
            .code,
            Code::RAY_BASE_INFEASIBLE
        );
    }

    #[test]
    fn a_certificate_with_no_claim_record_closes_nothing() {
        let refusal = close(
            &mut Work::new(),
            &proof(Claim::Optimal, vec![root()]),
            &model(),
        )
        .expect_err("a claim is required");
        assert_eq!(refusal.code, Code::CLAIM_WITNESS_MISSING);
    }
}
