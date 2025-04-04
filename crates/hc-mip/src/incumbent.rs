//! Accepting a solution, exactly.
//!
//! An incumbent is not a good answer the search found. It is the number every
//! prune in the tree is measured against, and a prune is irreversible: once a
//! node is closed because its bound is no better than the incumbent, the region
//! it covered is gone. If the incumbent is not genuinely feasible, the search
//! may have thrown away the answer, and the certificate it eventually writes
//! will be a correct proof of a false statement.
//!
//! So a candidate is screened here against the model's own rationals, and the
//! screening has no tolerance in it anywhere. A point that is feasible within a
//! fence is not feasible; the fence is a search heuristic and has no standing
//! in a conclusion.
//!
//! Three things are checked and each has its own answer, because a candidate
//! that is rejected is worth understanding: does it satisfy the system, does it
//! sit inside every bound, and is it integral wherever the model says so.

use hc_exact::Rational;
use hc_lp::exact::Canonical;

/// A solution that survived screening.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Incumbent {
    /// The point, over every column of the working shape.
    pub point: Vec<Rational>,
    /// The objective it attains, recomputed rather than carried.
    pub objective: Rational,
}

/// Why a candidate was not accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejected {
    /// The point does not satisfy the system exactly.
    Residual {
        /// The row whose residual is nonzero.
        row: usize,
    },
    /// A column sits outside its bounds exactly.
    Bounds {
        /// The column at fault.
        column: usize,
    },
    /// A column the model declares integer holds a fraction.
    Fractional {
        /// The column at fault.
        column: usize,
    },
    /// The candidate has the wrong number of components.
    Dimension,
}

impl core::fmt::Display for Rejected {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Residual { row } => write!(formatter, "row {row} has a nonzero residual"),
            Self::Bounds { column } => write!(formatter, "column {column} is outside its bounds"),
            Self::Fractional { column } => {
                write!(formatter, "column {column} is integer and holds a fraction")
            }
            Self::Dimension => formatter.write_str("the candidate is the wrong length"),
        }
    }
}

impl core::error::Error for Rejected {}

/// Screens a candidate against the model.
///
/// # Errors
///
/// Returns [`Rejected`] naming the first thing that failed.
pub fn screen(canonical: &Canonical, point: &[Rational]) -> Result<Incumbent, Rejected> {
    if point.len() != canonical.columns() {
        return Err(Rejected::Dimension);
    }
    if let Some(row) = canonical
        .residual_at(point)
        .iter()
        .position(|value| !value.is_zero())
    {
        return Err(Rejected::Residual { row });
    }
    if let Some(column) = canonical.violations(point).first() {
        return Err(Rejected::Bounds { column: *column });
    }
    for (column, value) in point.iter().enumerate() {
        if canonical.is_integral(column) && !value.is_integer() {
            return Err(Rejected::Fractional { column });
        }
    }
    Ok(Incumbent {
        objective: canonical.objective_at(point),
        point: point.to_vec(),
    })
}

/// The most fractional integer column of a point, and the integer to split at.
///
/// "Most fractional" means the component whose distance from the nearest
/// integer is largest. Ties go to the lowest column index, which is the
/// project's final tie-break everywhere; the choice affects the shape of the
/// tree and never the correctness of what the tree proves.
#[must_use]
pub fn most_fractional(canonical: &Canonical, point: &[Rational]) -> Option<(usize, Rational)> {
    let half = Rational::from_i64(2).recip().expect("two is not zero");
    let mut best: Option<(usize, Rational, Rational)> = None;

    for (column, value) in point.iter().enumerate() {
        if !canonical.is_integral(column) || value.is_integer() {
            continue;
        }
        let floor = Rational::from_int(value.floor());
        let fraction = value.sub(&floor);
        // Distance to the nearer integer: the fraction below one half, or its
        // complement above.
        let distance = if fraction.cmp_value(&half).is_gt() {
            Rational::one().sub(&fraction)
        } else {
            fraction
        };
        let better = match &best {
            None => true,
            Some((_, seen, _)) => distance.cmp_value(seen).is_gt(),
        };
        if better {
            best = Some((column, distance, floor));
        }
    }
    best.map(|(column, _, floor)| (column, floor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_core::Model;
    use hc_exact::BigInt;
    use hc_io::mps::{Form, read};

    const TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

    fn model() -> Model {
        read(TINY, Form::Fixed).expect("the frozen instance parses")
    }

    fn ratio(top: i64, bottom: i64) -> Rational {
        Rational::new(BigInt::from_i64(top), BigInt::from_i64(bottom)).expect("a nonzero bottom")
    }

    fn whole(value: i64) -> Rational {
        Rational::from_i64(value)
    }

    /// The working shape of the frozen instance has two columns: the variable
    /// and the logical standing for the single row's activity.
    fn point(x: Rational) -> Vec<Rational> {
        vec![x.clone(), x]
    }

    #[test]
    fn a_genuinely_feasible_integral_point_is_accepted_with_its_own_objective() {
        let canonical = Canonical::of(&model());
        let accepted = screen(&canonical, &point(whole(4))).expect("x = 4 is feasible");
        assert_eq!(accepted.objective.to_canonical_string(), "-4");
    }

    #[test]
    fn a_point_that_breaks_the_system_is_named_by_its_row() {
        let canonical = Canonical::of(&model());
        // The logical must equal the row's activity exactly; here it does not.
        let broken = vec![whole(3), whole(4)];
        assert_eq!(
            screen(&canonical, &broken),
            Err(Rejected::Residual { row: 0 })
        );
    }

    #[test]
    fn a_point_outside_its_bounds_is_named_by_its_column() {
        let canonical = Canonical::of(&model());
        assert!(matches!(
            screen(&canonical, &point(whole(-1))),
            Err(Rejected::Bounds { .. })
        ));
        assert!(matches!(
            screen(&canonical, &point(whole(9))),
            Err(Rejected::Bounds { .. })
        ));
    }

    #[test]
    fn a_fraction_where_the_model_declares_an_integer_is_refused() {
        let canonical = Canonical::of(&model());
        assert_eq!(
            screen(&canonical, &point(ratio(7, 2))),
            Err(Rejected::Fractional { column: 0 })
        );
    }

    #[test]
    fn a_candidate_of_the_wrong_length_is_refused_rather_than_padded() {
        let canonical = Canonical::of(&model());
        assert_eq!(screen(&canonical, &[]), Err(Rejected::Dimension));
    }

    #[test]
    fn the_most_fractional_column_is_the_one_furthest_from_an_integer() {
        let canonical = Canonical::of(&model());
        // Only column zero is integral in this model, so it is the answer
        // whenever it is fractional and there is no answer when it is not.
        let (column, at) =
            most_fractional(&canonical, &point(ratio(7, 2))).expect("a fractional column");
        assert_eq!(column, 0);
        assert_eq!(at.to_canonical_string(), "3");
        assert_eq!(most_fractional(&canonical, &point(whole(4))), None);
    }

    #[test]
    fn the_split_is_taken_at_the_floor_on_both_sides_of_a_half() {
        let canonical = Canonical::of(&model());
        for (top, bottom, expected) in [(1, 4, "0"), (3, 4, "0"), (9, 4, "2"), (7, 4, "1")] {
            let (_, at) = most_fractional(&canonical, &point(ratio(top, bottom)))
                .expect("a fractional column");
            assert_eq!(at.to_canonical_string(), expected, "{top}/{bottom}");
        }
    }
}
