//! Turning a solve into a certificate, or admitting there is not one.
//!
//! Everything the search produced is rebuilt here from the model's own
//! rationals: the point, the multipliers, the bound, the direction. Nothing
//! floating point crosses into a record. The search's numbers are used for
//! exactly one thing — to say which columns were basic and which way an
//! unbounded column was moving — and even those are checked rather than
//! believed.
//!
//! Assembly failing is a normal outcome and is reported as one. A basis that
//! is singular exactly, a bound that would have to cite an axiom the model
//! does not contain, a point that turns out not to be feasible after all: each
//! of those means the stronger claim is unavailable, and the honest response
//! is to make the weaker one or none at all. There is no path here that
//! upgrades a weaker statement to a stronger one.

use hc_core::{Model, Sha256};
use hc_exact::Rational;
#[cfg(test)]
use hc_lp::Problem;
use hc_lp::{Basis, Outcome, Status};

use crate::hcc::{Certificate, ClaimKind, Record};
use crate::lp::{self, Axioms, Gap};
use hc_lp::exact::{Canonical, System, rebuild};

/// Why no certificate could be assembled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoProof {
    /// The exact rebuild could not be completed.
    Rebuild(Gap),
    /// The rebuilt point is not feasible in the model.
    ///
    /// The float search believed it was, within its fence. Exactly, it is not,
    /// and the fence is not a thing a proof may appeal to.
    PointInfeasible {
        /// The column that is outside its bounds.
        column: usize,
    },
    /// The rebuilt point does not satisfy the system exactly.
    ResidualNotZero {
        /// The row with a nonzero residual.
        row: usize,
    },
    /// The bound and the point's objective do not meet, so the claim is not
    /// optimality.
    BoundBelowObjective {
        /// The bound that was established.
        bound: Rational,
        /// The objective the point attains.
        objective: Rational,
    },
    /// The bound exceeds the objective of a feasible point, which cannot be.
    ///
    /// This is not a weaker claim to fall back to; it is a contradiction
    /// between two things this crate computed, and it is reported rather than
    /// resolved.
    BoundAboveObjective {
        /// The bound that was established.
        bound: Rational,
        /// The objective the point attains.
        objective: Rational,
    },
    /// The contradiction's value is not strictly positive, so it proves
    /// nothing.
    ContradictionNotPositive {
        /// The value the multiplied right-hand sides summed to.
        value: Rational,
    },
    /// The direction does not strictly improve the objective.
    RayDoesNotImprove {
        /// The rate of change along the direction.
        rate: Rational,
    },
    /// A node could not be closed and could not be given a bound.
    ///
    /// A tree holding one of these proves nothing about the region that node
    /// covers, so there is no certificate to write and the node is named.
    Unproved {
        /// The node.
        node: u64,
        /// Why its rebuild declined.
        reason: String,
    },
    /// The search reached no conclusion worth certifying.
    NothingToCertify,
}

impl NoProof {
    /// A short, stable name for the reason.
    ///
    /// The `Display` form carries the row, the column, or the value that made
    /// the difference, which is what a person debugging one instance needs and
    /// exactly the wrong thing to tally a campaign by: a hundred declines for
    /// one reason would appear as a hundred reasons. This is the tally key.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Rebuild(Gap::Singular(_)) => "basis singular exactly",
            Self::Rebuild(Gap::NoSuchBound { .. }) => "no such bound to cite",
            Self::Rebuild(Gap::Infeasible { .. }) => "rebuilt point outside its bounds",
            Self::PointInfeasible { .. } => "point infeasible exactly",
            Self::ResidualNotZero { .. } => "residual not zero exactly",
            Self::BoundBelowObjective { .. } => "bound below the objective",
            Self::BoundAboveObjective { .. } => "bound above the objective",
            Self::ContradictionNotPositive { .. } => "contradiction not positive",
            Self::RayDoesNotImprove { .. } => "ray does not improve",
            Self::Unproved { .. } => "a node is unproved",
            Self::NothingToCertify => "nothing to certify",
        }
    }
}

impl core::fmt::Display for NoProof {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Rebuild(gap) => write!(formatter, "{gap}"),
            Self::Unproved { node, reason } => {
                write!(formatter, "node {node} is unproved: {reason}")
            }
            Self::PointInfeasible { column } => {
                write!(formatter, "column {column} is outside its bounds exactly")
            }
            Self::ResidualNotZero { row } => {
                write!(formatter, "row {row} is not satisfied exactly")
            }
            Self::BoundBelowObjective { bound, objective } => {
                write!(
                    formatter,
                    "the bound {bound} is below the objective {objective}"
                )
            }
            Self::BoundAboveObjective { bound, objective } => {
                write!(
                    formatter,
                    "the bound {bound} exceeds the objective {objective}"
                )
            }
            Self::ContradictionNotPositive { value } => {
                write!(
                    formatter,
                    "the contradiction sums to {value}, which is not positive"
                )
            }
            Self::RayDoesNotImprove { rate } => {
                write!(formatter, "the direction changes the objective by {rate}")
            }
            Self::NothingToCertify => formatter.write_str("the search reached no conclusion"),
        }
    }
}

impl core::error::Error for NoProof {}

/// The canonical hash of a model: the digest of its canonical dump.
///
/// Defined over the dump rather than over any implementation's model type,
/// because that is the one rendering two independently written implementations
/// already agree on byte for byte.
#[must_use]
pub fn model_hash(model: &Model) -> [u8; 32] {
    let mut state = Sha256::new();
    state.update(model.canonical_dump().as_bytes());
    state.finish()
}

/// Assembles a certificate for a linear program's outcome.
///
/// # Errors
///
/// Returns [`NoProof`] naming what stopped the assembly.
pub fn assemble_lp(
    model: &Model,
    instance: &[u8],
    outcome: &Outcome,
) -> Result<Certificate, NoProof> {
    let canonical = Canonical::of(model);
    let axioms = Axioms::of(model);
    let records = match outcome {
        Outcome::Optimal { basis, .. } => optimal_records(model, &canonical, &axioms, basis)?,
        Outcome::Infeasible { basis, .. } => infeasible_records(model, &canonical, &axioms, basis)?,
        Outcome::Unbounded {
            basis,
            entering,
            direction,
            ..
        } => unbounded_records(&canonical, basis, *entering, *direction)?,
        Outcome::EffortLimit {
            basis,
            feasible: true,
            ..
        } => point_only_records(&canonical, basis)?,
        _ => return Err(NoProof::NothingToCertify),
    };

    let claim = match records.iter().find_map(|record| match record {
        Record::Claim { kind, .. } => Some(*kind),
        _ => None,
    }) {
        Some(kind) => kind,
        None => return Err(NoProof::NothingToCertify),
    };

    Ok(Certificate {
        format_tag: model.format().byte(),
        instance_bytes: instance.len() as u64,
        digest: *model.digest(),
        claim,
        records,
    })
}

/// The exact point a basis implies, checked against the model.
fn checked_point(canonical: &Canonical, basis: &Basis) -> Result<Vec<Rational>, NoProof> {
    let point = rebuild::exact_point(canonical, basis, &|column| {
        rebuild::resting_value(canonical, basis, column)
    })
    .map_err(NoProof::Rebuild)?;

    if let Some(row) = canonical
        .residual_at(&point)
        .iter()
        .position(|value| !value.is_zero())
    {
        return Err(NoProof::ResidualNotZero { row });
    }
    if let Some(column) = canonical.violations(&point).first() {
        return Err(NoProof::PointInfeasible { column: *column });
    }
    Ok(point)
}

fn optimal_records(
    model: &Model,
    canonical: &Canonical,
    axioms: &Axioms,
    basis: &Basis,
) -> Result<Vec<Record>, NoProof> {
    let point = checked_point(canonical, basis)?;
    let objective = canonical.objective_at(&point);

    let costs: Vec<Rational> = (0..canonical.columns())
        .map(|index| canonical.cost(index).clone())
        .collect();
    let multipliers =
        rebuild::exact_multipliers(canonical, basis, &costs).map_err(NoProof::Rebuild)?;
    let target: Vec<Rational> = model
        .variables()
        .iter()
        .map(|variable| variable.objective.clone())
        .collect();
    let inference = lp::assemble(
        model,
        axioms,
        &multipliers,
        &target,
        model.objective_constant(),
    )
    .map_err(NoProof::Rebuild)?;

    // The two numbers must meet. Below and the claim is a gap; above and two
    // things this crate computed disagree, which is reported rather than
    // smoothed over.
    if inference.value > objective {
        return Err(NoProof::BoundAboveObjective {
            bound: inference.value,
            objective,
        });
    }
    let structural = canonical.structural_part(&point).to_vec();
    let mut records = vec![
        Record::Root {
            node: 0,
            model: model_hash(model),
        },
        Record::DualBound {
            node: 0,
            terms: inference
                .terms
                .iter()
                .map(|(id, value)| (*id, value.clone()))
                .collect(),
        },
        Record::Incumbent {
            reduced: structural.clone(),
            original: structural,
            objective: objective.clone(),
        },
    ];

    if inference.value < objective {
        // A real gap on a linear program. The bound is true and the point is
        // feasible; what is not established is that they meet.
        let gap = relative_gap(&objective, &inference.value);
        records.push(Record::Claim {
            kind: ClaimKind::FeasibleWithGap,
            objective: Some(objective),
            lower: Some(inference.value),
            gap: Some(gap),
        });
    } else {
        records.push(Record::Claim {
            kind: ClaimKind::Optimal,
            objective: Some(objective),
            lower: None,
            gap: None,
        });
    }

    Ok(Certificate::terminated(records))
}

fn infeasible_records(
    model: &Model,
    canonical: &Canonical,
    axioms: &Axioms,
    basis: &Basis,
) -> Result<Vec<Record>, NoProof> {
    // The first phase's objective, recomputed exactly rather than taken from
    // the search: which basic columns sit outside their bounds, and which way.
    let point = rebuild::exact_point(canonical, basis, &|column| {
        rebuild::resting_value(canonical, basis, column)
    })
    .map_err(NoProof::Rebuild)?;

    let mut costs = vec![Rational::zero(); canonical.columns()];
    for column in basis.basic() {
        let value = &point[*column];
        if canonical.lower(*column).is_some_and(|bound| value < bound) {
            costs[*column] = Rational::from_i64(-1);
        } else if canonical.upper(*column).is_some_and(|bound| value > bound) {
            costs[*column] = Rational::one();
        }
    }

    let multipliers =
        rebuild::exact_multipliers(canonical, basis, &costs).map_err(NoProof::Rebuild)?;
    let zero = vec![Rational::zero(); model.variables().len()];
    let inference = lp::assemble(model, axioms, &multipliers, &zero, &Rational::zero())
        .map_err(NoProof::Rebuild)?;

    if !inference.value.is_positive() {
        return Err(NoProof::ContradictionNotPositive {
            value: inference.value,
        });
    }

    let records = vec![
        Record::Root {
            node: 0,
            model: model_hash(model),
        },
        Record::Contradiction {
            node: 0,
            terms: inference
                .terms
                .iter()
                .map(|(id, value)| (*id, value.clone()))
                .collect(),
        },
        Record::Claim {
            kind: ClaimKind::Infeasible,
            objective: None,
            lower: None,
            gap: None,
        },
    ];
    Ok(Certificate::terminated(records))
}

fn unbounded_records(
    canonical: &Canonical,
    basis: &Basis,
    entering: usize,
    direction: i8,
) -> Result<Vec<Record>, NoProof> {
    let base = checked_point(canonical, basis)?;
    let ray = ray_direction(canonical, basis, entering, direction)?;

    let records = vec![
        Record::Ray {
            base: canonical.structural_part(&base).to_vec(),
            direction: canonical.structural_part(&ray).to_vec(),
            // A linear claim needs no step. The model declares no integer
            // column, so there is no lattice to stay on.
            step: None,
        },
        Record::Claim {
            kind: ClaimKind::Unbounded,
            objective: None,
            lower: None,
            gap: None,
        },
    ];
    Ok(Certificate::terminated(records))
}

/// Rebuilds an unbounded relaxation's direction exactly, over every column.
///
/// The simplex reports a ray of its own, and that ray is about the scaled
/// problem in rounded numbers. This one is rebuilt from the model: the entering
/// column moves by one, every basic column moves to keep the system satisfied,
/// and nothing else moves.
///
/// Both the linear and the mixed-integer claims want this, which is why it is
/// separate from either. What differs between them is the base point travelled
/// from and whether a lattice step is required — not the direction, which is a
/// fact about the region.
pub(crate) fn ray_direction(
    canonical: &Canonical,
    basis: &Basis,
    entering: usize,
    direction: i8,
) -> Result<Vec<Rational>, NoProof> {
    let mut column = vec![Rational::zero(); canonical.rows()];
    for (row, value) in canonical.column(entering) {
        column[*row as usize] = value.clone();
    }
    let columns: Vec<Vec<(u32, Rational)>> = basis
        .basic()
        .iter()
        .map(|index| canonical.column(*index).to_vec())
        .collect();
    let system = System::from_columns(canonical.rows(), &columns)
        .map_err(|failure| NoProof::Rebuild(Gap::Singular(failure)))?;
    let alpha = system
        .solve(&column)
        .map_err(|failure| NoProof::Rebuild(Gap::Singular(failure)))?;

    let sign = Rational::from_i64(i64::from(direction));
    let mut ray = vec![Rational::zero(); canonical.columns()];
    ray[entering] = sign.clone();
    for (row, index) in basis.basic().iter().enumerate() {
        ray[*index] = sign.neg().mul(&alpha[row]);
    }

    // The direction must strictly improve the objective, and it must be a
    // recession direction: the system stays satisfied and no finite bound is
    // left. The first two are checked here; the third is what the verifier
    // will check against the original model.
    let rate = (0..canonical.columns()).fold(Rational::zero(), |total, index| {
        total.add(&canonical.cost(index).mul(&ray[index]))
    });
    if !rate.is_negative() {
        return Err(NoProof::RayDoesNotImprove { rate });
    }
    if let Some(row) = canonical
        .residual_at(&ray)
        .iter()
        .position(|value| !value.is_zero())
    {
        return Err(NoProof::ResidualNotZero { row });
    }

    Ok(ray)
}

fn point_only_records(canonical: &Canonical, basis: &Basis) -> Result<Vec<Record>, NoProof> {
    let point = checked_point(canonical, basis)?;
    let objective = canonical.objective_at(&point);
    let records = vec![
        Record::FeasiblePoint {
            point: canonical.structural_part(&point).to_vec(),
            objective: objective.clone(),
        },
        Record::Claim {
            kind: ClaimKind::FeasibleNoBound,
            objective: Some(objective),
            lower: None,
            gap: None,
        },
    ];
    Ok(Certificate::terminated(records))
}

/// The exact relative gap, as the normative formula states it.
#[must_use]
pub fn relative_gap(upper: &Rational, lower: &Rational) -> Rational {
    let spread = upper.sub(lower);
    let scale = {
        let magnitude = upper.abs();
        if magnitude > Rational::one() {
            magnitude
        } else {
            Rational::one()
        }
    };
    spread.checked_div(&scale).unwrap_or_else(Rational::zero)
}

/// Whether a basis's statuses are self-consistent for assembly.
///
/// A cheap structural check performed before the expensive exact one, so that
/// an obviously malformed basis is reported as malformed rather than as a
/// singular system.
#[must_use]
pub fn basis_is_well_formed(basis: &Basis, rows: usize) -> bool {
    if basis.basic().len() != rows {
        return false;
    }
    (0..basis.columns()).all(|column| {
        basis.position_of(column).is_some() == (basis.status(column) == Status::Basic)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_core::Tol;
    use hc_lp::{Options, Working};

    fn model(source: &str) -> Model {
        hc_io::mps::read(source.as_bytes(), hc_io::mps::Form::Free).expect("accepted")
    }

    /// Solves a model and assembles whatever certificate it supports.
    fn certify(source: &str) -> Result<Certificate, NoProof> {
        let instance = source.as_bytes();
        let model = model(source);
        let working = Working::from_model(&model);
        let problem = Problem::scaled(&working, Tol::solver());
        let (outcome, _) = hc_lp::solve(&problem, &Options::default());
        assemble_lp(&model, instance, &outcome)
    }

    const OPTIMAL: &str = concat!(
        "ROWS\n N obj\n L r1\n L r2\n",
        "COLUMNS\n x obj -3 r1 1\n x r2 1\n y obj -2 r1 1\n y r2 3\n",
        "RHS\n rhs r1 4 r2 6\n",
        "ENDATA\n"
    );

    #[test]
    fn an_optimal_solve_produces_the_records_the_order_requires() {
        let certificate = certify(OPTIMAL).expect("a certificate");
        assert_eq!(certificate.claim, ClaimKind::Optimal);
        let codes: Vec<u8> = certificate.records.iter().map(Record::code).collect();
        assert_eq!(
            codes,
            vec![3, 16, 80, 96, 127],
            "root, bound, incumbent, claim, end"
        );
        assert_eq!(certificate.digest, *model(OPTIMAL).digest());
    }

    #[test]
    fn the_certificate_round_trips_through_its_bytes() {
        let certificate = certify(OPTIMAL).expect("a certificate");
        let bytes = certificate.encode();
        let recovered = crate::read::decode(&bytes).expect("well formed");
        assert_eq!(recovered, certificate);
        crate::read::bind(&recovered, OPTIMAL.as_bytes()).expect("bound to its instance");
    }

    #[test]
    fn the_bound_and_the_incumbent_meet_exactly_at_an_optimum() {
        let certificate = certify(OPTIMAL).expect("a certificate");
        let objective = certificate.records.iter().find_map(|record| match record {
            Record::Incumbent { objective, .. } => Some(objective.clone()),
            _ => None,
        });
        let claim = certificate.records.iter().find_map(|record| match record {
            Record::Claim { objective, .. } => objective.clone(),
            _ => None,
        });
        assert_eq!(objective, claim);
        // And the bound recomputes to the objective vector, which is what the
        // verifier will check.
        let instance = model(OPTIMAL);
        let terms = certificate
            .records
            .iter()
            .find_map(|record| match record {
                Record::DualBound { terms, .. } => Some(terms.clone()),
                _ => None,
            })
            .expect("a bound");
        let (coefficients, right) = lp::recompute(&instance, &terms);
        let expected: Vec<Rational> = instance
            .variables()
            .iter()
            .map(|v| v.objective.clone())
            .collect();
        assert_eq!(coefficients, expected);
        assert_eq!(
            right.add(instance.objective_constant()),
            claim.expect("an objective")
        );
    }

    #[test]
    fn an_infeasible_solve_produces_a_contradiction_that_sums_to_zero() {
        let source = concat!(
            "ROWS\n N obj\n G a\n L b\n",
            "COLUMNS\n x obj 1 a 2\n x b 1\n",
            "RHS\n rhs a 6 b 2\n",
            "ENDATA\n"
        );
        let certificate = certify(source).expect("a certificate");
        assert_eq!(certificate.claim, ClaimKind::Infeasible);
        let codes: Vec<u8> = certificate.records.iter().map(Record::code).collect();
        assert_eq!(
            codes,
            vec![3, 17, 96, 127],
            "root, contradiction, claim, end"
        );

        let instance = model(source);
        let terms = certificate
            .records
            .iter()
            .find_map(|record| match record {
                Record::Contradiction { terms, .. } => Some(terms.clone()),
                _ => None,
            })
            .expect("a contradiction");
        let (coefficients, right) = lp::recompute(&instance, &terms);
        assert!(
            coefficients.iter().all(Rational::is_zero),
            "{coefficients:?}"
        );
        assert!(right.is_positive(), "the contradiction sums to {right}");
        assert!(terms.iter().all(|(_, value)| value.is_positive()));
    }

    #[test]
    fn an_unbounded_solve_produces_a_ray_with_no_root_or_tree() {
        let source = concat!(
            "ROWS\n N obj\n L c1\n",
            "COLUMNS\n x obj -1\n y obj 1 c1 1\n",
            "RHS\n r c1 4\n",
            "ENDATA\n"
        );
        let certificate = certify(source).expect("a certificate");
        assert_eq!(certificate.claim, ClaimKind::Unbounded);
        let codes: Vec<u8> = certificate.records.iter().map(Record::code).collect();
        assert_eq!(codes, vec![82, 96, 127], "ray, claim, end");
        // Transform, root, and tree records are forbidden in this form.
        assert!(!codes.contains(&3));

        let instance = model(source);
        let direction = certificate
            .records
            .iter()
            .find_map(|record| match record {
                Record::Ray { direction, .. } => Some(direction.clone()),
                _ => None,
            })
            .expect("a ray");
        let rate = instance
            .variables()
            .iter()
            .zip(&direction)
            .fold(Rational::zero(), |total, (variable, step)| {
                total.add(&variable.objective.mul(step))
            });
        assert!(
            rate.is_negative(),
            "the direction changes the objective by {rate}"
        );
    }

    #[test]
    fn the_model_hash_is_the_digest_of_the_dump_and_not_the_instance_digest() {
        let instance = model(OPTIMAL);
        let hash = model_hash(&instance);
        let mut state = Sha256::new();
        state.update(instance.canonical_dump().as_bytes());
        assert_eq!(hash, state.finish());
        // Two different questions, two different answers.
        assert_ne!(&hash, instance.digest());
    }

    #[test]
    fn the_relative_gap_follows_the_stated_formula() {
        let two = Rational::from_i64(2);
        let ten = Rational::from_i64(10);
        // A denominator of one when the objective is small.
        assert_eq!(
            relative_gap(&Rational::one(), &Rational::zero()).to_canonical_string(),
            "1"
        );
        // And the objective's magnitude when it is not.
        assert_eq!(relative_gap(&ten, &two).to_canonical_string(), "4/5");
        // A closed gap is exactly zero.
        assert!(relative_gap(&ten, &ten).is_zero());
    }

    #[test]
    fn a_search_that_concluded_nothing_produces_no_certificate() {
        let model = model(OPTIMAL);
        let working = Working::from_model(&model);
        let problem = Problem::unscaled(&working);
        let outcome = Outcome::Numerical {
            basis: Basis::logical(&problem),
            note: "for the test".to_owned(),
        };
        assert_eq!(
            assemble_lp(&model, OPTIMAL.as_bytes(), &outcome),
            Err(NoProof::NothingToCertify)
        );
    }

    #[test]
    fn a_well_formed_basis_is_recognised_and_a_malformed_one_is_not() {
        let model = model(OPTIMAL);
        let working = Working::from_model(&model);
        let problem = Problem::unscaled(&working);
        let basis = Basis::logical(&problem);
        assert!(basis_is_well_formed(&basis, problem.rows()));
        assert!(!basis_is_well_formed(&basis, problem.rows() + 1));
    }
}
