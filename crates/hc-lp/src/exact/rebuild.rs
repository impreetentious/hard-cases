//! Rebuilding what a basis implies, exactly.
//!
//! A basis is not a conclusion. It is a list of which columns the search
//! believes are basic, arrived at in floating point, and the numbers attached
//! to it are approximations of quantities nobody has established. What this
//! module does is take that list and recompute, over the model's exact
//! rationals, the vertex it names and the row multipliers it implies.
//!
//! The recomputation can fail, and failing is a normal outcome rather than an
//! error. A basis that factors comfortably in floating point can be singular
//! exactly; that is not a defect in the search, it is the reason the rebuild
//! happens at all.

use hc_exact::Rational;

use crate::basis::{Basis, Status};
use crate::exact::canonical::Canonical;
use crate::exact::system::{Failure, System};
use hc_core::{RowId, Side, VarId};

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

/// One row of the basis-transformed matrix, exactly.
///
/// The multipliers `rho` solving `B^T rho = e_r`, which turn the model's row
/// equations into the equation the basis puts row `r` in. A cut derivation
/// records them and both implementations reconstruct the equation from them,
/// so this is the one thing about a basis that reaches a certificate — as
/// numbers, checked by recomputation, rather than as a claim about a basis.
///
/// # Errors
///
/// Returns [`Gap::Singular`] when the basis is singular in exact arithmetic.
pub fn tableau_row(
    canonical: &Canonical,
    basis: &Basis,
    position: usize,
) -> Result<Vec<Rational>, Gap> {
    let columns: Vec<Vec<(u32, Rational)>> = basis
        .basic()
        .iter()
        .map(|column| canonical.column(*column).to_vec())
        .collect();
    let system = System::from_columns(canonical.rows(), &columns).map_err(Gap::Singular)?;
    let mut unit = vec![Rational::zero(); canonical.rows()];
    if position >= unit.len() {
        return Err(Gap::Singular(Failure::Singular { at: position }));
    }
    unit[position] = Rational::one();
    system.transposed().solve(&unit).map_err(Gap::Singular)
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
