//! The search tree, in the order a proof is written.
//!
//! Nodes are created parent before child and never reordered. That is not an
//! implementation preference: the certificate serializes this structure
//! directly, and a proof whose order had to be repaired afterwards would be
//! evidence that the search and the proof disagree about what happened.
//!
//! A node carries one assumption and a parent. Its full region is the
//! conjunction of the assumptions along the path back to the root, which is
//! computed rather than stored — storing it would be storing the same
//! information twice, and the two copies would eventually differ.

use hc_exact::Rational;

/// One branch assumption: a bound a node's region adds to its parent's.
///
/// The two children of a split carry `x <= k` and `x >= k + 1` for an integer
/// `k`, which are disjoint and together cover every integer point the parent
/// admitted. That is the whole of why a tree proves anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assumption {
    /// The column the assumption is about.
    pub column: usize,
    /// True for the at-most side, false for the at-least side.
    pub at_most: bool,
    /// The integer the assumption is made at: `k` on the left, `k + 1` on the
    /// right.
    pub value: Rational,
}

impl Assumption {
    /// The assumption written as `a . x >= b`, which is the form a proof
    /// cites.
    ///
    /// The at-most side is negated, exactly as a row's upper side is, so that
    /// there is one form and no convention for a reader to know.
    #[must_use]
    pub fn as_inequality(&self) -> (Rational, Rational) {
        if self.at_most {
            (Rational::from_i64(-1), self.value.neg())
        } else {
            (Rational::one(), self.value.clone())
        }
    }
}

/// One node of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The node's identifier, which is its position in creation order.
    pub id: u64,
    /// The node it was split from, absent only for the root.
    pub parent: Option<u64>,
    /// The assumption it adds to its parent's region.
    pub assumption: Option<Assumption>,
    /// How many splits separate it from the root.
    pub depth: u32,
}

/// The tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    entries: Vec<Entry>,
}

impl Tree {
    /// A tree holding only its root.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: vec![Entry {
                id: 0,
                parent: None,
                assumption: None,
                depth: 0,
            }],
        }
    }

    /// How many nodes exist.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the tree is empty, which it never is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The node an identifier names.
    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Entry> {
        usize::try_from(id).ok().and_then(|at| self.entries.get(at))
    }

    /// Every node, in creation order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Splits a node in two, returning the children left first.
    ///
    /// # Panics
    ///
    /// Panics when the parent does not exist. A split of a node that was never
    /// created is a broken invariant in the caller, not an input error.
    pub fn split(&mut self, parent: u64, column: usize, at: &Rational) -> (u64, u64) {
        let depth = self
            .get(parent)
            .unwrap_or_else(|| panic!("node {parent} does not exist"))
            .depth
            + 1;
        let left = self.entries.len() as u64;
        self.entries.push(Entry {
            id: left,
            parent: Some(parent),
            assumption: Some(Assumption {
                column,
                at_most: true,
                value: at.clone(),
            }),
            depth,
        });
        let right = self.entries.len() as u64;
        self.entries.push(Entry {
            id: right,
            parent: Some(parent),
            assumption: Some(Assumption {
                column,
                at_most: false,
                value: at.add(&Rational::one()),
            }),
            depth,
        });
        (left, right)
    }

    /// The assumptions on the path from the root to a node, root end first.
    #[must_use]
    pub fn path(&self, node: u64) -> Vec<&Assumption> {
        let mut out = Vec::new();
        let mut at = self.get(node);
        while let Some(entry) = at {
            if let Some(assumption) = &entry.assumption {
                out.push(assumption);
            }
            at = entry.parent.and_then(|parent| self.get(parent));
        }
        out.reverse();
        out
    }

    /// Whether one node is an ancestor of another, or the same node.
    ///
    /// This is what a cut's scope is checked against, and it is here rather
    /// than in the cut machinery because ancestry is a property of the tree.
    #[must_use]
    pub fn covers(&self, ancestor: u64, node: u64) -> bool {
        let mut at = Some(node);
        while let Some(current) = at {
            if current == ancestor {
                return true;
            }
            at = self.get(current).and_then(|entry| entry.parent);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn whole(value: i64) -> Rational {
        Rational::from_i64(value)
    }

    #[test]
    fn a_new_tree_holds_a_root_with_no_assumption() {
        let tree = Tree::new();
        assert_eq!(tree.len(), 1);
        let root = tree.get(0).expect("a root");
        assert_eq!(root.parent, None);
        assert_eq!(root.assumption, None);
        assert_eq!(root.depth, 0);
        assert!(tree.path(0).is_empty());
    }

    #[test]
    fn a_split_creates_two_children_that_partition_their_parent() {
        let mut tree = Tree::new();
        let (left, right) = tree.split(0, 2, &whole(3));
        assert_eq!((left, right), (1, 2));

        let left_side = tree.get(left).expect("a left child");
        let right_side = tree.get(right).expect("a right child");
        assert_eq!(
            left_side.assumption,
            Some(Assumption {
                column: 2,
                at_most: true,
                value: whole(3)
            })
        );
        assert_eq!(
            right_side.assumption,
            Some(Assumption {
                column: 2,
                at_most: false,
                value: whole(4)
            })
        );
        assert_eq!(left_side.depth, 1);
        assert_eq!(right_side.depth, 1);
    }

    #[test]
    fn the_two_sides_are_written_as_inequalities_of_one_form() {
        let mut tree = Tree::new();
        let (left, right) = tree.split(0, 0, &whole(3));
        let (coefficient, bound) = tree
            .get(left)
            .and_then(|entry| entry.assumption.as_ref())
            .expect("an assumption")
            .as_inequality();
        // x <= 3 becomes -x >= -3.
        assert_eq!(coefficient.to_canonical_string(), "-1");
        assert_eq!(bound.to_canonical_string(), "-3");

        let (coefficient, bound) = tree
            .get(right)
            .and_then(|entry| entry.assumption.as_ref())
            .expect("an assumption")
            .as_inequality();
        assert_eq!(coefficient.to_canonical_string(), "1");
        assert_eq!(bound.to_canonical_string(), "4");
    }

    #[test]
    fn a_path_reads_from_the_root_down() {
        let mut tree = Tree::new();
        let (left, _) = tree.split(0, 0, &whole(1));
        let (_, deep) = tree.split(left, 1, &whole(5));

        let path = tree.path(deep);
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].column, 0);
        assert!(path[0].at_most);
        assert_eq!(path[1].column, 1);
        assert!(!path[1].at_most);
    }

    #[test]
    fn ancestry_is_reflexive_and_does_not_reach_across_to_a_sibling() {
        let mut tree = Tree::new();
        let (left, right) = tree.split(0, 0, &whole(1));
        let (grandchild, _) = tree.split(left, 1, &whole(2));

        assert!(tree.covers(0, grandchild));
        assert!(tree.covers(left, grandchild));
        assert!(tree.covers(left, left));
        assert!(!tree.covers(right, grandchild));
        assert!(!tree.covers(grandchild, left));
    }

    #[test]
    #[should_panic(expected = "does not exist")]
    fn splitting_a_node_that_was_never_created_is_a_broken_invariant() {
        Tree::new().split(9, 0, &whole(1));
    }
}
