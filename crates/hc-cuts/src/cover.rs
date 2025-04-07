//! Knapsack cover cuts, and the lifting nobody is trusted about.
//!
//! A cover is a set of binary variables whose weights together exceed the
//! capacity of a row. Not all of them can be chosen, so the base cut says at
//! most `|C| - 1` of them may be. That much is easy to see and easy to check:
//! the condition is a strict inequality on a sum of integers, and the campaign
//! that plants a non-strict "cover" exists because reading it as non-strict is
//! wrong on exactly the cases nobody looks at.
//!
//! Lifting is where a cover cut becomes strong and where it becomes dangerous.
//! Each variable outside the cover may be given a coefficient, and the largest
//! one that keeps the cut valid is the answer to a small optimisation problem.
//! A separation routine may guess it; nothing here reads the guess. The
//! coefficient is recomputed by the dynamic program below, in exact integers,
//! and compared.
//!
//! The caps are a real part of the rule rather than an implementation limit.
//! Outside them the lifting is not attempted and only the unlifted cover cut is
//! admissible, so a certificate can never contain a lifted coefficient that
//! nobody could afford to recheck.

use hc_exact::Rational;
use hc_lp::exact::Canonical;

/// The most variables a lifting program will consider.
pub const DP_VARS: usize = 64;
/// The largest capacity a lifting program will consider.
pub const DP_CAPACITY: u64 = 4_096;
/// The most states a lifting program will build.
pub const DP_STATES: u64 = 1_000_000;

/// A row read as a binary knapsack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Knapsack {
    /// The columns and their weights, ascending by column.
    pub weights: Vec<(usize, u64)>,
    /// The capacity.
    pub capacity: u64,
}

/// Why a row or a cover is not usable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The row does not exist.
    NoSuchRow {
        /// The index.
        row: usize,
    },
    /// The row has no finite upper side to be a capacity.
    NoCapacity,
    /// A coefficient or the capacity is not a positive integer.
    NotPositiveInteger,
    /// A column in the row is not a binary variable.
    NotBinary {
        /// The column.
        column: usize,
    },
    /// The proposed cover's weights do not exceed the capacity.
    NotACover,
    /// A named column is not in the row.
    NotInRow {
        /// The column.
        column: usize,
    },
    /// A lifting order names a column twice, or one already in the cover.
    BadOrder {
        /// The column.
        column: usize,
    },
    /// A lifted variable admits no feasible assignment, so it should have been
    /// fixed rather than lifted.
    NoFeasibleState {
        /// The column.
        column: usize,
    },
    /// The program is larger than the caps admit.
    BeyondCaps,
}

impl core::fmt::Display for Refused {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoSuchRow { row } => write!(formatter, "row {row} does not exist"),
            Self::NoCapacity => formatter.write_str("the row has no finite upper side"),
            Self::NotPositiveInteger => {
                formatter.write_str("a weight or the capacity is not a positive integer")
            }
            Self::NotBinary { column } => write!(formatter, "column {column} is not binary"),
            Self::NotACover => {
                formatter.write_str("the cover's weights do not exceed the capacity")
            }
            Self::NotInRow { column } => write!(formatter, "column {column} is not in the row"),
            Self::BadOrder { column } => {
                write!(formatter, "column {column} appears twice in the lift order")
            }
            Self::NoFeasibleState { column } => write!(
                formatter,
                "column {column} admits no feasible assignment and should have been fixed"
            ),
            Self::BeyondCaps => formatter.write_str("the lifting program exceeds its caps"),
        }
    }
}

impl core::error::Error for Refused {}

/// Reads a row as a binary knapsack, or says why it is not one.
///
/// # Errors
///
/// Returns [`Refused`] naming what disqualified the row.
pub fn knapsack_from(canonical: &Canonical, row: usize) -> Result<Knapsack, Refused> {
    if row >= canonical.rows() {
        return Err(Refused::NoSuchRow { row });
    }
    // A row's capacity is the upper bound of its activity column, which is
    // where the working shape keeps a row's own sides.
    let capacity = whole(
        canonical
            .upper(canonical.structural() + row)
            .ok_or(Refused::NoCapacity)?,
    )?;

    let mut weights = Vec::new();
    for column in 0..canonical.structural() {
        let mut coefficient = None;
        for (at, value) in canonical.column(column) {
            if *at as usize == row {
                coefficient = Some(value.clone());
            }
        }
        let Some(value) = coefficient else {
            continue;
        };
        if value.is_zero() {
            continue;
        }
        if !is_binary(canonical, column) {
            return Err(Refused::NotBinary { column });
        }
        let weight = whole(&value)?;
        if weight == 0 {
            return Err(Refused::NotPositiveInteger);
        }
        weights.push((column, weight));
    }
    Ok(Knapsack { weights, capacity })
}

/// Whether a column is a binary variable of the model.
fn is_binary(canonical: &Canonical, column: usize) -> bool {
    canonical.is_integral(column)
        && canonical
            .lower(column)
            .is_some_and(|value| *value == Rational::zero())
        && canonical
            .upper(column)
            .is_some_and(|value| *value == Rational::one())
}

/// A rational as a nonnegative machine integer, or a refusal.
fn whole(value: &Rational) -> Result<u64, Refused> {
    if !value.is_integer() || value.is_negative() {
        return Err(Refused::NotPositiveInteger);
    }
    value
        .floor()
        .to_string()
        .parse()
        .map_err(|_| Refused::NotPositiveInteger)
}

/// Whether a set of columns is genuinely a cover.
///
/// A strict inequality, and deliberately so: reading it as non-strict admits a
/// set whose weights exactly fill the capacity, and the cut that follows
/// removes the feasible point where every member is chosen.
#[must_use]
pub fn is_cover(knapsack: &Knapsack, cover: &[usize]) -> bool {
    let mut total = 0_u64;
    for column in cover {
        match knapsack.weights.iter().find(|(at, _)| at == column) {
            None => return false,
            Some((_, weight)) => total = total.saturating_add(*weight),
        }
    }
    total > knapsack.capacity
}

/// A cover cut: coefficients on the left and a bound on the right, in the
/// `sum <= limit` sense the family is stated in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cut {
    /// The columns and their coefficients, ascending by column.
    pub coefficients: Vec<(usize, i64)>,
    /// The right-hand side.
    pub limit: i64,
}

/// Recomputes the lifted cover cut.
///
/// `order` is the sequence the lifting was performed in. It is part of the
/// derivation rather than a hint: lifting one variable changes what the next
/// one may take, so a different order is a different cut.
///
/// # Errors
///
/// Returns [`Refused`] naming what stopped the derivation.
pub fn lift(knapsack: &Knapsack, cover: &[usize], order: &[usize]) -> Result<Cut, Refused> {
    if !is_cover(knapsack, cover) {
        return Err(Refused::NotACover);
    }
    if knapsack.weights.len() > DP_VARS || knapsack.capacity > DP_CAPACITY {
        return Err(Refused::BeyondCaps);
    }
    let states = (knapsack.weights.len() as u64).saturating_mul(knapsack.capacity + 1);
    if states > DP_STATES {
        return Err(Refused::BeyondCaps);
    }

    let limit = cover.len() as i64 - 1;
    // The value each column contributes to the objective the program
    // maximises. Cover members are worth one; a variable already lifted is
    // worth its coefficient; everything else is worth nothing yet.
    let mut value: Vec<(usize, i64)> = cover.iter().map(|column| (*column, 1)).collect();
    let mut lifted: Vec<(usize, i64)> = Vec::new();

    for column in order {
        if cover.contains(column) || lifted.iter().any(|(at, _)| at == column) {
            return Err(Refused::BadOrder { column: *column });
        }
        let Some((_, weight)) = knapsack.weights.iter().find(|(at, _)| at == column) else {
            return Err(Refused::NotInRow { column: *column });
        };
        if *weight > knapsack.capacity {
            // Fixing this variable to one leaves no feasible assignment, so
            // the variable should have been fixed to zero rather than lifted.
            return Err(Refused::NoFeasibleState { column: *column });
        }

        let room = knapsack.capacity - weight;
        let best = maximum(knapsack, &value, *column, room);
        let coefficient = limit - best;
        if coefficient < 0 {
            // A negative coefficient would weaken the cut into something the
            // family does not describe.
            return Err(Refused::NoFeasibleState { column: *column });
        }
        if coefficient > 0 {
            value.push((*column, coefficient));
        }
        lifted.push((*column, coefficient));
    }

    let mut coefficients: Vec<(usize, i64)> = cover.iter().map(|column| (*column, 1)).collect();
    for (column, coefficient) in lifted {
        if coefficient != 0 {
            coefficients.push((column, coefficient));
        }
    }
    coefficients.sort_unstable();
    Ok(Cut {
        coefficients,
        limit,
    })
}

/// The most the objective can reach inside a capacity, excluding one column.
///
/// An ordinary zero-one knapsack over machine integers. Everything in it is
/// bounded by the caps checked before it runs.
fn maximum(knapsack: &Knapsack, value: &[(usize, i64)], excluded: usize, room: u64) -> i64 {
    let capacity = usize::try_from(room).unwrap_or(0);
    let mut best = vec![0_i64; capacity + 1];
    for (column, weight) in &knapsack.weights {
        if *column == excluded {
            continue;
        }
        let Some((_, worth)) = value.iter().find(|(at, _)| at == column) else {
            continue;
        };
        let weight = usize::try_from(*weight).unwrap_or(usize::MAX);
        if weight > capacity {
            continue;
        }
        for room in (weight..=capacity).rev() {
            let candidate = best[room - weight] + worth;
            if candidate > best[room] {
                best[room] = candidate;
            }
        }
    }
    best[capacity]
}

/// The cut in the canonical `a . x >= b` sense a proof cites.
///
/// The family is stated as `sum <= limit`, and every inequality a certificate
/// cites is written the one way, so the sign is moved here rather than at each
/// use site.
#[must_use]
pub fn as_inequality(cut: &Cut) -> (Vec<(usize, Rational)>, Rational) {
    let coefficients = cut
        .coefficients
        .iter()
        .map(|(column, value)| (*column, Rational::from_i64(-value)))
        .collect();
    (coefficients, Rational::from_i64(-cut.limit))
}
