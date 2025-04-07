//! Choosing what to split on.
//!
//! Branching decides how big the tree gets and decides nothing about what the
//! tree proves. Both halves of that matter. A better rule closes the search
//! sooner; a rule that were somehow *wrong* would still produce a valid proof,
//! because the two children of any integer split cover their parent whatever
//! variable was chosen. So this file may be heuristic throughout, and it is —
//! and the campaign that runs the search under both rules asserts they reach
//! the same answers.
//!
//! Two rules. Most-fractional picks the variable furthest from an integer,
//! which is the textbook choice and the one to teach with. Reliability
//! pseudocost estimates how much each variable's two sides would degrade the
//! bound, measures that directly by trial solves while an estimate is still
//! unreliable, and thereafter trusts the running average.
//!
//! Everything about the tie-breaking is fixed, because the tree has to be
//! reproducible even though its shape is not load-bearing. Larger score first,
//! then lowest column index, and no floating comparison decides anything except
//! which of two legal trees gets built.

use hc_exact::Rational;
use hc_lp::exact::Canonical;

/// Which rule chooses the branching variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Branching {
    /// The variable furthest from an integer. The textbook rule.
    MostFractional,
    /// Pseudocosts, initialised by trial solves while they are unreliable.
    ReliabilityPseudocost,
}

/// How many observations a variable's estimate needs before it is trusted.
pub const RELIABILITY: u64 = 4;
/// How many candidates a node may spend trial solves on.
pub const STRONG_CANDIDATES: usize = 8;
/// How many simplex iterations one trial solve may take.
pub const STRONG_ITERATIONS: u64 = 100;

/// The running estimate of what splitting a variable costs.
///
/// One average per variable per side, kept as a sum and a count so that a
/// variable nobody has branched on yet is distinguishable from one whose
/// branches happened to cost nothing. The distinction matters: an average of
/// zero is a measurement and an absent average is not.
#[derive(Clone, Debug, Default)]
pub struct Pseudocosts {
    down: Vec<(f64, u64)>,
    up: Vec<(f64, u64)>,
}

impl Pseudocosts {
    /// Fresh estimates over a column count.
    #[must_use]
    pub fn new(columns: usize) -> Self {
        Self {
            down: vec![(0.0, 0); columns],
            up: vec![(0.0, 0); columns],
        }
    }

    /// Records what one branch actually cost.
    ///
    /// `degradation` is how much the child's bound rose above its parent's and
    /// `fraction` is how far the variable had to move to get there. A move of
    /// zero teaches nothing and is discarded rather than divided by, which is
    /// the whole of the trap this guards.
    pub fn observe(&mut self, column: usize, up: bool, degradation: f64, fraction: f64) {
        if fraction <= 0.0 || !degradation.is_finite() || degradation < 0.0 {
            return;
        }
        let side = if up { &mut self.up } else { &mut self.down };
        let Some(slot) = side.get_mut(column) else {
            return;
        };
        slot.0 += degradation / fraction;
        slot.1 += 1;
    }

    /// How many observations a variable's two sides have between them.
    #[must_use]
    pub fn observations(&self, column: usize) -> u64 {
        let down = self.down.get(column).map_or(0, |slot| slot.1);
        let up = self.up.get(column).map_or(0, |slot| slot.1);
        down.min(up)
    }

    /// Whether a variable's estimate has been measured often enough.
    #[must_use]
    pub fn reliable(&self, column: usize, threshold: u64) -> bool {
        self.observations(column) >= threshold
    }

    /// The average cost of one side, or `None` when nothing has been seen.
    #[must_use]
    pub fn average(&self, column: usize, up: bool) -> Option<f64> {
        let side = if up { &self.up } else { &self.down };
        let (total, count) = *side.get(column)?;
        (count > 0).then(|| total / count as f64)
    }

    /// The score of splitting a variable whose value has this fractional part.
    ///
    /// The product of the two estimated degradations, which is the ordinary
    /// choice and prefers a variable that costs something on both sides over
    /// one that costs a great deal on one and nothing on the other. A side
    /// with no measurement contributes a unit, so an unmeasured variable
    /// scores by the side that has been measured; a side measured to cost
    /// nothing contributes zero, which is what it was measured to be. A
    /// variable that scores zero is still a legal split and is still chosen
    /// when nothing scores higher.
    #[must_use]
    pub fn score(&self, column: usize, fraction: f64) -> f64 {
        let down = self.average(column, false).unwrap_or(1.0) * fraction;
        let up = self.average(column, true).unwrap_or(1.0) * (1.0 - fraction);
        down * up
    }
}

/// One fractional integer column, and where it would split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// The column.
    pub column: usize,
    /// The integer the split is taken at.
    pub at: Rational,
    /// How far the value sits above that integer, as text for exactness.
    pub fraction: Rational,
}

/// Every integer column the point is fractional on, in index order.
#[must_use]
pub fn candidates(canonical: &Canonical, point: &[Rational]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (column, value) in point.iter().enumerate() {
        if !canonical.is_integral(column) || value.is_integer() {
            continue;
        }
        let floor = Rational::from_int(value.floor());
        let fraction = value.sub(&floor);
        out.push(Candidate {
            column,
            at: floor,
            fraction,
        });
    }
    out
}

/// The candidate the most-fractional rule picks.
///
/// The one furthest from the nearer integer, with the lowest index breaking
/// ties.
#[must_use]
pub fn most_fractional(list: &[Candidate]) -> Option<&Candidate> {
    let half = Rational::from_i64(2).recip().expect("two is not zero");
    let mut best: Option<(&Candidate, Rational)> = None;
    for candidate in list {
        let distance = if candidate.fraction.cmp_value(&half).is_gt() {
            Rational::one().sub(&candidate.fraction)
        } else {
            candidate.fraction.clone()
        };
        let better = best
            .as_ref()
            .is_none_or(|(_, seen)| distance.cmp_value(seen).is_gt());
        if better {
            best = Some((candidate, distance));
        }
    }
    best.map(|(candidate, _)| candidate)
}

/// The candidate the pseudocost rule picks, given a way to measure one.
///
/// `measure` is asked to evaluate a candidate whose estimate is not yet
/// reliable; it returns the two degradations it observed, or `None` when it
/// declined. The caller supplies it because measuring means solving two child
/// relaxations, and that is the search's business rather than this file's.
pub fn by_pseudocost(
    list: &[Candidate],
    costs: &mut Pseudocosts,
    measure: &mut dyn FnMut(&Candidate) -> Option<(f64, f64)>,
) -> Option<Candidate> {
    // Trial solves are spent on the least-measured candidates first, and only
    // on a bounded number of them, because strong branching is a search that
    // buys information with the same currency the search itself spends.
    let mut unreliable: Vec<&Candidate> = list
        .iter()
        .filter(|candidate| !costs.reliable(candidate.column, RELIABILITY))
        .collect();
    unreliable.sort_by_key(|candidate| (costs.observations(candidate.column), candidate.column));
    for candidate in unreliable.into_iter().take(STRONG_CANDIDATES) {
        if let Some((down, up)) = measure(candidate) {
            let fraction = to_f64(&candidate.fraction);
            costs.observe(candidate.column, false, down, fraction);
            costs.observe(candidate.column, true, up, 1.0 - fraction);
        }
    }

    let mut best: Option<(&Candidate, f64)> = None;
    for candidate in list {
        let score = costs.score(candidate.column, to_f64(&candidate.fraction));
        let better = best.as_ref().is_none_or(|(_, seen)| score > *seen);
        if better {
            best = Some((candidate, score));
        }
    }
    best.map(|(candidate, _)| candidate.clone())
}

/// A rational as a machine float, for the estimates alone.
///
/// Nothing this value touches reaches a conclusion. It orders candidates and
/// it stops there.
#[must_use]
pub fn to_f64(value: &Rational) -> f64 {
    value.to_binary64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ratio(top: i64, bottom: i64) -> Rational {
        Rational::new(
            hc_exact::BigInt::from_i64(top),
            hc_exact::BigInt::from_i64(bottom),
        )
        .expect("a nonzero denominator")
    }

    fn candidate(column: usize, top: i64, bottom: i64) -> Candidate {
        Candidate {
            column,
            at: Rational::zero(),
            fraction: ratio(top, bottom),
        }
    }

    #[test]
    fn trap_pseudocost_div_zero() {
        let mut costs = Pseudocosts::new(3);
        // A branch that moved nothing teaches nothing, and dividing by it
        // would produce an infinity that then wins every comparison forever.
        costs.observe(0, true, 5.0, 0.0);
        assert_eq!(costs.average(0, true), None);
        assert_eq!(costs.observations(0), 0);

        // A branch that cost nothing is a measurement and is kept.
        costs.observe(1, true, 0.0, 0.5);
        assert_eq!(costs.average(1, true), Some(0.0));
        assert_eq!(costs.observations(1), 0, "one side is not both sides");
        costs.observe(1, false, 0.0, 0.5);
        assert_eq!(costs.observations(1), 1);

        // Every score is finite whatever has or has not been measured, and a
        // variable measured to cost nothing scores below one measured to cost
        // something rather than tying with it at an infinity.
        for column in 0..3 {
            let score = costs.score(column, 0.5);
            assert!(score.is_finite(), "column {column} scored {score}");
        }
        assert!(
            costs.score(1, 0.5) < costs.score(2, 0.5),
            "a variable measured to degrade nothing should sort below an unmeasured one"
        );
    }

    #[test]
    fn reliability_counts_the_thinner_of_the_two_sides() {
        let mut costs = Pseudocosts::new(1);
        for _ in 0..10 {
            costs.observe(0, true, 1.0, 0.5);
        }
        assert!(
            !costs.reliable(0, RELIABILITY),
            "ten measurements of one side do not make a variable reliable"
        );
        for _ in 0..RELIABILITY {
            costs.observe(0, false, 1.0, 0.5);
        }
        assert!(costs.reliable(0, RELIABILITY));
    }

    #[test]
    fn the_most_fractional_rule_takes_the_furthest_from_an_integer() {
        let list = vec![
            candidate(0, 1, 10),
            candidate(1, 4, 10),
            candidate(2, 9, 10),
        ];
        assert_eq!(most_fractional(&list).map(|c| c.column), Some(1));
        assert_eq!(most_fractional(&[]), None);
    }

    #[test]
    fn ties_go_to_the_lowest_index() {
        let list = vec![candidate(3, 1, 2), candidate(1, 1, 2)];
        // Both are exactly one half away, so the first one seen wins and the
        // list is in index order.
        assert_eq!(most_fractional(&list).map(|c| c.column), Some(3));
    }

    #[test]
    fn measurement_is_spent_on_the_least_measured_candidates_first() {
        let mut costs = Pseudocosts::new(4);
        for _ in 0..RELIABILITY {
            costs.observe(0, true, 1.0, 0.5);
            costs.observe(0, false, 1.0, 0.5);
        }
        let list: Vec<Candidate> = (0..4).map(|column| candidate(column, 1, 2)).collect();

        let mut asked: Vec<usize> = Vec::new();
        let chosen = by_pseudocost(&list, &mut costs, &mut |candidate| {
            asked.push(candidate.column);
            Some((2.0, 2.0))
        });
        assert!(chosen.is_some());
        assert!(
            !asked.contains(&0),
            "a reliable variable should not be measured again"
        );
        assert_eq!(asked, vec![1, 2, 3]);
    }
}
