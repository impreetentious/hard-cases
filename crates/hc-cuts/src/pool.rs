//! The pool: which cuts exist, and where each of them may be used.
//!
//! A cut derived at a node is true of that node's region and of nothing
//! outside it. Using one in a sibling subtree is the failure that quietly
//! invalidates every descendant proof that cited it, and it is the failure a
//! pool makes easy: pools deduplicate, pools age entries out, pools promote
//! popular entries, and every one of those is a way for an inequality to
//! travel somewhere it is not true.
//!
//! So this pool does none of it. An entry carries the node it was derived at
//! and is offered at that node and its descendants, full stop. Deduplication
//! happens within an ancestry and never across one. There is no promotion
//! mechanism, not because none was wanted but because a promotion mechanism is
//! precisely the bug.

use hc_exact::Rational;

/// Which family a pooled cut came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// A Gomory mixed-integer cut.
    Gomory,
    /// A lifted knapsack cover cut.
    Cover,
}

/// A cut in the pool, with everything a certificate needs to record it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The node the cut was derived at, and the root of the subtree it may be
    /// used in.
    pub scope: u64,
    /// Which family it came from.
    pub family: Family,
    /// The derivation: for a Gomory cut, the combination and the isolated
    /// column; for a cover, the source row, the cover, and the lift order.
    pub derivation: Derivation,
    /// The cut's coefficients over the model's variables, ascending.
    pub coefficients: Vec<(usize, Rational)>,
    /// The right-hand side, in the canonical `a . x >= b` sense.
    pub right: Rational,
}

/// What a certificate records so a cut can be recomputed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Derivation {
    /// A combination of row equations and the integer column it isolates.
    Gomory {
        /// The multipliers over row indices, ascending.
        combination: Vec<(usize, Rational)>,
        /// The isolated integer column.
        isolated: usize,
    },
    /// A source row, a cover, and the order the lifting ran in.
    Cover {
        /// The source row.
        row: usize,
        /// The cover's members, ascending.
        members: Vec<usize>,
        /// The order the lifting ran in.
        order: Vec<usize>,
    },
}

/// The cuts a search has derived.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pool {
    entries: Vec<Entry>,
}

impl Pool {
    /// An empty pool.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Every entry, in the order they were derived.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// How many cuts the pool holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the pool is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Adds a cut, unless an entry with the same inequality is already visible
    /// in the same ancestry.
    ///
    /// Deduplication is deliberately narrow. Two identical inequalities
    /// derived in different subtrees are two cuts, and merging them would be
    /// promotion by another name.
    pub fn add(&mut self, entry: Entry, lineage: &[u64]) -> bool {
        let duplicate = self.entries.iter().any(|held| {
            held.coefficients == entry.coefficients
                && held.right == entry.right
                && lineage.contains(&held.scope)
        });
        if duplicate {
            return false;
        }
        self.entries.push(entry);
        true
    }

    /// The entries usable at a node, given its lineage from the root.
    ///
    /// `lineage` is the node and every ancestor of it. An entry is offered
    /// when its scope is one of them and never otherwise.
    #[must_use]
    pub fn visible(&self, lineage: &[u64]) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|entry| lineage.contains(&entry.scope))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(scope: u64, coefficient: i64) -> Entry {
        Entry {
            scope,
            family: Family::Gomory,
            derivation: Derivation::Gomory {
                combination: Vec::new(),
                isolated: 0,
            },
            coefficients: vec![(0, Rational::from_i64(coefficient))],
            right: Rational::zero(),
        }
    }

    #[test]
    fn trap_local_cut_escape() {
        let mut pool = Pool::new();
        // Node 1 and node 2 are siblings under the root.
        assert!(pool.add(entry(1, 1), &[1, 0]));
        assert!(pool.add(entry(0, 2), &[0]));

        // At node 1: its own cut and the root's.
        let here = pool.visible(&[1, 0]);
        assert_eq!(here.len(), 2);

        // At its sibling: only the root's. The cut derived at node 1 is true
        // of node 1's region and says nothing about node 2's.
        let there = pool.visible(&[2, 0]);
        assert_eq!(there.len(), 1);
        assert_eq!(there[0].scope, 0);

        // And at a node in neither lineage, nothing local at all.
        assert_eq!(pool.visible(&[9]).len(), 0);
    }

    #[test]
    fn a_duplicate_inside_one_ancestry_is_dropped_and_across_two_is_not() {
        let mut pool = Pool::new();
        assert!(pool.add(entry(0, 1), &[0]));
        // The same inequality again, at a node whose lineage includes the
        // root: already visible there, so it adds nothing.
        assert!(!pool.add(entry(1, 1), &[1, 0]));
        assert_eq!(pool.len(), 1);

        // The same inequality in a subtree that cannot see the first one is a
        // different cut, because it is true of a different region.
        let mut pool = Pool::new();
        assert!(pool.add(entry(1, 1), &[1, 0]));
        assert!(pool.add(entry(2, 1), &[2, 0]));
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn an_empty_pool_offers_nothing_and_says_so() {
        let pool = Pool::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
        assert!(pool.visible(&[0]).is_empty());
    }
}
