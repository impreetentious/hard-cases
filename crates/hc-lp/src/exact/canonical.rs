//! The exact model in the shape the search worked on.
//!
//! The simplex operates on a copy with one logical column per row, so that
//! every constraint is a bound on a variable and the basis is always square.
//! A basis it returns is expressed in those coordinates, and rebuilding what
//! that basis implies means rebuilding it in the same coordinates — exactly
//! this time, from the model's own rationals rather than from the scaled
//! floating-point copy the search actually touched.
//!
//! Scaling does not appear here at all. The search may look at the model
//! through whatever change of units helps it; a proof is about the model.

use hc_core::{Model, RowId, VarId};
use hc_exact::Rational;

/// The exact model with its logical columns made explicit.
/// One row added to an exact view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactRow {
    /// The terms, over the model's variables.
    pub terms: Vec<(usize, Rational)>,
    /// The lower side, where it has one.
    pub low: Option<Rational>,
    /// The upper side, where it has one.
    pub high: Option<Rational>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canonical {
    row_count: usize,
    structural: usize,
    columns: Vec<Vec<(u32, Rational)>>,
    lower: Vec<Option<Rational>>,
    upper: Vec<Option<Rational>>,
    cost: Vec<Rational>,
    integral: Vec<bool>,
    constant: Rational,
    digest: [u8; 32],
}

impl Canonical {
    /// Builds the exact working shape from a model.
    #[must_use]
    pub fn of(model: &Model) -> Self {
        let structural = model.variables().len();
        let row_count = model.rows().len();
        let mut columns = Vec::with_capacity(structural + row_count);
        let mut lower = Vec::with_capacity(structural + row_count);
        let mut upper = Vec::with_capacity(structural + row_count);
        let mut cost = Vec::with_capacity(structural + row_count);
        let mut integral = Vec::with_capacity(structural + row_count);

        for (index, variable) in model.variables().iter().enumerate() {
            columns.push(
                model
                    .matrix()
                    .column(VarId(index as u32))
                    .map(|(row, value)| (row.0, value.clone()))
                    .collect(),
            );
            lower.push(variable.lower.clone());
            upper.push(variable.upper.clone());
            cost.push(variable.objective.clone());
            integral.push(variable.integral);
        }
        for (index, row) in model.rows().iter().enumerate() {
            // The logical of a row is its activity, entering the system with a
            // minus sign so that `A x - s = 0` and the row's own bounds sit on
            // `s`.
            columns.push(vec![(index as u32, Rational::from_i64(-1))]);
            lower.push(row.lower.clone());
            upper.push(row.upper.clone());
            cost.push(Rational::zero());
            // A logical variable is a row's activity. Declaring it integer
            // would be a claim about the row that the model never made.
            integral.push(false);
        }

        Self {
            row_count,
            structural,
            columns,
            lower,
            upper,
            cost,
            integral,
            constant: model.objective_constant().clone(),
            digest: *model.digest(),
        }
    }

    /// The number of rows.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.row_count
    }

    /// The number of columns from the original model.
    #[must_use]
    pub fn structural(&self) -> usize {
        self.structural
    }

    /// The total number of columns.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.columns.len()
    }

    /// One column's rows and exact coefficients.
    #[must_use]
    pub fn column(&self, index: usize) -> &[(u32, Rational)] {
        &self.columns[index]
    }

    /// A column's exact lower bound, absent when it is unbounded below.
    #[must_use]
    pub fn lower(&self, index: usize) -> Option<&Rational> {
        self.lower[index].as_ref()
    }

    /// A column's exact upper bound, absent when it is unbounded above.
    #[must_use]
    pub fn upper(&self, index: usize) -> Option<&Rational> {
        self.upper[index].as_ref()
    }

    /// One extra row of the exact view: its terms and its two optional sides.
    #[must_use]
    pub fn extra_row(
        terms: Vec<(usize, Rational)>,
        low: Option<Rational>,
        high: Option<Rational>,
    ) -> ExactRow {
        ExactRow { terms, low, high }
    }

    /// A copy with extra rows appended, exactly.
    ///
    /// The exact counterpart of the working copy's augmentation, and it exists
    /// for the same reason: the two must agree about what problem a node is
    /// solving. A cut is a row of the relaxation and never a row of the model,
    /// so nothing already numbered moves.
    #[must_use]
    pub fn with_rows(&self, extra: &[ExactRow]) -> Self {
        let mut copy = self.clone();
        for ExactRow { terms, low, high } in extra {
            let row = copy.row_count;
            copy.row_count += 1;
            for (column, value) in terms {
                if *column >= copy.structural || value.is_zero() {
                    continue;
                }
                copy.columns[*column].push((row as u32, value.clone()));
                copy.columns[*column].sort_by_key(|(at, _)| *at);
            }
            // The logical column for the new row enters with a minus sign, as
            // every logical does, and carries the row's own bounds.
            copy.columns
                .push(vec![(row as u32, Rational::from_i64(-1))]);
            copy.lower.push(low.clone());
            copy.upper.push(high.clone());
            copy.cost.push(Rational::zero());
            copy.integral.push(false);
        }
        copy
    }

    /// A copy with some columns' bounds narrowed, exactly.
    ///
    /// The exact counterpart of the working copy's narrowing, and it exists
    /// for the same reason: a search node differs from its parent by tighter
    /// bounds and by nothing else. The two must agree about what a node's
    /// region is, or the float search and the exact rebuild would be working
    /// on different subproblems.
    ///
    /// Narrowing only, again. A request that would widen is ignored, because a
    /// node able to relax its parent's assumptions would break the property
    /// that the children partition the parent.
    #[must_use]
    pub fn narrowed(&self, bounds: &[(usize, Option<Rational>, Option<Rational>)]) -> Self {
        let mut copy = self.clone();
        for (column, low, high) in bounds {
            if *column >= copy.lower.len() {
                continue;
            }
            if let Some(value) = low {
                let tighter = match &copy.lower[*column] {
                    None => true,
                    Some(seen) => value.cmp_value(seen).is_gt(),
                };
                if tighter {
                    copy.lower[*column] = Some(value.clone());
                }
            }
            if let Some(value) = high {
                let tighter = match &copy.upper[*column] {
                    None => true,
                    Some(seen) => value.cmp_value(seen).is_lt(),
                };
                if tighter {
                    copy.upper[*column] = Some(value.clone());
                }
            }
        }
        copy
    }

    /// Whether the model declares a column integer.
    ///
    /// Only structural columns can be, and this answers for every index so a
    /// caller never has to remember which half of the vector it is looking at.
    #[must_use]
    pub fn is_integral(&self, index: usize) -> bool {
        self.integral[index]
    }

    /// A column's exact objective coefficient.
    #[must_use]
    pub fn cost(&self, index: usize) -> &Rational {
        &self.cost[index]
    }

    /// The exact objective constant.
    #[must_use]
    pub fn constant(&self) -> &Rational {
        &self.constant
    }

    /// The digest of the instance this was built from.
    #[must_use]
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    /// Whether a column is one of the model's own.
    #[must_use]
    pub fn is_structural(&self, index: usize) -> bool {
        index < self.structural
    }

    /// The row a logical column stands for.
    #[must_use]
    pub fn row_of(&self, index: usize) -> Option<RowId> {
        if self.is_structural(index) {
            None
        } else {
            Some(RowId((index - self.structural) as u32))
        }
    }

    /// The exact objective at a point over every column.
    ///
    /// # Panics
    ///
    /// Panics when the point is not the column count.
    #[must_use]
    pub fn objective_at(&self, point: &[Rational]) -> Rational {
        assert_eq!(
            point.len(),
            self.columns(),
            "point length differs from the column count"
        );
        let mut total = self.constant.clone();
        for (cost, value) in self.cost.iter().zip(point) {
            if cost.is_zero() || value.is_zero() {
                continue;
            }
            total = total.add(&cost.mul(value));
        }
        total
    }

    /// The exact residual of `A x` at a point, one entry per row.
    ///
    /// Zero everywhere is what it means for a point to satisfy the system.
    ///
    /// # Panics
    ///
    /// Panics when the point is not the column count.
    #[must_use]
    pub fn residual_at(&self, point: &[Rational]) -> Vec<Rational> {
        assert_eq!(
            point.len(),
            self.columns(),
            "point length differs from the column count"
        );
        let mut residual = vec![Rational::zero(); self.row_count];
        for (index, value) in point.iter().enumerate() {
            if value.is_zero() {
                continue;
            }
            for (row, coefficient) in &self.columns[index] {
                let slot = &mut residual[*row as usize];
                *slot = slot.add(&coefficient.mul(value));
            }
        }
        residual
    }

    /// Every column whose value falls outside its bounds, exactly.
    ///
    /// Exactly, with no fence anywhere. A point is feasible or it is not, and
    /// a search tolerance has no business in the answer.
    ///
    /// # Panics
    ///
    /// Panics when the point is not the column count.
    #[must_use]
    pub fn violations(&self, point: &[Rational]) -> Vec<usize> {
        assert_eq!(
            point.len(),
            self.columns(),
            "point length differs from the column count"
        );
        (0..self.columns())
            .filter(|index| {
                let value = &point[*index];
                self.lower[*index]
                    .as_ref()
                    .is_some_and(|bound| value < bound)
                    || self.upper[*index]
                        .as_ref()
                        .is_some_and(|bound| value > bound)
            })
            .collect()
    }

    /// The structural half of a point, which is the point in the model's own
    /// coordinates.
    #[must_use]
    pub fn structural_part<'a>(&self, point: &'a [Rational]) -> &'a [Rational] {
        &point[..self.structural]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_exact::BigInt;

    fn model(source: &str) -> Model {
        hc_io::mps::read(source.as_bytes(), hc_io::mps::Form::Free).expect("accepted")
    }

    fn number(value: i64) -> Rational {
        Rational::from_i64(value)
    }

    fn ratio(top: i64, bottom: i64) -> Rational {
        Rational::new(BigInt::from_i64(top), BigInt::from_i64(bottom)).expect("nonzero")
    }

    fn sample() -> Model {
        model(concat!(
            "ROWS\n N obj\n L r1\n G r2\n",
            "COLUMNS\n x obj 2 r1 1\n x r2 1\n y obj -1 r1 3\n",
            "RHS\n rhs r1 6 r2 1\n rhs obj 4\n",
            "BOUNDS\n UP b x 4\n",
            "ENDATA\n"
        ))
    }

    #[test]
    fn logical_columns_carry_their_rows_bounds_and_a_minus_one() {
        let canonical = Canonical::of(&sample());
        assert_eq!(canonical.rows(), 2);
        assert_eq!(canonical.structural(), 2);
        assert_eq!(canonical.columns(), 4);
        assert_eq!(canonical.column(2), &[(0u32, number(-1))]);
        assert_eq!(canonical.column(3), &[(1u32, number(-1))]);
        assert_eq!(canonical.upper(2), Some(&number(6)));
        assert_eq!(canonical.lower(2), None);
        assert_eq!(canonical.lower(3), Some(&number(1)));
        assert_eq!(canonical.upper(3), None);
        assert!(canonical.is_structural(0));
        assert!(!canonical.is_structural(2));
        assert_eq!(canonical.row_of(3), Some(RowId(1)));
        assert_eq!(canonical.row_of(0), None);
    }

    #[test]
    fn the_objective_carries_the_constant_and_the_logicals_cost_nothing() {
        let canonical = Canonical::of(&sample());
        assert_eq!(canonical.constant().to_canonical_string(), "-4");
        assert_eq!(canonical.cost(2), &Rational::zero());
        let point = vec![number(3), ratio(1, 3), number(4), number(3)];
        // -4 + 2*3 + (-1)*(1/3) = 2 - 1/3.
        assert_eq!(canonical.objective_at(&point).to_canonical_string(), "5/3");
    }

    #[test]
    fn a_point_that_solves_the_system_has_an_exactly_zero_residual() {
        let canonical = Canonical::of(&sample());
        let x = ratio(1, 3);
        let y = ratio(2, 7);
        // Row one: x + 3y. Row two: x.
        let first = x.add(&y.mul(&number(3)));
        let second = x.clone();
        let point = vec![x, y, first, second];
        let residual = canonical.residual_at(&point);
        assert!(
            residual.iter().all(hc_exact::Rational::is_zero),
            "{residual:?}"
        );

        // And a point that misses says which row, exactly.
        let mut wrong = point;
        wrong[2] = wrong[2].add(&ratio(1, 1_000_000_000));
        let residual = canonical.residual_at(&wrong);
        assert!(!residual[0].is_zero());
        assert!(residual[1].is_zero());
    }

    #[test]
    fn violations_are_decided_exactly_and_not_within_a_fence() {
        let canonical = Canonical::of(&sample());
        let inside = vec![number(4), number(0), number(4), number(1)];
        assert!(canonical.violations(&inside).is_empty());

        // One part in a billion outside an upper bound is outside it. A search
        // fence would call this feasible; a proof may not.
        let outside = vec![
            number(4).add(&ratio(1, 1_000_000_000)),
            number(0),
            number(4),
            number(1),
        ];
        assert_eq!(canonical.violations(&outside), vec![0]);

        let below = vec![number(4), number(0), number(4), number(0)];
        assert_eq!(canonical.violations(&below), vec![3]);
    }

    #[test]
    fn the_structural_part_is_the_point_in_the_models_own_coordinates() {
        let canonical = Canonical::of(&sample());
        let point = vec![number(1), number(2), number(3), number(4)];
        assert_eq!(canonical.structural_part(&point), &[number(1), number(2)]);
    }

    #[test]
    fn the_instance_digest_comes_across_untouched() {
        let model = sample();
        assert_eq!(Canonical::of(&model).digest(), model.digest());
    }

    #[test]
    fn a_model_with_no_rows_produces_no_logical_columns() {
        let empty = model("ROWS\n N obj\nCOLUMNS\n x obj 1\nENDATA\n");
        let canonical = Canonical::of(&empty);
        assert_eq!(canonical.rows(), 0);
        assert_eq!(canonical.columns(), 1);
        assert!(canonical.residual_at(&[number(5)]).is_empty());
    }
}
