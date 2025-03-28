//! Sparse vectors and the accumulator every kernel writes through.
//!
//! Two representations, used for different halves of the same work. A
//! [`SparseVec`] is a compressed pair of arrays with strictly ascending
//! indices — cheap to store, cheap to iterate, and awkward to add into. A
//! [`Workspace`] is a dense scratch array paired with a list of the positions
//! that have been touched, which is the opposite trade: adding is a single
//! indexed write, and reading the result back costs a sort.
//!
//! The pairing is what makes a sparse elimination affordable. A column is
//! scattered into the workspace once, updated by however many operations the
//! step needs, and gathered back once — rather than merged repeatedly, which
//! is where a naive implementation spends all of its time.
//!
//! Gathering drops entries below a named fence. That fence is a policy about
//! what counts as structurally zero, and it is passed in rather than read from
//! a global so that the policy is visible in the signature of every call that
//! applies it.

use hc_core::Tol;

/// A sparse vector: strictly ascending indices with their values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SparseVec {
    indices: Vec<u32>,
    values: Vec<f64>,
}

impl SparseVec {
    /// An empty vector.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            indices: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Builds a vector from ascending indices and their values.
    ///
    /// # Panics
    ///
    /// Panics when the lengths differ or the indices are not strictly
    /// ascending. Both are invariants of the caller rather than properties of
    /// an input, so breaking one is a defect and says so.
    #[must_use]
    pub fn from_sorted(indices: Vec<u32>, values: Vec<f64>) -> Self {
        assert_eq!(indices.len(), values.len(), "index and value counts differ");
        for pair in indices.windows(2) {
            assert!(
                pair[0] < pair[1],
                "sparse indices are not strictly ascending"
            );
        }
        Self { indices, values }
    }

    /// The stored indices.
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// The stored values.
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// The number of stored entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// Whether nothing is stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// The stored entries, in index order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, f64)> {
        self.indices
            .iter()
            .copied()
            .zip(self.values.iter().copied())
    }

    /// The value at a position, when one is stored there.
    #[must_use]
    pub fn get(&self, index: u32) -> Option<f64> {
        self.indices
            .binary_search(&index)
            .ok()
            .map(|at| self.values[at])
    }

    /// The inner product with a dense vector.
    ///
    /// # Panics
    ///
    /// Panics when an index falls outside the dense vector.
    #[must_use]
    pub fn dot_dense(&self, dense: &[f64]) -> f64 {
        let mut total = 0.0;
        for (index, value) in self.iter() {
            total += value * dense[index as usize];
        }
        total
    }

    /// The largest absolute value stored, or zero when nothing is.
    #[must_use]
    pub fn norm_infinity(&self) -> f64 {
        self.values
            .iter()
            .fold(0.0f64, |largest, value| largest.max(value.abs()))
    }

    /// Multiplies every stored value in place.
    pub fn scale(&mut self, factor: f64) {
        for value in &mut self.values {
            *value *= factor;
        }
    }

    /// Drops entries whose magnitude is below the structural-zero fence.
    ///
    /// A stored zero is not wrong, it is expensive: it occupies a position in
    /// every later operation and contributes nothing. Dropping is a size
    /// decision and never a numerical one — the value discarded is below the
    /// fence that defines what the search is willing to treat as absent.
    pub fn prune(&mut self, tol: Tol) {
        let mut keep = 0usize;
        for at in 0..self.indices.len() {
            if self.values[at].abs() <= tol.zero {
                continue;
            }
            self.indices[keep] = self.indices[at];
            self.values[keep] = self.values[at];
            keep += 1;
        }
        self.indices.truncate(keep);
        self.values.truncate(keep);
    }
}

/// A dense accumulator with a record of which positions were touched.
///
/// The record is what makes clearing cheap: a workspace over ten thousand
/// positions that touched three of them resets three, not ten thousand.
#[derive(Clone, Debug)]
pub struct Workspace {
    dense: Vec<f64>,
    touched: Vec<u32>,
    marked: Vec<bool>,
}

impl Workspace {
    /// A workspace over a fixed number of positions.
    #[must_use]
    pub fn with_len(len: usize) -> Self {
        Self {
            dense: vec![0.0; len],
            touched: Vec::new(),
            marked: vec![false; len],
        }
    }

    /// The number of positions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.dense.len()
    }

    /// Whether the workspace has no positions at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dense.is_empty()
    }

    /// How many positions currently hold something.
    #[must_use]
    pub fn touched(&self) -> usize {
        self.touched.len()
    }

    /// The value at a position.
    ///
    /// # Panics
    ///
    /// Panics when the position is outside the workspace.
    #[must_use]
    pub fn value(&self, index: u32) -> f64 {
        self.dense[index as usize]
    }

    /// Adds a value at a position.
    ///
    /// # Panics
    ///
    /// Panics when the position is outside the workspace.
    pub fn add_at(&mut self, index: u32, value: f64) {
        let at = index as usize;
        if !self.marked[at] {
            self.marked[at] = true;
            self.touched.push(index);
        }
        self.dense[at] += value;
    }

    /// Replaces the value at a position.
    ///
    /// # Panics
    ///
    /// Panics when the position is outside the workspace.
    pub fn set_at(&mut self, index: u32, value: f64) {
        let at = index as usize;
        if !self.marked[at] {
            self.marked[at] = true;
            self.touched.push(index);
        }
        self.dense[at] = value;
    }

    /// Adds a sparse vector.
    pub fn scatter(&mut self, vector: &SparseVec) {
        for (index, value) in vector.iter() {
            self.add_at(index, value);
        }
    }

    /// Adds a multiple of a sparse vector.
    ///
    /// This is the operation an elimination step is made of, and it is the
    /// only place the working vector grows: a position that was structurally
    /// absent becomes present the moment a multiple lands on it.
    pub fn axpy(&mut self, factor: f64, vector: &SparseVec) {
        if factor == 0.0 {
            return;
        }
        for (index, value) in vector.iter() {
            self.add_at(index, factor * value);
        }
    }

    /// Reads the accumulated vector back out and clears the workspace.
    ///
    /// Entries at or below the structural-zero fence are dropped. Cancellation
    /// is the ordinary reason a touched position holds nothing: two multiples
    /// landed on it and undid each other, and carrying the result forward as a
    /// stored zero would make every later operation pay for it.
    pub fn gather(&mut self, tol: Tol) -> SparseVec {
        self.touched.sort_unstable();
        let mut indices = Vec::with_capacity(self.touched.len());
        let mut values = Vec::with_capacity(self.touched.len());
        for &index in &self.touched {
            let at = index as usize;
            let value = self.dense[at];
            self.dense[at] = 0.0;
            self.marked[at] = false;
            if value.abs() <= tol.zero {
                continue;
            }
            indices.push(index);
            values.push(value);
        }
        self.touched.clear();
        SparseVec { indices, values }
    }

    /// Reads the accumulated vector into a dense slice and clears the
    /// workspace.
    ///
    /// # Panics
    ///
    /// Panics when the slice is not the workspace's length.
    pub fn gather_dense(&mut self, into: &mut [f64]) {
        assert_eq!(
            into.len(),
            self.dense.len(),
            "destination length differs from the workspace"
        );
        into.fill(0.0);
        for &index in &self.touched {
            let at = index as usize;
            into[at] = self.dense[at];
            self.dense[at] = 0.0;
            self.marked[at] = false;
        }
        self.touched.clear();
    }

    /// Discards everything without reading it.
    pub fn clear(&mut self) {
        for &index in &self.touched {
            let at = index as usize;
            self.dense[at] = 0.0;
            self.marked[at] = false;
        }
        self.touched.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tol() -> Tol {
        Tol::solver()
    }

    fn vector(entries: &[(u32, f64)]) -> SparseVec {
        SparseVec::from_sorted(
            entries.iter().map(|(index, _)| *index).collect(),
            entries.iter().map(|(_, value)| *value).collect(),
        )
    }

    #[test]
    fn an_empty_vector_is_empty_in_every_way() {
        let empty = SparseVec::new();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert_eq!(empty.iter().count(), 0);
        assert_eq!(empty.get(0), None);
        assert_eq!(empty.dot_dense(&[1.0, 2.0]), 0.0);
        assert_eq!(empty.norm_infinity(), 0.0);
    }

    #[test]
    fn entries_are_found_by_position_and_absences_reported() {
        let vector = vector(&[(1, 2.0), (4, -3.0), (9, 0.5)]);
        assert_eq!(vector.get(1), Some(2.0));
        assert_eq!(vector.get(4), Some(-3.0));
        assert_eq!(vector.get(0), None);
        assert_eq!(vector.get(5), None);
        assert_eq!(vector.get(10), None);
        assert_eq!(vector.len(), 3);
        assert_eq!(vector.norm_infinity(), 3.0);
    }

    #[test]
    #[should_panic(expected = "strictly ascending")]
    fn unsorted_indices_are_a_defect_and_say_so() {
        let _ = SparseVec::from_sorted(vec![3, 1], vec![1.0, 1.0]);
    }

    #[test]
    #[should_panic(expected = "strictly ascending")]
    fn a_repeated_index_is_a_defect_too() {
        let _ = SparseVec::from_sorted(vec![1, 1], vec![1.0, 1.0]);
    }

    #[test]
    fn the_inner_product_touches_only_stored_positions() {
        let vector = vector(&[(0, 2.0), (3, -1.0)]);
        assert_eq!(vector.dot_dense(&[1.0, 100.0, 100.0, 5.0]), 2.0 - 5.0);
    }

    #[test]
    fn scattering_and_gathering_is_the_identity_on_a_pruned_vector() {
        let original = vector(&[(0, 1.0), (2, -2.0), (7, 0.25)]);
        let mut workspace = Workspace::with_len(8);
        workspace.scatter(&original);
        assert_eq!(workspace.touched(), 3);
        let recovered = workspace.gather(tol());
        assert_eq!(recovered, original);
        assert_eq!(workspace.touched(), 0);
    }

    #[test]
    fn an_elimination_step_reads_as_one_multiply_and_add() {
        // The row being eliminated, and the pivot row scaled to cancel its
        // leading entry. This is the whole of a sparse elimination.
        let target = vector(&[(0, 2.0), (1, 1.0), (5, 3.0)]);
        let pivot = vector(&[(0, 1.0), (2, 4.0)]);
        let mut workspace = Workspace::with_len(8);
        workspace.scatter(&target);
        workspace.axpy(-2.0, &pivot);
        let result = workspace.gather(tol());
        assert_eq!(result, vector(&[(1, 1.0), (2, -8.0), (5, 3.0)]));
    }

    #[test]
    fn cancellation_removes_a_position_rather_than_storing_a_zero() {
        let left = vector(&[(0, 1.0), (3, 2.0)]);
        let mut workspace = Workspace::with_len(4);
        workspace.scatter(&left);
        workspace.axpy(-1.0, &left);
        let result = workspace.gather(tol());
        assert!(result.is_empty(), "a cancelled position was stored anyway");
    }

    #[test]
    fn a_value_below_the_fence_is_dropped_and_one_above_it_is_kept() {
        let mut workspace = Workspace::with_len(4);
        workspace.set_at(0, tol().zero / 2.0);
        workspace.set_at(1, tol().zero * 10.0);
        let result = workspace.gather(tol());
        assert_eq!(result.indices(), &[1]);
    }

    #[test]
    fn a_multiple_of_zero_touches_nothing() {
        let mut workspace = Workspace::with_len(4);
        workspace.axpy(0.0, &vector(&[(0, 1.0), (1, 1.0)]));
        assert_eq!(workspace.touched(), 0);
        assert!(workspace.gather(tol()).is_empty());
    }

    #[test]
    fn gathering_returns_positions_in_index_order_however_they_arrived() {
        let mut workspace = Workspace::with_len(16);
        for index in [9u32, 2, 15, 0, 7] {
            workspace.add_at(index, f64::from(index) + 1.0);
        }
        let result = workspace.gather(tol());
        assert_eq!(result.indices(), &[0, 2, 7, 9, 15]);
        assert_eq!(result.values(), &[1.0, 3.0, 8.0, 10.0, 16.0]);
    }

    #[test]
    fn repeated_additions_at_one_position_accumulate_once() {
        let mut workspace = Workspace::with_len(4);
        workspace.add_at(2, 1.0);
        workspace.add_at(2, 2.0);
        workspace.add_at(2, -0.5);
        assert_eq!(workspace.touched(), 1);
        assert_eq!(workspace.value(2), 2.5);
        assert_eq!(workspace.gather(tol()), vector(&[(2, 2.5)]));
    }

    #[test]
    fn clearing_resets_only_what_was_touched() {
        let mut workspace = Workspace::with_len(1000);
        workspace.add_at(500, 1.0);
        workspace.clear();
        assert_eq!(workspace.touched(), 0);
        assert_eq!(workspace.value(500), 0.0);
        // And the workspace is reusable afterwards, which is the property the
        // touched list exists to make cheap.
        workspace.add_at(500, 2.0);
        assert_eq!(workspace.value(500), 2.0);
    }

    #[test]
    fn a_dense_read_back_fills_every_position() {
        let mut workspace = Workspace::with_len(4);
        workspace.scatter(&vector(&[(1, 5.0), (3, -1.0)]));
        let mut dense = [9.0; 4];
        workspace.gather_dense(&mut dense);
        assert_eq!(dense, [0.0, 5.0, 0.0, -1.0]);
        assert_eq!(workspace.touched(), 0);
    }

    #[test]
    fn scaling_and_pruning_are_separate_decisions() {
        let mut vector = vector(&[(0, 1.0), (1, 2.0)]);
        vector.scale(-3.0);
        assert_eq!(vector.values(), &[-3.0, -6.0]);
        // Scaling to below the fence does not drop anything; pruning does.
        vector.scale(tol().zero / 10.0);
        assert_eq!(vector.len(), 2);
        vector.prune(tol());
        assert!(vector.is_empty());
    }

    #[test]
    fn a_workspace_over_no_positions_is_well_formed() {
        let mut workspace = Workspace::with_len(0);
        assert!(workspace.is_empty());
        assert_eq!(workspace.len(), 0);
        assert!(workspace.gather(tol()).is_empty());
    }
}
