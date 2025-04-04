//! Rebuilding a linear program's claim, exactly, from a basis.
//!
//! Everything here is a construction over the *canonical inequalities* — the
//! numbered axioms of the original model, in which a variable bound is an
//! inequality exactly like a row side. That uniformity is what makes a
//! contradiction complete: there is no separate convention for bounds and
//! therefore no bound a reader has to know to supply.
//!
//! One construction does two jobs. Given multipliers on the rows and a target
//! objective vector, it produces nonnegative multipliers over the canonical
//! inequalities whose coefficients sum to that target, together with the exact
//! bound they establish. Aim it at the objective and it produces a lower bound
//! on the objective. Aim it at the zero vector and a positive bound is a
//! contradiction — zero cannot be at least something positive — which is the
//! entire Farkas argument, in the same code, with no second implementation to
//! keep in step.
//!
//! Every failure path is a real answer rather than an error. A multiplier that
//! needs a bound the model does not have means no bound can be built this way,
//! and saying so is how a solve degrades to a weaker true statement instead of
//! asserting the stronger one anyway.

use hc_core::{IneqId, IneqSource, Model, RowId, Side, VarId};
use hc_exact::Rational;
pub use hc_lp::exact::{Gap, Wanted};

/// Nonnegative multipliers over canonical inequalities, and what they prove.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inference {
    /// The cited inequalities and their multipliers, in identifier order.
    ///
    /// Identifiers are plain integers rather than the model's own type,
    /// because a proof made at a search node may cite a branch assumption,
    /// which is a canonical inequality with an identifier above the model's
    /// base block and no counterpart in the model at all.
    ///
    /// Every multiplier is strictly positive. A zero multiplier contributes
    /// nothing and would give one inference two encodings, so it is never
    /// stored.
    pub terms: Vec<(u64, Rational)>,
    /// The value the multiplied right-hand sides sum to, plus the objective
    /// constant when the target was the objective.
    pub value: Rational,
}

impl Inference {
    /// Whether the inference cites nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

/// A reverse index from a model's inequalities back to their identifiers.
///
/// The forward direction is the specification's; this is the direction an
/// assembler needs, and building it here keeps the ordering rule stated in
/// exactly one place — the model — rather than reimplemented wherever a
/// multiplier has to be attached to an axiom.
#[derive(Clone, Debug)]
pub struct Axioms {
    row_lower: Vec<Option<IneqId>>,
    row_upper: Vec<Option<IneqId>>,
    var_lower: Vec<Option<IneqId>>,
    var_upper: Vec<Option<IneqId>>,
}

impl Axioms {
    /// Indexes a model's canonical inequalities.
    #[must_use]
    pub fn of(model: &Model) -> Self {
        let mut index = Self {
            row_lower: vec![None; model.rows().len()],
            row_upper: vec![None; model.rows().len()],
            var_lower: vec![None; model.variables().len()],
            var_upper: vec![None; model.variables().len()],
        };
        for (position, inequality) in model.inequalities().iter().enumerate() {
            let id = IneqId(position as u32);
            match (inequality.source, inequality.side) {
                (IneqSource::Row(row), Side::Lower) => index.row_lower[row.index()] = Some(id),
                (IneqSource::Row(row), Side::Upper) => index.row_upper[row.index()] = Some(id),
                (IneqSource::Var(variable), Side::Lower) => {
                    index.var_lower[variable.index()] = Some(id);
                }
                (IneqSource::Var(variable), Side::Upper) => {
                    index.var_upper[variable.index()] = Some(id);
                }
            }
        }
        index
    }

    /// The identifier of a row's side, when the model has it.
    #[must_use]
    pub fn row(&self, row: RowId, side: Side) -> Option<IneqId> {
        match side {
            Side::Lower => self.row_lower[row.index()],
            Side::Upper => self.row_upper[row.index()],
        }
    }

    /// The identifier of a variable's bound, when the model has it.
    #[must_use]
    pub fn var(&self, variable: VarId, side: Side) -> Option<IneqId> {
        match side {
            Side::Lower => self.var_lower[variable.index()],
            Side::Upper => self.var_upper[variable.index()],
        }
    }
}

/// Builds nonnegative multipliers over the canonical inequalities whose
/// coefficients sum to a target vector, and the bound they establish.
///
/// `multipliers` are over rows. `target` is the coefficient vector the sum
/// must equal, over the model's own variables — the objective for a bound, the
/// zero vector for a contradiction. `constant` is added to the resulting value
/// and is the objective constant for a bound and zero for a contradiction.
///
/// # Errors
///
/// Returns [`Gap::NoSuchBound`] when a nonzero multiplier would have to cite an
/// inequality the model does not have.
pub fn assemble(
    model: &Model,
    axioms: &Axioms,
    multipliers: &[Rational],
    target: &[Rational],
    constant: &Rational,
) -> Result<Inference, Gap> {
    assemble_with(
        &ModelBounds { model, axioms },
        model.rows().len(),
        model.variables().len(),
        model.matrix(),
        multipliers,
        target,
        constant,
    )
}

/// One inequality a bound supplier offers: its identifier and its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Supply {
    /// The canonical inequality's identifier.
    pub id: u64,
    /// The right-hand side, as written in the model or the assumption.
    pub bound: Rational,
}

/// Where the bounds an inference may cite come from.
///
/// At the root this is the model. At a search node it is the model overlaid
/// with the branch assumptions on the path, because a node's effective bound on
/// a branched variable is the assumption rather than anything the file said —
/// and citing the file's bound there would be citing a weaker inequality than
/// the one the arithmetic actually used.
pub trait Bounds {
    /// The inequality supplying a row's side, when there is one.
    fn row(&self, row: RowId, side: Side) -> Option<Supply>;
    /// The inequality supplying a variable's bound, when there is one.
    fn var(&self, variable: VarId, side: Side) -> Option<Supply>;
}

/// The model's own bounds, which is what a root inference cites.
pub struct ModelBounds<'a> {
    /// The model.
    pub model: &'a Model,
    /// Its inequality index.
    pub axioms: &'a Axioms,
}

impl Bounds for ModelBounds<'_> {
    fn row(&self, row: RowId, side: Side) -> Option<Supply> {
        let id = self.axioms.row(row, side)?;
        let bound = match side {
            Side::Lower => self.model.rows()[row.index()].lower.clone(),
            Side::Upper => self.model.rows()[row.index()].upper.clone(),
        }?;
        Some(Supply {
            id: u64::from(id.0),
            bound,
        })
    }

    fn var(&self, variable: VarId, side: Side) -> Option<Supply> {
        let id = self.axioms.var(variable, side)?;
        let bound = match side {
            Side::Lower => self.model.variables()[variable.index()].lower.clone(),
            Side::Upper => self.model.variables()[variable.index()].upper.clone(),
        }?;
        Some(Supply {
            id: u64::from(id.0),
            bound,
        })
    }
}

/// The general form: the same construction over any bound supplier.
///
/// # Errors
///
/// Returns [`Gap::NoSuchBound`] when a nonzero multiplier would have to cite an
/// inequality the supplier does not offer.
pub fn assemble_with(
    bounds: &dyn Bounds,
    rows: usize,
    variables: usize,
    matrix: &hc_core::Matrix,
    multipliers: &[Rational],
    target: &[Rational],
    constant: &Rational,
) -> Result<Inference, Gap> {
    let mut terms: Vec<(u64, Rational)> = Vec::new();
    let mut value = constant.clone();

    // The row multipliers. A positive multiplier cites the row's lower side as
    // written; a negative one cites the negated upper side, with the sign
    // moved onto the axiom rather than onto the multiplier.
    for (index, weight) in multipliers.iter().enumerate() {
        if weight.is_zero() {
            continue;
        }
        let row = RowId(index as u32);
        let side = if weight.is_positive() {
            Side::Lower
        } else {
            Side::Upper
        };
        let Some(supply) = bounds.row(row, side) else {
            return Err(Gap::NoSuchBound {
                wanted: Wanted::Row(row, side),
            });
        };
        terms.push((supply.id, weight.abs()));
        value = value.add(&weight.mul(&supply.bound));
    }

    // What the rows do not account for is made up by the variable bounds, one
    // per variable, with the same sign convention.
    let by_row = matrix.by_row(rows);
    let mut accounted = vec![Rational::zero(); variables];
    for (index, weight) in multipliers.iter().enumerate() {
        if weight.is_zero() {
            continue;
        }
        for (variable, coefficient) in by_row.row(RowId(index as u32)) {
            let slot = &mut accounted[variable.index()];
            *slot = slot.add(&weight.mul(coefficient));
        }
    }

    for (index, wanted) in target.iter().enumerate() {
        let residual = wanted.sub(&accounted[index]);
        if residual.is_zero() {
            continue;
        }
        let variable = VarId(index as u32);
        let side = if residual.is_positive() {
            Side::Lower
        } else {
            Side::Upper
        };
        let Some(supply) = bounds.var(variable, side) else {
            return Err(Gap::NoSuchBound {
                wanted: Wanted::Var(variable, side),
            });
        };
        terms.push((supply.id, residual.abs()));
        value = value.add(&residual.mul(&supply.bound));
    }

    // Identifier order, which is the order the format requires and which makes
    // one inference have one encoding.
    terms.sort_by_key(|(id, _)| *id);
    Ok(Inference { terms, value })
}

/// Checks an assembled inference against the model, from scratch.
///
/// This is the same arithmetic the verifier will perform, done on this side
/// before anything is written out. It is not a substitute for the independent
/// check — it shares this crate's arithmetic and this crate's model — but a
/// certificate that fails here would fail there, and finding out now costs
/// nothing.
///
/// Returns the coefficient vector the terms sum to, so a caller can compare it
/// against whatever it expected.
#[must_use]
pub fn recompute(model: &Model, terms: &[(u64, Rational)]) -> (Vec<Rational>, Rational) {
    let mut coefficients = vec![Rational::zero(); model.variables().len()];
    let mut right = Rational::zero();
    let by_row = model.matrix().by_row(model.rows().len());

    for (id, weight) in terms {
        // Identifiers above the model's base block name branch assumptions and
        // cuts, which this recomputation does not know about. A caller
        // checking a node inference uses the node-aware form; this one exists
        // for the root, where every citation is the model's own.
        let Ok(narrow) = u32::try_from(*id) else {
            continue;
        };
        let Some(inequality) = model.inequality(IneqId(narrow)) else {
            continue;
        };
        let negate = inequality.side == Side::Upper;
        match inequality.source {
            IneqSource::Row(row) => {
                for (variable, coefficient) in by_row.row(row) {
                    let signed = if negate {
                        coefficient.neg()
                    } else {
                        coefficient.clone()
                    };
                    let slot = &mut coefficients[variable.index()];
                    *slot = slot.add(&weight.mul(&signed));
                }
                let bound = match inequality.side {
                    Side::Lower => model.rows()[row.index()].lower.clone(),
                    Side::Upper => model.rows()[row.index()].upper.clone(),
                };
                if let Some(bound) = bound {
                    let signed = if negate { bound.neg() } else { bound };
                    right = right.add(&weight.mul(&signed));
                }
            }
            IneqSource::Var(variable) => {
                let unit = if negate {
                    Rational::from_i64(-1)
                } else {
                    Rational::one()
                };
                let slot = &mut coefficients[variable.index()];
                *slot = slot.add(&weight.mul(&unit));
                let bound = match inequality.side {
                    Side::Lower => model.variables()[variable.index()].lower.clone(),
                    Side::Upper => model.variables()[variable.index()].upper.clone(),
                };
                if let Some(bound) = bound {
                    let signed = if negate { bound.neg() } else { bound };
                    right = right.add(&weight.mul(&signed));
                }
            }
        }
    }
    (coefficients, right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_core::Tol;
    use hc_lp::exact::{Canonical, exact_multipliers, exact_point, resting_value};
    use hc_lp::{Basis, Status};
    use hc_lp::{Options, Outcome, Problem, Working};

    fn model(source: &str) -> Model {
        hc_io::mps::read(source.as_bytes(), hc_io::mps::Form::Free).expect("accepted")
    }

    /// Solves a model and rebuilds everything its basis implies, exactly.
    fn rebuild(model: &Model) -> (Canonical, Basis, Vec<Rational>) {
        let working = Working::from_model(model);
        let problem = Problem::unscaled(&working);
        let (outcome, _) = hc_lp::solve(&problem, &Options::default());
        let basis = match outcome {
            Outcome::Optimal { basis, .. } => basis,
            other => panic!("expected an optimum, got {}", other.as_str()),
        };
        let canonical = Canonical::of(model);
        let point = exact_point(&canonical, &basis, &|column| {
            resting_value(&canonical, &basis, column)
        })
        .expect("the basis is nonsingular");
        (canonical, basis, point)
    }

    #[test]
    fn the_rebuilt_point_is_the_vertex_and_not_a_rounding() {
        // The optimum here is one third, which has no finite binary
        // expansion. A rebuild that went through the search's own numbers
        // would produce something very close to it and not equal to it.
        let instance = model(concat!(
            "ROWS\n N obj\n E r\n",
            "COLUMNS\n x obj -1 r 3\n",
            "RHS\n rhs r 1\n",
            "ENDATA\n"
        ));
        let (canonical, _, point) = rebuild(&instance);
        assert_eq!(point[0].to_canonical_string(), "1/3");
        assert!(canonical.residual_at(&point).iter().all(Rational::is_zero));
        assert_eq!(canonical.objective_at(&point).to_canonical_string(), "-1/3");
    }

    #[test]
    fn the_rebuilt_point_satisfies_the_system_and_the_bounds_exactly() {
        let instance = model(concat!(
            "ROWS\n N obj\n L r1\n G r2\n E r3\n",
            "COLUMNS\n",
            " x obj -3 r1 7\n x r2 2 r3 1\n",
            " y obj -5 r1 3\n y r2 5 r3 1\n",
            "RHS\n rhs r1 22 r2 4\n rhs r3 3\n",
            "BOUNDS\n UP b x 9\n UP b y 9\n",
            "ENDATA\n"
        ));
        let (canonical, _, point) = rebuild(&instance);
        assert!(canonical.residual_at(&point).iter().all(Rational::is_zero));
        assert!(
            canonical.violations(&point).is_empty(),
            "{:?}",
            canonical.violations(&point)
        );
    }

    #[test]
    fn an_optimal_basis_yields_a_bound_that_equals_its_objective() {
        // The two numbers meeting exactly is what optimality means, and it is
        // the check that would catch a sign error anywhere in the assembly.
        let instance = model(concat!(
            "ROWS\n N obj\n L r1\n L r2\n",
            "COLUMNS\n x obj -3 r1 1\n x r2 1\n y obj -2 r1 1\n y r2 3\n",
            "RHS\n rhs r1 4 r2 6\n",
            "ENDATA\n"
        ));
        let (canonical, basis, point) = rebuild(&instance);
        let costs: Vec<Rational> = (0..canonical.columns())
            .map(|index| canonical.cost(index).clone())
            .collect();
        let multipliers =
            exact_multipliers(&canonical, &basis, &costs).expect("the basis is nonsingular");

        let axioms = Axioms::of(&instance);
        let objective: Vec<Rational> = instance
            .variables()
            .iter()
            .map(|variable| variable.objective.clone())
            .collect();
        let inference = assemble(
            &instance,
            &axioms,
            &multipliers,
            &objective,
            instance.objective_constant(),
        )
        .expect("every cited bound exists");

        assert_eq!(inference.value, canonical.objective_at(&point));
        assert!(!inference.is_empty());
        assert!(
            inference
                .terms
                .iter()
                .all(|(_, weight)| weight.is_positive())
        );

        // And the terms recompute to the objective vector, coefficient for
        // coefficient, which is the check the verifier will perform.
        let (coefficients, right) = recompute(&instance, &inference.terms);
        assert_eq!(coefficients, objective);
        assert_eq!(right.add(instance.objective_constant()), inference.value);
    }

    #[test]
    fn the_terms_are_in_identifier_order_and_carry_no_zero() {
        let instance = model(concat!(
            "ROWS\n N obj\n L r1\n G r2\n",
            "COLUMNS\n x obj 2 r1 1\n x r2 1\n y obj 3 r1 1\n y r2 1\n",
            "RHS\n rhs r1 8 r2 2\n",
            "BOUNDS\n UP b x 5\n UP b y 5\n",
            "ENDATA\n"
        ));
        let (canonical, basis, _) = rebuild(&instance);
        let costs: Vec<Rational> = (0..canonical.columns())
            .map(|index| canonical.cost(index).clone())
            .collect();
        let multipliers = exact_multipliers(&canonical, &basis, &costs).expect("nonsingular");
        let axioms = Axioms::of(&instance);
        let objective: Vec<Rational> = instance
            .variables()
            .iter()
            .map(|v| v.objective.clone())
            .collect();
        let inference = assemble(
            &instance,
            &axioms,
            &multipliers,
            &objective,
            instance.objective_constant(),
        )
        .expect("every cited bound exists");

        for pair in inference.terms.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "the terms are not in identifier order"
            );
        }
        assert!(inference.terms.iter().all(|(_, weight)| !weight.is_zero()));
    }

    #[test]
    fn a_multiplier_with_no_bound_to_cite_is_reported_rather_than_invented() {
        // A row with no lower side cannot support a positive multiplier, and
        // the honest answer is that there is no bound of this shape — not a
        // bound built from an axiom the model does not contain.
        let instance = model(concat!(
            "ROWS\n N obj\n L r1\n",
            "COLUMNS\n x obj 1 r1 1\n",
            "RHS\n rhs r1 4\n",
            "BOUNDS\n FR b x\n",
            "ENDATA\n"
        ));
        let axioms = Axioms::of(&instance);
        let objective: Vec<Rational> = instance
            .variables()
            .iter()
            .map(|v| v.objective.clone())
            .collect();
        // A positive multiplier on a row that has only an upper side.
        let failure = assemble(
            &instance,
            &axioms,
            &[Rational::one()],
            &objective,
            instance.objective_constant(),
        )
        .expect_err("there is no lower side to cite");
        assert_eq!(
            failure,
            Gap::NoSuchBound {
                wanted: Wanted::Row(RowId(0), Side::Lower)
            }
        );
        assert!(failure.to_string().contains("no finite"));
    }

    #[test]
    fn a_zero_target_turns_the_same_construction_into_a_contradiction() {
        // Two rows that cannot both hold: 2x >= 6 and x <= 2 with x >= 0. The
        // multipliers one half on the first and one on the second sum to zero
        // coefficients and to a strictly positive right-hand side.
        let instance = model(concat!(
            "ROWS\n N obj\n G a\n L b\n",
            "COLUMNS\n x obj 1 a 2\n x b 1\n",
            "RHS\n rhs a 6 b 2\n",
            "ENDATA\n"
        ));
        let axioms = Axioms::of(&instance);
        let zero = vec![Rational::zero(); instance.variables().len()];
        let half =
            Rational::new(hc_exact::BigInt::one(), hc_exact::BigInt::from_i64(2)).expect("nonzero");
        let inference = assemble(
            &instance,
            &axioms,
            &[half, Rational::from_i64(-1)],
            &zero,
            &Rational::zero(),
        )
        .expect("both sides exist");

        let (coefficients, right) = recompute(&instance, &inference.terms);
        assert!(
            coefficients.iter().all(Rational::is_zero),
            "{coefficients:?}"
        );
        assert!(right.is_positive(), "the contradiction's value is {right}");
        assert_eq!(right, inference.value);
        assert_eq!(inference.value.to_canonical_string(), "1");
    }

    #[test]
    fn the_axiom_index_agrees_with_the_models_own_ordering() {
        let instance = model(concat!(
            "ROWS\n N obj\n L r1\n E r2\n",
            "COLUMNS\n x obj 1 r1 1\n x r2 1\n",
            "RHS\n rhs r1 4 r2 2\n",
            "BOUNDS\n UP b x 3\n",
            "ENDATA\n"
        ));
        let axioms = Axioms::of(&instance);
        for (position, inequality) in instance.inequalities().iter().enumerate() {
            let id = IneqId(position as u32);
            let found = match inequality.source {
                IneqSource::Row(row) => axioms.row(row, inequality.side),
                IneqSource::Var(variable) => axioms.var(variable, inequality.side),
            };
            assert_eq!(found, Some(id));
        }
        // A side the model does not have is absent rather than invented.
        assert_eq!(axioms.row(RowId(0), Side::Lower), None);
    }

    #[test]
    fn an_exactly_singular_basis_is_reported_rather_than_solved() {
        // A basis of two identical columns. The float search would never
        // choose it; the point is that the rebuild says so rather than
        // producing numbers.
        let instance = model(concat!(
            "ROWS\n N obj\n E r1\n E r2\n",
            "COLUMNS\n x obj 1 r1 1\n x r2 1\n y obj 1 r1 1\n y r2 1\n",
            "RHS\n rhs r1 1 r2 1\n",
            "ENDATA\n"
        ));
        let canonical = Canonical::of(&instance);
        let working = Working::from_model(&instance);
        let problem = Problem::unscaled(&working);
        let mut basis = Basis::logical(&problem);
        basis.pivot(0, 0, Status::AtLower);
        basis.pivot(1, 1, Status::AtLower);
        let costs: Vec<Rational> = (0..canonical.columns())
            .map(|index| canonical.cost(index).clone())
            .collect();
        assert!(matches!(
            exact_multipliers(&canonical, &basis, &costs),
            Err(Gap::Singular(_))
        ));
        let _ = Tol::solver();
    }
}
