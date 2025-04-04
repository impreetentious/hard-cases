//! The inequalities a proof is allowed to cite.
//!
//! A model has rows with two optional sides, and columns with two optional
//! bounds. A proof has one kind of thing: an inequality `a . x >= b` with a
//! stable identifier. This module is the whole of the translation between
//! them, and it exists as its own file because the uniformity is load-bearing
//! rather than cosmetic.
//!
//! The uniformity is what makes a contradiction complete. Farkas witnesses are
//! usually written over rows, with the variable bounds supplied by a
//! convention the reader is expected to know; a witness that omits a bound
//! multiplier then looks like a witness. Here a bound is an inequality with an
//! identifier exactly as a row side is, the multipliers have to sum to zero
//! over everything, and the classic omission simply fails to sum.
//!
//! Nothing here is dense. A model with a hundred thousand columns has an
//! objective vector with a hundred thousand entries and inequalities with a
//! handful each, so an inequality is a sorted list of the columns it mentions.
//! The comparison against the objective is then a merge, and a coefficient the
//! combination never touches has to be zero in the objective for the merge to
//! succeed — which is the check, not an omission from it.

use crate::vmodel::{Face, Instance};
use crate::vrat::Frac;
use crate::vwork::Work;

/// One canonical inequality, in the single form `a . x >= b`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inequality {
    /// The columns it mentions and their coefficients, ascending by column,
    /// with no zero among them.
    pub terms: Vec<(u32, Frac)>,
    /// The right-hand side.
    pub right: Frac,
}

impl Inequality {
    /// The coefficient on a column, which is zero where the list is silent.
    #[must_use]
    pub fn coefficient(&self, column: u32) -> Frac {
        self.terms
            .binary_search_by(|(at, _)| at.cmp(&column))
            .map_or_else(|_| Frac::zero(), |at| self.terms[at].1.clone())
    }
}

/// The inequalities of one model, addressed by identifier.
///
/// Identifiers are positions in the order `Instance::axioms` fixes: rows in
/// index order with the lower side before the upper, then columns in index
/// order with the lower bound before the upper. Only finite sides take an
/// identifier, because an absent bound is not something a proof may lean on.
#[derive(Clone, Debug)]
pub struct Axioms<'a> {
    instance: &'a Instance,
    rows: Vec<Vec<(u32, Frac)>>,
    /// Inequalities introduced by the certificate rather than by the model.
    ///
    /// Branch assumptions and, later, cuts. They take identifiers above the
    /// model's base block, assigned in certificate record order, which is what
    /// lets both implementations compute the same identifiers without either
    /// telling the other.
    extra: Vec<Inequality>,
}

impl<'a> Axioms<'a> {
    /// Indexes a model's inequalities.
    ///
    /// The row-major view is built once here. The instance stores its
    /// coefficients by column because that is the order its dump walks, and
    /// every inequality this module hands out is a row, so building the
    /// transpose once is cheaper than searching the column list per row and is
    /// the only copy of the matrix this crate makes.
    #[must_use]
    pub fn of(instance: &'a Instance) -> Self {
        let mut rows = vec![Vec::new(); instance.rows.len()];
        for cell in &instance.cells {
            if cell.value.is_zero() {
                continue;
            }
            rows[cell.row as usize].push((cell.column, cell.value.clone()));
        }
        for row in &mut rows {
            row.sort_by_key(|(column, _)| *column);
        }
        Self {
            instance,
            rows,
            extra: Vec::new(),
        }
    }

    /// The number of inequalities the model itself supplies.
    ///
    /// Everything at or above this is something the certificate introduced.
    #[must_use]
    pub fn base(&self) -> u64 {
        self.instance.axioms().len() as u64
    }

    /// Adds an inequality the certificate introduced, returning its
    /// identifier.
    ///
    /// The identifier is the next one, and nothing may take it back. A record
    /// stream that allocated identifiers in one order and cited them in
    /// another would be a stream whose meaning depended on how it was read.
    pub fn introduce(&mut self, inequality: Inequality) -> u64 {
        let id = self.base() + self.extra.len() as u64;
        self.extra.push(inequality);
        id
    }

    /// How many inequalities exist, the introduced ones included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.instance.axioms().len() + self.extra.len()
    }

    /// Whether the model has no inequalities at all.
    ///
    /// A model with no finite row side and no finite bound is unusual and not
    /// impossible, and a proof over it can cite nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The inequality an identifier names, when it names one.
    #[must_use]
    pub fn get(&self, id: u64) -> Option<Inequality> {
        if id >= self.base() {
            let at = usize::try_from(id - self.base()).ok()?;
            return self.extra.get(at).cloned();
        }
        let at = usize::try_from(id).ok()?;
        let axiom = *self.instance.axioms().get(at)?;
        Some(match (axiom.from_row, axiom.face) {
            (true, Face::Low) => Inequality {
                terms: self.rows[axiom.at as usize].clone(),
                right: self.instance.rows[axiom.at as usize]
                    .low
                    .clone()
                    .unwrap_or_else(Frac::zero),
            },
            (true, Face::High) => Inequality {
                terms: self.rows[axiom.at as usize]
                    .iter()
                    .map(|(column, value)| (*column, value.negated()))
                    .collect(),
                right: self.instance.rows[axiom.at as usize]
                    .high
                    .clone()
                    .unwrap_or_else(Frac::zero)
                    .negated(),
            },
            (false, Face::Low) => Inequality {
                terms: vec![(axiom.at, Frac::one())],
                right: self.instance.columns[axiom.at as usize]
                    .low
                    .clone()
                    .unwrap_or_else(Frac::zero),
            },
            (false, Face::High) => Inequality {
                terms: vec![(axiom.at, Frac::one().negated())],
                right: self.instance.columns[axiom.at as usize]
                    .high
                    .clone()
                    .unwrap_or_else(Frac::zero)
                    .negated(),
            },
        })
    }

    /// The objective vector, as the sparse list a combination is compared
    /// against.
    #[must_use]
    pub fn objective(&self) -> Vec<(u32, Frac)> {
        self.instance
            .columns
            .iter()
            .enumerate()
            .filter(|(_, column)| !column.cost.is_zero())
            .map(|(index, column)| (index as u32, column.cost.clone()))
            .collect()
    }

    /// The model the inequalities are about.
    #[must_use]
    pub const fn instance(&self) -> &'a Instance {
        self.instance
    }
}

/// A running sparse sum of multiplied inequalities.
///
/// Kept ordered by column so that the final comparison against the objective
/// is a walk down two sorted lists. Terms that cancel are dropped, which is
/// what makes a contradiction's zero vector an empty list rather than a list
/// of zeros nobody remembered to remove.
#[derive(Clone, Debug, Default)]
pub struct Sum {
    terms: Vec<(u32, Frac)>,
}

impl Sum {
    /// An empty sum.
    #[must_use]
    pub fn new() -> Self {
        Self { terms: Vec::new() }
    }

    /// Adds a multiple of one inequality's coefficients.
    ///
    /// The meter is threaded rather than optional: this is where a proof's
    /// arithmetic actually happens, and an unmetered path through it would be
    /// an unmetered path through the whole calculus.
    pub fn add(&mut self, work: &mut Work, multiplier: &Frac, inequality: &Inequality) {
        for (column, value) in &inequality.terms {
            let scaled = work.multiply(value, multiplier);
            if scaled.is_zero() {
                continue;
            }
            match self.terms.binary_search_by(|(at, _)| at.cmp(column)) {
                Ok(at) => {
                    let total = work.add(&self.terms[at].1, &scaled);
                    if total.is_zero() {
                        self.terms.remove(at);
                    } else {
                        self.terms[at].1 = total;
                    }
                }
                Err(at) => self.terms.insert(at, (*column, scaled)),
            }
        }
    }

    /// Whether every coefficient is exactly zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    /// The accumulated terms, ascending by column.
    #[must_use]
    pub fn terms(&self) -> &[(u32, Frac)] {
        &self.terms
    }

    /// The first column at which this sum differs from a target vector.
    ///
    /// Both lists are ascending and neither holds a zero, so a difference is
    /// either a column one has and the other does not, or a column both have
    /// with unequal values. Returning the column rather than a boolean is what
    /// lets a refusal name the coefficient that did not match.
    #[must_use]
    pub fn first_difference(&self, target: &[(u32, Frac)]) -> Option<u32> {
        let (mut here, mut there) = (0, 0);
        loop {
            match (self.terms.get(here), target.get(there)) {
                (None, None) => return None,
                (Some((column, _)), None) | (None, Some((column, _))) => return Some(*column),
                (Some((left, ours)), Some((right, theirs))) => {
                    if left != right {
                        return Some(*left.min(right));
                    }
                    if ours != theirs {
                        return Some(*left);
                    }
                    here += 1;
                    there += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmodel::{Cell, Column, Constraint, Direction, Tag, whole};

    /// The model the frozen dual vectors are written against: minimize -x
    /// subject to x <= 4, with x between 0 and 10 and integer.
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

    #[test]
    fn the_three_inequalities_are_the_ones_the_vectors_name() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        assert_eq!(axioms.len(), 3);

        let row = axioms.get(0).expect("the row upper side");
        assert_eq!(row.terms, vec![(0, whole(-1))]);
        assert_eq!(row.right, whole(-4));

        let low = axioms.get(1).expect("the variable lower bound");
        assert_eq!(low.terms, vec![(0, whole(1))]);
        assert_eq!(low.right, whole(0));

        let high = axioms.get(2).expect("the variable upper bound");
        assert_eq!(high.terms, vec![(0, whole(-1))]);
        assert_eq!(high.right, whole(-10));

        assert!(axioms.get(3).is_none());
    }

    #[test]
    fn an_introduced_inequality_takes_the_next_identifier_and_answers_to_it() {
        let instance = model_a();
        let mut axioms = Axioms::of(&instance);
        let base = axioms.base();
        assert_eq!(base, 3);

        let id = axioms.introduce(Inequality {
            terms: vec![(0, whole(-1))],
            right: whole(-2),
        });
        assert_eq!(id, base);
        assert_eq!(axioms.len(), 4);
        let introduced = axioms.get(id).expect("the introduced inequality");
        assert_eq!(introduced.right, whole(-2));
        assert!(axioms.get(base + 1).is_none());

        // The model's own inequalities are untouched by the introduction.
        assert_eq!(axioms.get(0).expect("the row side").right, whole(-4));
    }

    #[test]
    fn the_objective_vector_omits_the_zeros() {
        let mut instance = model_a();
        instance.columns.push(Column {
            low: Some(whole(0)),
            high: None,
            whole: false,
            cost: Frac::zero(),
            name: "Y".to_owned(),
        });
        let axioms = Axioms::of(&instance);
        assert_eq!(axioms.objective(), vec![(0, whole(-1))]);
    }

    #[test]
    fn a_multiple_of_the_row_side_reaches_the_objective_vector() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let mut sum = Sum::new();
        sum.add(
            &mut Work::new(),
            &Frac::one(),
            &axioms.get(0).expect("the row upper side"),
        );
        assert_eq!(sum.first_difference(&axioms.objective()), None);
    }

    #[test]
    fn halves_of_two_upper_sides_reach_it_as_well() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let half = Frac::ratio(
            &crate::vint::Int::from_i64(1),
            &crate::vint::Int::from_i64(2),
        )
        .expect("one half");
        let mut work = Work::new();
        let mut sum = Sum::new();
        sum.add(
            &mut work,
            &half,
            &axioms.get(0).expect("the row upper side"),
        );
        sum.add(
            &mut work,
            &half,
            &axioms.get(2).expect("the variable upper bound"),
        );
        assert_eq!(sum.first_difference(&axioms.objective()), None);
    }

    #[test]
    fn opposite_multiples_cancel_to_an_empty_sum_rather_than_to_zeros() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let mut sum = Sum::new();
        sum.add(
            &mut Work::new(),
            &Frac::one(),
            &axioms.get(1).expect("the lower bound"),
        );
        sum.add(
            &mut Work::new(),
            &Frac::one(),
            &axioms.get(0).expect("the row upper side"),
        );
        assert!(sum.is_zero());
        assert!(sum.terms().is_empty());
    }

    #[test]
    fn a_difference_is_reported_at_the_column_it_happens_at() {
        let instance = model_a();
        let axioms = Axioms::of(&instance);
        let mut sum = Sum::new();
        sum.add(
            &mut Work::new(),
            &Frac::one(),
            &axioms.get(1).expect("the lower bound"),
        );
        assert_eq!(sum.first_difference(&axioms.objective()), Some(0));

        let empty = Sum::new();
        assert_eq!(empty.first_difference(&axioms.objective()), Some(0));
        assert_eq!(empty.first_difference(&[]), None);
    }
}
