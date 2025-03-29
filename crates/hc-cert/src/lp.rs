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
use hc_lp::{Basis, Status};

use crate::canonical::Canonical;
use crate::exact::{Failure, System};

/// Why an exact claim could not be assembled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gap {
    /// The basis is singular in exact arithmetic.
    ///
    /// A basis can factor comfortably in floating point and be singular
    /// exactly. That is not a defect in the search; it is the reason the
    /// rebuild happens at all.
    Singular(Failure),
    /// A multiplier needs a bound the model does not have.
    ///
    /// The inference would have to cite an inequality that does not exist, so
    /// there is no bound of this shape to be had.
    NoSuchBound {
        /// The canonical inequality that would have been needed.
        wanted: Wanted,
    },
    /// A column sits outside its bounds, so the point is not feasible.
    Infeasible {
        /// The column at fault.
        column: usize,
    },
}

/// A canonical inequality an inference wanted and the model does not have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wanted {
    /// A row's side.
    Row(RowId, Side),
    /// A variable's bound.
    Var(VarId, Side),
}

impl core::fmt::Display for Gap {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Singular(inner) => write!(formatter, "{inner}"),
            Self::NoSuchBound { wanted } => match wanted {
                Wanted::Row(row, side) => {
                    write!(formatter, "row {row} has no finite {side} side to cite")
                }
                Wanted::Var(variable, side) => {
                    write!(
                        formatter,
                        "variable {variable} has no finite {side} bound to cite"
                    )
                }
            },
            Self::Infeasible { column } => {
                write!(formatter, "column {column} sits outside its bounds")
            }
        }
    }
}

impl core::error::Error for Gap {}

/// Nonnegative multipliers over canonical inequalities, and what they prove.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inference {
    /// The cited inequalities and their multipliers, in identifier order.
    ///
    /// Every multiplier is strictly positive. A zero multiplier contributes
    /// nothing and would give one inference two encodings, so it is never
    /// stored.
    pub terms: Vec<(IneqId, Rational)>,
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

/// The exact point a basis implies.
///
/// The nonbasic columns take their resting values and the basic ones are
/// whatever solves the system — computed here in rationals, so the answer is
/// the vertex rather than a rounding of it.
///
/// # Errors
///
/// Returns [`Gap::Singular`] when the basis is singular in exact arithmetic.
pub fn exact_point(
    canonical: &Canonical,
    basis: &Basis,
    resting: &dyn Fn(usize) -> Option<Rational>,
) -> Result<Vec<Rational>, Gap> {
    let mut point = vec![Rational::zero(); canonical.columns()];
    let mut target = vec![Rational::zero(); canonical.rows()];

    for (column, slot) in point.iter_mut().enumerate() {
        if basis.status(column).is_basic() {
            continue;
        }
        let value = resting(column).unwrap_or_else(Rational::zero);
        *slot = value.clone();
        if value.is_zero() {
            continue;
        }
        for (row, coefficient) in canonical.column(column) {
            let slot = &mut target[*row as usize];
            *slot = slot.sub(&coefficient.mul(&value));
        }
    }

    let columns: Vec<Vec<(u32, Rational)>> = basis
        .basic()
        .iter()
        .map(|column| canonical.column(*column).to_vec())
        .collect();
    let system = System::from_columns(canonical.rows(), &columns).map_err(Gap::Singular)?;
    let basic = system.solve(&target).map_err(Gap::Singular)?;
    for (position, column) in basis.basic().iter().enumerate() {
        point[*column] = basic[position].clone();
    }
    Ok(point)
}

/// Where a nonbasic column rests, exactly, according to its status.
#[must_use]
pub fn resting_value(canonical: &Canonical, basis: &Basis, column: usize) -> Option<Rational> {
    match basis.status(column) {
        Status::Basic => None,
        Status::AtLower | Status::Fixed => canonical.lower(column).cloned(),
        Status::AtUpper => canonical.upper(column).cloned(),
        Status::Free => Some(Rational::zero()),
    }
}

/// The exact row multipliers a basis implies for a cost vector.
///
/// The cost vector is over every column, structural and logical alike, because
/// the first phase's objective lives on the logical columns and the second
/// phase's lives on the structural ones.
///
/// # Errors
///
/// Returns [`Gap::Singular`] when the basis is singular in exact arithmetic.
pub fn exact_multipliers(
    canonical: &Canonical,
    basis: &Basis,
    costs: &[Rational],
) -> Result<Vec<Rational>, Gap> {
    let columns: Vec<Vec<(u32, Rational)>> = basis
        .basic()
        .iter()
        .map(|column| canonical.column(*column).to_vec())
        .collect();
    let system = System::from_columns(canonical.rows(), &columns).map_err(Gap::Singular)?;
    let basic: Vec<Rational> = basis
        .basic()
        .iter()
        .map(|column| costs[*column].clone())
        .collect();
    system.transposed().solve(&basic).map_err(Gap::Singular)
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
    let mut terms: Vec<(IneqId, Rational)> = Vec::new();
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
        let Some(id) = axioms.row(row, side) else {
            return Err(Gap::NoSuchBound {
                wanted: Wanted::Row(row, side),
            });
        };
        let bound = match side {
            Side::Lower => model.rows()[index].lower.clone(),
            Side::Upper => model.rows()[index].upper.clone(),
        }
        .ok_or(Gap::NoSuchBound {
            wanted: Wanted::Row(row, side),
        })?;
        terms.push((id, weight.abs()));
        value = value.add(&weight.mul(&bound));
    }

    // What the rows do not account for is made up by the variable bounds, one
    // per variable, with the same sign convention.
    let by_row = model.matrix().by_row(model.rows().len());
    let mut accounted = vec![Rational::zero(); model.variables().len()];
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
        let Some(id) = axioms.var(variable, side) else {
            return Err(Gap::NoSuchBound {
                wanted: Wanted::Var(variable, side),
            });
        };
        let bound = match side {
            Side::Lower => model.variables()[index].lower.clone(),
            Side::Upper => model.variables()[index].upper.clone(),
        }
        .ok_or(Gap::NoSuchBound {
            wanted: Wanted::Var(variable, side),
        })?;
        terms.push((id, residual.abs()));
        value = value.add(&residual.mul(&bound));
    }

    // Identifier order, which is the order the format requires and which makes
    // one inference have one encoding.
    terms.sort_by_key(|(id, _)| id.0);
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
pub fn recompute(model: &Model, terms: &[(IneqId, Rational)]) -> (Vec<Rational>, Rational) {
    let mut coefficients = vec![Rational::zero(); model.variables().len()];
    let mut right = Rational::zero();
    let by_row = model.matrix().by_row(model.rows().len());

    for (id, weight) in terms {
        let Some(inequality) = model.inequality(*id) else {
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
                pair[0].0.0 < pair[1].0.0,
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
