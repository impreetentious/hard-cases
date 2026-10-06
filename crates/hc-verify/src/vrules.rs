//! The two linear inference rules.
//!
//! Both are the same construction pointed at two different targets. Take
//! nonnegative multipliers over inequalities written `a . x >= b`, sum the
//! multiplied coefficients, and sum the multiplied right-hand sides. If the
//! coefficients land on the objective vector, the right-hand side sum is a
//! lower bound on the objective. If they land on the zero vector and the
//! right-hand side sum is positive, then zero is at least something positive
//! and the model has no feasible point.
//!
//! That is the entire calculus for a linear program. There is no rule that
//! rounds, no rule that trusts a basis, and no rule that accepts a number
//! because a solver computed it. A basis never appears here at all: it is a
//! guess about which constraints are tight, it belongs to the search, and the
//! multipliers it suggests are checked as multipliers rather than believed as
//! a basis.
//!
//! Every refusal names the earliest rule that could have caught the defect,
//! and the order the checks run in is part of the specification rather than an
//! implementation detail. A negative multiplier is caught before the sum is
//! formed, because after the sum it is indistinguishable from arithmetic that
//! merely came out wrong.

use crate::vaxiom::{Axioms, Sum};
use crate::vrat::Frac;
use crate::vreject::{Code, Rejected};
use crate::vwork::Work;

/// What a checked combination establishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    /// The value the combination proves the objective is at least.
    ///
    /// Recomputed here from the multipliers and the model. Nothing in a
    /// certificate states it, precisely so that nothing in a certificate can
    /// state it wrongly.
    pub value: Frac,
}

/// Checks the multipliers themselves, before any of them is used.
///
/// Both rules share this, and both need it to happen first. A negative
/// multiplier reverses the inequality it is applied to, which turns a sound
/// rule into an arbitrary one; a zero multiplier contributes nothing and gives
/// one inference two encodings.
fn check_multipliers(terms: &[(u64, Frac)]) -> Result<(), Rejected> {
    for (id, multiplier) in terms {
        if multiplier.is_minus() {
            return Err(Rejected::new(
                Code::DUAL_NEGATIVE,
                format!("inequality {id} carries the multiplier {multiplier}"),
            ));
        }
    }
    for (id, multiplier) in terms {
        if multiplier.is_zero() {
            return Err(Rejected::new(
                Code::DUAL_ZERO,
                format!("inequality {id} carries a zero multiplier"),
            ));
        }
    }
    Ok(())
}

/// Sums the multiplied coefficients and the multiplied right-hand sides.
///
/// `visible` decides which identifiers this node may cite. At the root of a
/// linear program every inequality is visible, and inside a tree the scope
/// rules narrow it; the rule itself does not know the difference, which is why
/// the narrowing cannot be forgotten in one of the two callers.
fn combine(
    work: &mut Work,
    axioms: &Axioms<'_>,
    terms: &[(u64, Frac)],
    visible: &dyn Fn(u64) -> bool,
) -> Result<(Sum, Frac), Rejected> {
    let mut sum = Sum::new();
    let mut right = Frac::zero();
    for (id, multiplier) in terms {
        let Some(inequality) = axioms.get(*id) else {
            return Err(Rejected::new(
                Code::DUAL_NO_SUCH_INEQUALITY,
                format!("inequality {id} does not exist in this model"),
            ));
        };
        if !visible(*id) {
            return Err(Rejected::new(
                Code::SCOPE_NOT_VISIBLE,
                format!("inequality {id} is not visible here"),
            ));
        }
        sum.add(work, multiplier, &inequality);
        let scaled = work.multiply(&inequality.right, multiplier);
        right = work.add(&right, &scaled);
    }
    Ok((sum, right))
}

/// Every inequality is visible. The linear-program root uses this.
#[must_use]
pub fn everything(_: u64) -> bool {
    true
}

/// Checks a lower bound on the objective.
///
/// # Errors
///
/// Returns [`Rejected`] naming the earliest rule that refused.
pub fn dual_bound(
    work: &mut Work,
    axioms: &Axioms<'_>,
    terms: &[(u64, Frac)],
    visible: &dyn Fn(u64) -> bool,
) -> Result<Bound, Rejected> {
    check_multipliers(terms)?;
    let (sum, right) = combine(work, axioms, terms, visible)?;

    let objective = axioms.objective();
    if let Some(column) = sum.first_difference(&objective) {
        return Err(Rejected::new(
            Code::DUAL_COEFFICIENTS,
            format!("the multiplied coefficients differ from the objective at column {column}"),
        ));
    }

    // The objective constant is part of the bound and part of the model. It is
    // added here rather than anywhere else, because a bound that forgot it
    // would still be a bound on something and would be a bound on the wrong
    // thing.
    Ok(Bound {
        value: work.add(&axioms.instance().constant, &right),
    })
}

/// Checks a contradiction.
///
/// # Errors
///
/// Returns [`Rejected`] naming the earliest rule that refused.
pub fn contradiction(
    work: &mut Work,
    axioms: &Axioms<'_>,
    terms: &[(u64, Frac)],
    visible: &dyn Fn(u64) -> bool,
) -> Result<(), Rejected> {
    check_multipliers(terms)?;
    let (sum, right) = combine(work, axioms, terms, visible)?;

    // The coefficient check comes first and the vectors pin that order: a
    // witness over rows alone, with the variable bounds left to convention,
    // fails to sum to zero long before its right-hand side is interesting.
    if !sum.is_zero() {
        let column = sum.terms().first().map_or(0, |(column, _)| *column);
        return Err(Rejected::new(
            Code::DUAL_NOT_ZERO_SUM,
            format!("the multiplied coefficients are not zero at column {column}"),
        ));
    }
    if !right.is_plus() {
        return Err(Rejected::new(
            Code::DUAL_NOT_POSITIVE,
            format!("the multiplied right-hand sides sum to {right}"),
        ));
    }
    Ok(())
}

/// Compares a bound a certificate states against the bound it establishes.
///
/// Nothing in the certificate's inference records carries a bound value: the
/// only place a bound is written down is where a claim states one, and this is
/// the check that meets it. Keeping the comparison here rather than inside the
/// closure means the rule that recomputes and the rule that compares are the
/// same two lines wherever a bound is asserted.
///
/// # Errors
///
/// Returns [`Rejected`] when the stated bound is not the one established.
pub fn bound_is(established: &Bound, stated: &Frac) -> Result<(), Rejected> {
    if established.value == *stated {
        return Ok(());
    }
    Err(Rejected::new(
        Code::DUAL_BOUND_VALUE,
        format!(
            "the combination establishes {} and the certificate states {stated}",
            established.value
        ),
    ))
}

/// Checks a point against every row, every finite bound, and integrality.
///
/// The three checks are separate rules because they fail for different
/// reasons and a report that said only "infeasible" would be useless to the
/// person holding the point.
///
/// # Errors
///
/// Returns [`Rejected`] naming the earliest rule that refused.
pub fn point_is_feasible(
    work: &mut Work,
    axioms: &Axioms<'_>,
    point: &[Frac],
) -> Result<(), Rejected> {
    let instance = axioms.instance();
    if point.len() != instance.columns.len() {
        return Err(Rejected::new(
            Code::POINT_DIMENSION,
            format!(
                "the point has {} components and the model has {} columns",
                point.len(),
                instance.columns.len()
            ),
        ));
    }

    for (index, row) in instance.rows.iter().enumerate() {
        let activity = instance
            .activity_at(index as u32, point)
            .ok_or_else(|| Rejected::new(Code::POINT_DIMENSION, "the point is the wrong length"))?;
        if let Some(low) = &row.low
            && work.compare(&activity, low) == core::cmp::Ordering::Less
        {
            return Err(Rejected::new(
                Code::POINT_ROW,
                format!(
                    "row {} has activity {activity} below its lower side {low}",
                    row.name
                ),
            ));
        }
        if let Some(high) = &row.high
            && work.compare(&activity, high) == core::cmp::Ordering::Greater
        {
            return Err(Rejected::new(
                Code::POINT_ROW,
                format!(
                    "row {} has activity {activity} above its upper side {high}",
                    row.name
                ),
            ));
        }
    }

    for (column, value) in instance.columns.iter().zip(point) {
        if let Some(low) = &column.low
            && work.compare(value, low) == core::cmp::Ordering::Less
        {
            return Err(Rejected::new(
                Code::POINT_BOUND,
                format!(
                    "column {} holds {value} below its lower bound {low}",
                    column.name
                ),
            ));
        }
        if let Some(high) = &column.high
            && work.compare(value, high) == core::cmp::Ordering::Greater
        {
            return Err(Rejected::new(
                Code::POINT_BOUND,
                format!(
                    "column {} holds {value} above its upper bound {high}",
                    column.name
                ),
            ));
        }
    }

    for (column, value) in instance.columns.iter().zip(point) {
        work.inspect(value);
        if column.whole && !value.is_whole() {
            return Err(Rejected::new(
                Code::POINT_INTEGRALITY,
                format!(
                    "column {} is declared integer and holds {value}",
                    column.name
                ),
            ));
        }
    }
    Ok(())
}

/// Recomputes a point's objective and compares it against the claimed value.
///
/// # Errors
///
/// Returns [`Rejected`] when the two differ.
pub fn objective_is(
    work: &mut Work,
    axioms: &Axioms<'_>,
    point: &[Frac],
    stated: &Frac,
) -> Result<Frac, Rejected> {
    let value = axioms
        .instance()
        .objective_at(point)
        .ok_or_else(|| Rejected::new(Code::POINT_DIMENSION, "the point is the wrong length"))?;
    work.spend(point.len() as u64);
    if value != *stated {
        return Err(Rejected::new(
            Code::POINT_OBJECTIVE,
            format!("the point attains {value} and the certificate states {stated}"),
        ));
    }
    Ok(value)
}

/// The exact relative gap between an upper and a lower value.
///
/// `(U - L) / max(1, |U|)`, with the denominator preventing an instance whose
/// optimum happens to sit near zero from reporting an enormous relative gap
/// for an absolutely tiny one.
#[must_use]
pub fn relative_gap(work: &mut Work, upper: &Frac, lower: &Frac) -> Frac {
    let span = work.subtract(upper, lower);
    let scale = upper.magnitude();
    let denominator = if work.compare(&scale, &Frac::one()) == core::cmp::Ordering::Greater {
        scale
    } else {
        Frac::one()
    };
    work.divide(&span, &denominator).unwrap_or_else(Frac::zero)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vint::Int;
    use crate::vmodel::{Cell, Column, Constraint, Direction, Instance, Tag, whole};

    fn ratio(top: i64, bottom: i64) -> Frac {
        Frac::ratio(&Int::from_i64(top), &Int::from_i64(bottom)).expect("a nonzero denominator")
    }

    /// Minimize -x subject to x <= 4, with x between 0 and 10 and integer.
    fn model_a() -> Instance {
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

    /// Minimize nothing subject to x <= 2 and x >= 3.
    fn model_b() -> Instance {
        Instance {
            columns: vec![Column {
                low: Some(whole(3)),
                high: None,
                whole: false,
                cost: Frac::zero(),
                name: "X".to_owned(),
            }],
            rows: vec![Constraint {
                low: None,
                high: Some(whole(2)),
                name: "C1".to_owned(),
            }],
            ..model_a()
        }
    }

    #[test]
    fn the_tight_bound_is_recomputed_as_the_vectors_state() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let bound = dual_bound(&mut Work::new(), &axioms, &[(0, Frac::one())], &everything)
            .expect("accepted");
        assert_eq!(bound.value.to_canonical(), "-4");
    }

    #[test]
    fn a_weaker_bound_is_accepted_because_the_calculus_is_about_truth() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let bound = dual_bound(&mut Work::new(), &axioms, &[(2, Frac::one())], &everything)
            .expect("accepted");
        assert_eq!(bound.value.to_canonical(), "-10");
    }

    #[test]
    fn two_halves_reach_the_objective_and_prove_the_value_between() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let bound = dual_bound(
            &mut Work::new(),
            &axioms,
            &[(0, ratio(1, 2)), (2, ratio(1, 2))],
            &everything,
        )
        .expect("accepted");
        assert_eq!(bound.value.to_canonical(), "-7");
    }

    /// A negative multiplier reverses the inequality it is applied to, and it is
    /// checked before the sum is formed: afterwards it is indistinguishable from
    /// arithmetic that came out wrong.
    #[test]
    fn trap_negative_dual_multiplier() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let refusal = dual_bound(&mut Work::new(), &axioms, &[(0, whole(-1))], &everything)
            .expect_err("must be refused");
        assert_eq!(refusal.code, Code::DUAL_NEGATIVE);
    }

    #[test]
    fn a_zero_multiplier_is_refused_even_when_the_bound_is_right() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let refusal = dual_bound(
            &mut Work::new(),
            &axioms,
            &[(0, Frac::one()), (1, Frac::zero())],
            &everything,
        )
        .expect_err("must be refused");
        assert_eq!(refusal.code, Code::DUAL_ZERO);
    }

    #[test]
    fn a_true_inequality_that_does_not_sum_to_the_objective_proves_nothing() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let refusal = dual_bound(&mut Work::new(), &axioms, &[(1, Frac::one())], &everything)
            .expect_err("must be refused");
        assert_eq!(refusal.code, Code::DUAL_COEFFICIENTS);
    }

    #[test]
    fn a_citation_of_an_inequality_that_does_not_exist_is_named() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let refusal = dual_bound(&mut Work::new(), &axioms, &[(9, Frac::one())], &everything)
            .expect_err("must be refused");
        assert_eq!(refusal.code, Code::DUAL_NO_SUCH_INEQUALITY);
    }

    #[test]
    fn an_invisible_inequality_is_refused_by_the_scope_rule() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let refusal = dual_bound(&mut Work::new(), &axioms, &[(0, Frac::one())], &|id| {
            id != 0
        })
        .expect_err("must be refused");
        assert_eq!(refusal.code, Code::SCOPE_NOT_VISIBLE);
    }

    #[test]
    fn the_complete_witness_includes_the_bound_multiplier() {
        let instance = model_b();
        let axioms = Axioms::of(&instance);
        contradiction(
            &mut Work::new(),
            &axioms,
            &[(0, Frac::one()), (1, Frac::one())],
            &everything,
        )
        .expect("accepted");
    }

    /// A Farkas witness over rows only, with the variable bounds assumed available
    /// by convention. Here a bound is a canonical inequality with an identifier
    /// exactly as a row side is, and the coefficients fail to cancel.
    #[test]
    fn trap_infeasible_farkas_bounds() {
        let instance = model_b();
        let axioms = Axioms::of(&instance);
        let refusal = contradiction(&mut Work::new(), &axioms, &[(0, Frac::one())], &everything)
            .expect_err("must be refused");
        assert_eq!(refusal.code, Code::DUAL_NOT_ZERO_SUM);
    }

    #[test]
    fn the_coefficient_check_runs_before_the_right_hand_side_check() {
        let instance = model_b();
        let axioms = Axioms::of(&instance);
        let refusal = contradiction(
            &mut Work::new(),
            &axioms,
            &[(0, Frac::one()), (1, ratio(2, 3))],
            &everything,
        )
        .expect_err("must be refused");
        assert_eq!(refusal.code, Code::DUAL_NOT_ZERO_SUM);
    }

    #[test]
    fn a_zero_sum_with_a_nonpositive_right_hand_side_is_not_a_contradiction() {
        // x >= 0 and -x >= -10 sum to zero coefficients with right-hand sides
        // summing to -10, which says zero is at least minus ten. True, and no
        // contradiction.
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let refusal = contradiction(
            &mut Work::new(),
            &axioms,
            &[(1, Frac::one()), (2, Frac::one())],
            &everything,
        )
        .expect_err("must be refused");
        assert_eq!(refusal.code, Code::DUAL_NOT_POSITIVE);
    }

    #[test]
    fn a_stated_bound_is_compared_against_the_one_the_combination_establishes() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let bound = dual_bound(&mut Work::new(), &axioms, &[(0, Frac::one())], &everything)
            .expect("accepted");
        bound_is(&bound, &whole(-4)).expect("agrees");
        assert_eq!(
            bound_is(&bound, &whole(-3)).expect_err("disagrees").code,
            Code::DUAL_BOUND_VALUE
        );
    }

    #[test]
    fn a_point_is_checked_against_rows_bounds_and_integrality_in_that_order() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        point_is_feasible(&mut Work::new(), &axioms, &[whole(4)]).expect("feasible");

        assert_eq!(
            point_is_feasible(&mut Work::new(), &axioms, &[whole(5)])
                .expect_err("above the row")
                .code,
            Code::POINT_ROW
        );
        assert_eq!(
            point_is_feasible(&mut Work::new(), &axioms, &[whole(-1)])
                .expect_err("below the bound")
                .code,
            Code::POINT_BOUND
        );
        assert_eq!(
            point_is_feasible(&mut Work::new(), &axioms, &[ratio(7, 2)])
                .expect_err("fractional")
                .code,
            Code::POINT_INTEGRALITY
        );
        assert_eq!(
            point_is_feasible(&mut Work::new(), &axioms, &[])
                .expect_err("wrong length")
                .code,
            Code::POINT_DIMENSION
        );
    }

    #[test]
    fn the_objective_is_recomputed_and_compared() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        assert_eq!(
            objective_is(&mut Work::new(), &axioms, &[whole(4)], &whole(-4))
                .expect("agrees")
                .to_canonical(),
            "-4"
        );
        assert_eq!(
            objective_is(&mut Work::new(), &axioms, &[whole(4)], &whole(-3))
                .expect_err("disagrees")
                .code,
            Code::POINT_OBJECTIVE
        );
    }

    #[test]
    fn the_gap_scales_by_the_larger_of_one_and_the_incumbent() {
        assert_eq!(
            relative_gap(&mut Work::new(), &whole(-4), &whole(-4)).to_canonical(),
            "0"
        );
        assert_eq!(
            relative_gap(&mut Work::new(), &whole(-4), &whole(-8)).to_canonical(),
            "1"
        );
        assert_eq!(
            relative_gap(&mut Work::new(), &ratio(1, 2), &Frac::zero()).to_canonical(),
            "1/2"
        );
    }
}
