//! The tolerance vocabulary.
//!
//! These constants govern the *search* only. They decide when the solver is
//! willing to treat a floating-point quantity as good enough to keep looking
//! in a particular direction. They never decide whether a claim is true: a
//! bound is true because an exact inference says so, and an incumbent is
//! feasible because exact arithmetic over the original model says so.
//!
//! This module is the only place in the workspace permitted to contain a bare
//! `1e-*` literal. A threshold at a call site is folklore; a threshold with a
//! name and a documented meaning is policy.

/// Row-activity feasibility fence for the floating-point search.
pub const TOL_FEAS: f64 = 1e-6;

/// Integrality fence used to decide whether a variable looks integral enough
/// to stop branching on it.
///
/// A candidate that clears this fence is still screened exactly before it can
/// become an incumbent; passing here is permission to check, not to believe.
pub const TOL_INT: f64 = 1e-6;

/// Reduced-cost fence for declaring a floating-point basis optimal.
pub const TOL_OPT: f64 = 1e-9;

/// Smallest pivot magnitude the ratio test will admit.
pub const TOL_PIV: f64 = 1e-7;

/// Magnitude below which a computed entry is treated as structurally zero and
/// dropped from a sparse vector.
pub const TOL_ZERO: f64 = 1e-11;

// Fence ordering, checked at compile time rather than in a test: these
// relationships are load-bearing, and a release build should not be able to
// carry a tolerance table that contradicts itself.
const _: () = {
    assert!(TOL_ZERO < TOL_OPT);
    assert!(TOL_OPT < TOL_PIV);
    assert!(TOL_PIV < TOL_FEAS);
    assert!(TOL_FEAS == TOL_INT);
};

/// The tolerance set carried explicitly through solver calls.
///
/// Passed as a value rather than read from a global so that a call site's
/// policy is visible in its signature, and so a test can exercise a stricter
/// set without mutating shared state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tol {
    /// Row-activity feasibility fence.
    pub feas: f64,
    /// Integrality fence.
    pub int: f64,
    /// Reduced-cost optimality fence.
    pub opt: f64,
    /// Minimum admissible pivot magnitude.
    pub piv: f64,
    /// Structural-zero drop threshold.
    pub zero: f64,
}

impl Tol {
    /// The solver's declared tolerance set.
    #[must_use]
    pub const fn solver() -> Self {
        Self {
            feas: TOL_FEAS,
            int: TOL_INT,
            opt: TOL_OPT,
            piv: TOL_PIV,
            zero: TOL_ZERO,
        }
    }

    /// Whether a value is close enough to an integer for the search to stop
    /// branching on it.
    ///
    /// Deliberately named for what it is. The result is a search decision, and
    /// no proof may cite it.
    #[must_use]
    pub fn looks_integral(self, value: f64) -> bool {
        (value - value.round()).abs() <= self.int
    }
}

impl Default for Tol {
    fn default() -> Self {
        Self::solver()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrality_is_a_search_hint_at_the_declared_fence() {
        let tol = Tol::solver();
        assert!(tol.looks_integral(3.0));
        assert!(tol.looks_integral(3.0 + TOL_INT / 2.0));
        assert!(!tol.looks_integral(3.5));
        assert!(!tol.looks_integral(3.0 + TOL_INT * 10.0));
    }
}
