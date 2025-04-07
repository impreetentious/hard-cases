//! The Gomory mixed-integer derivation.
//!
//! The classical derivation starts from a tableau row, which is solver state.
//! This one starts from a *combination of the model's row equations*, which is
//! not: the certificate records the multipliers, and everything else — which
//! variables appear, what they are displaced from, what the constant is — is
//! computed from the model by both implementations.
//!
//! Writing it that way costs nothing and buys the whole property. A
//! displacement defined from the model's own bounds is nonnegative at every
//! point of the model, not merely at the vertex the search was sitting on, so
//! the inequality that comes out is valid whatever the search believed.
//!
//! The shape. Every row `i` of the model defines an equation `a_i . x - s_i =
//! 0`, where `s_i` is that row's activity and carries the row's bounds. A
//! rational combination of those equations gives coefficients over every
//! column, structural and activity alike. One of them — an integer structural
//! column — must come out with coefficient exactly one; that is the isolated
//! variable. Every other nonzero column is displaced from a finite bound, and
//! the constant follows. Then the formula.

use hc_exact::Rational;
use hc_lp::exact::Canonical;

/// Which bound a column's displacement is measured from.
///
/// The choice is the model's, not a basis's: the lower bound when there is a
/// finite one, and the upper bound otherwise. That rule is what makes the
/// displacement nonnegative everywhere in the model rather than only at some
/// vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum From {
    /// `w = x - l`.
    Lower,
    /// `w = u - x`.
    Upper,
}

/// One displaced column in the reconstructed equation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Displacement {
    /// The column, in the working shape's numbering.
    pub column: usize,
    /// Which bound it is measured from.
    pub from: From,
    /// The endpoint it is measured from.
    pub endpoint: Rational,
    /// The coefficient of the displacement in the equation.
    pub coefficient: Rational,
    /// Whether the model declares the column integer.
    pub integral: bool,
}

/// The equation a combination reconstructs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Derivation {
    /// The isolated integer column.
    pub isolated: usize,
    /// The displaced columns, ascending by column.
    pub displacements: Vec<Displacement>,
    /// The constant on the right of the equation.
    pub constant: Rational,
}

/// Why a combination does not reconstruct a usable equation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The named column is not an integer column of the model.
    NotInteger {
        /// The column.
        column: usize,
    },
    /// The combination does not give the named column a coefficient of one.
    NotIsolated {
        /// The coefficient it gave.
        coefficient: Rational,
    },
    /// A displaced column has no finite bound to measure from.
    NoEndpoint {
        /// The column.
        column: usize,
    },
    /// An integer column is displaced from a fractional endpoint.
    FractionalEndpoint {
        /// The column.
        column: usize,
    },
    /// The constant is an integer, so the derivation yields nothing.
    IntegralConstant,
    /// A row index names a row the model does not have.
    NoSuchRow {
        /// The index.
        row: usize,
    },
}

impl core::fmt::Display for Refused {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotInteger { column } => {
                write!(formatter, "column {column} is not an integer column")
            }
            Self::NotIsolated { coefficient } => {
                write!(
                    formatter,
                    "the isolated column has coefficient {coefficient}"
                )
            }
            Self::NoEndpoint { column } => {
                write!(
                    formatter,
                    "column {column} has no finite bound to displace from"
                )
            }
            Self::FractionalEndpoint { column } => {
                write!(
                    formatter,
                    "column {column} is integer and its endpoint is not"
                )
            }
            Self::IntegralConstant => {
                formatter.write_str("the constant is an integer, so there is no cut")
            }
            Self::NoSuchRow { row } => write!(formatter, "row {row} does not exist"),
        }
    }
}

impl core::error::Error for Refused {}

/// Reconstructs the equation a combination of row equations gives.
///
/// # Errors
///
/// Returns [`Refused`] naming what stopped the reconstruction.
pub fn reconstruct(
    canonical: &Canonical,
    combination: &[(usize, Rational)],
    isolated: usize,
) -> Result<Derivation, Refused> {
    // The coefficient the combination gives each column. A structural column
    // picks up the row coefficients; the activity column of row `i` picks up
    // minus that row's own multiplier, because the equation is
    // `a_i . x - s_i = 0`.
    let mut alpha = vec![Rational::zero(); canonical.columns()];
    for (row, multiplier) in combination {
        if *row >= canonical.rows() {
            return Err(Refused::NoSuchRow { row: *row });
        }
        if multiplier.is_zero() {
            continue;
        }
        for (column, slot) in alpha.iter_mut().enumerate().take(canonical.structural()) {
            for (at, coefficient) in canonical.column(column) {
                if *at as usize == *row {
                    *slot = slot.add(&multiplier.mul(coefficient));
                }
            }
        }
        let activity = canonical.structural() + row;
        let slot = &mut alpha[activity];
        *slot = slot.sub(multiplier);
    }

    if isolated >= canonical.structural() || !canonical.is_integral(isolated) {
        return Err(Refused::NotInteger { column: isolated });
    }
    if alpha[isolated] != Rational::one() {
        return Err(Refused::NotIsolated {
            coefficient: alpha[isolated].clone(),
        });
    }

    let mut displacements = Vec::new();
    let mut constant = Rational::zero();
    for (column, coefficient) in alpha.iter().enumerate() {
        if column == isolated || coefficient.is_zero() {
            continue;
        }
        let (from, endpoint) = match (canonical.lower(column), canonical.upper(column)) {
            (Some(low), _) => (From::Lower, low.clone()),
            (None, Some(high)) => (From::Upper, high.clone()),
            (None, None) => return Err(Refused::NoEndpoint { column }),
        };
        let integral = canonical.is_integral(column);
        if integral && !endpoint.is_integer() {
            return Err(Refused::FractionalEndpoint { column });
        }
        // Substituting `x = l + w` or `x = u - w` moves the endpoint's
        // contribution to the constant and flips the sign in the upper case.
        constant = constant.sub(&coefficient.mul(&endpoint));
        let displacement = match from {
            From::Lower => coefficient.clone(),
            From::Upper => coefficient.neg(),
        };
        displacements.push(Displacement {
            column,
            from,
            endpoint,
            coefficient: displacement,
            integral,
        });
    }

    if constant.is_integer() {
        return Err(Refused::IntegralConstant);
    }
    Ok(Derivation {
        isolated,
        displacements,
        constant,
    })
}

/// A cut, in the model's own coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gmi {
    /// The coefficients over the working shape's columns, ascending.
    pub coefficients: Vec<(usize, Rational)>,
    /// The right-hand side, in the canonical `a . x >= b` sense.
    pub right: Rational,
}

/// The fractional part of a rational: the value less its floor.
#[must_use]
pub fn fraction(value: &Rational) -> Rational {
    value.sub(&Rational::from_int(value.floor()))
}

/// Applies the formula to a reconstructed equation.
///
/// The four cases are the four quadrants of the classical derivation. Swapping
/// two of them produces an inequality that is valid on most instances and
/// wrong on some, which is why each has its own vector.
#[must_use]
pub fn formula(derivation: &Derivation) -> Gmi {
    let f = fraction(&derivation.constant);
    let one_minus_f = Rational::one().sub(&f);

    // In displacement space first: one coefficient per displacement, and `f`
    // on the right.
    let mut displaced: Vec<(usize, Rational)> = Vec::new();
    for term in &derivation.displacements {
        let a = &term.coefficient;
        let coefficient = if term.integral {
            let f_j = fraction(a);
            if f_j.cmp_value(&f).is_le() {
                f_j
            } else {
                // f (1 - f_j) / (1 - f)
                f.mul(&Rational::one().sub(&f_j))
                    .mul(&one_minus_f.recip().expect("f is strictly below one"))
            }
        } else if a.is_negative() {
            // -f a / (1 - f)
            f.neg()
                .mul(a)
                .mul(&one_minus_f.recip().expect("f is strictly below one"))
        } else {
            a.clone()
        };
        if !coefficient.is_zero() {
            displaced.push((term.column, coefficient));
        }
    }

    // Then back to the model's coordinates. `w = x - l` contributes `c x` and
    // moves `c l` to the right; `w = u - x` contributes `-c x` and moves
    // `-c u` there.
    let mut coefficients: Vec<(usize, Rational)> = Vec::new();
    let mut right = f;
    for (column, coefficient) in &displaced {
        let term = derivation
            .displacements
            .iter()
            .find(|term| term.column == *column)
            .expect("a displacement the formula named");
        match term.from {
            From::Lower => {
                coefficients.push((*column, coefficient.clone()));
                right = right.add(&coefficient.mul(&term.endpoint));
            }
            From::Upper => {
                coefficients.push((*column, coefficient.neg()));
                right = right.sub(&coefficient.mul(&term.endpoint));
            }
        }
    }
    coefficients.sort_by_key(|(column, _)| *column);
    Gmi {
        coefficients,
        right,
    }
}

/// Reconstructs and applies the formula in one step.
///
/// # Errors
///
/// Returns [`Refused`] naming what stopped the derivation.
pub fn derive(
    canonical: &Canonical,
    combination: &[(usize, Rational)],
    isolated: usize,
) -> Result<Gmi, Refused> {
    Ok(formula(&reconstruct(canonical, combination, isolated)?))
}

/// The cut in the model's own variables, with activity columns expanded.
///
/// A cut over the working shape may mention an activity column, which is a row
/// of the model rather than a variable of it. Expanding it distributes the
/// coefficient over that row and leaves an inequality a proof can cite.
#[must_use]
pub fn over_variables(canonical: &Canonical, cut: &Gmi) -> Gmi {
    let mut out = vec![Rational::zero(); canonical.structural()];
    for (column, coefficient) in &cut.coefficients {
        if *column < canonical.structural() {
            let slot = &mut out[*column];
            *slot = slot.add(coefficient);
            continue;
        }
        let row = *column - canonical.structural();
        for (structural, slot) in out.iter_mut().enumerate() {
            for (at, value) in canonical.column(structural) {
                if *at as usize == row {
                    *slot = slot.add(&coefficient.mul(value));
                }
            }
        }
    }
    Gmi {
        coefficients: out
            .into_iter()
            .enumerate()
            .filter(|(_, value)| !value.is_zero())
            .collect(),
        right: cut.right.clone(),
    }
}
