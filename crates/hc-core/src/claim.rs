//! The terminal vocabulary.
//!
//! Closed, and closed on purpose. There is no status meaning probably optimal,
//! none meaning optimal within tolerance, and none meaning the budget ran out
//! but the answer is almost certainly right. Every entry either names a proof
//! an independent checker accepted or admits that no claim is being made.
//!
//! The two entries that carry no bound are the honest ones and they are the
//! reason the vocabulary works. A solve that found a real point but could not
//! prove anything about optimality has a true thing to say, and a solve that
//! found nothing says so permanently rather than disappearing from a table.

use core::fmt;

/// A terminal claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Claim {
    /// The stated objective is attained and nothing feasible is smaller.
    Optimal,
    /// The model has no feasible point.
    Infeasible,
    /// The model has feasible points of arbitrarily small objective value.
    Unbounded,
    /// A feasible point, a certified lower bound, and the exact gap between.
    FeasibleWithGap,
    /// A feasible point, and nothing whatsoever about a lower bound.
    FeasibleNoBound,
    /// No mathematical claim at all.
    NoProgress,
}

impl Claim {
    /// The name this claim is published under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Optimal => "optimal",
            Self::Infeasible => "infeasible",
            Self::Unbounded => "unbounded",
            Self::FeasibleWithGap => "feasible(bound,gap)",
            Self::FeasibleNoBound => "feasible(no-certified-bound)",
            Self::NoProgress => "no-progress",
        }
    }

    /// Whether a certificate exists for this claim.
    ///
    /// The distinction is not cosmetic. A claim that carries a certificate can
    /// be rechecked by something that shares no code with the solver; one that
    /// does not is being taken on the solver's word, which is why the only
    /// such entry asserts nothing.
    #[must_use]
    pub const fn carries_certificate(self) -> bool {
        !matches!(self, Self::NoProgress)
    }

    /// Whether the claim asserts a lower bound on the objective.
    #[must_use]
    pub const fn asserts_bound(self) -> bool {
        matches!(
            self,
            Self::Optimal | Self::Infeasible | Self::FeasibleWithGap
        )
    }

    /// Whether the claim requires an exhibited feasible point.
    #[must_use]
    pub const fn requires_point(self) -> bool {
        matches!(
            self,
            Self::Optimal | Self::Unbounded | Self::FeasibleWithGap | Self::FeasibleNoBound
        )
    }

    /// Every claim, in the order the normative asset lists them.
    #[must_use]
    pub const fn all() -> [Self; 6] {
        [
            Self::Optimal,
            Self::Infeasible,
            Self::Unbounded,
            Self::FeasibleWithGap,
            Self::FeasibleNoBound,
            Self::NoProgress,
        ]
    }
}

impl fmt::Display for Claim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Why a solve made no claim.
///
/// A closed list, because the alternative is a free-text field that eventually
/// carries an excuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoProgressReason {
    /// The effort budget ran out with nothing certifiable in hand.
    EffortExhausted,
    /// The numerical escalation ladder ran to its end without progress.
    NumericsExhausted,
    /// The relaxation was unbounded and no integer-compatible ray was found.
    UncertifiedRelaxation,
    /// A frontier node could not be certified and no incumbent existed.
    UncertifiedFrontier,
    /// A cap was exceeded before assembly finished.
    CapExceeded,
}

impl NoProgressReason {
    /// The text this reason is published under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EffortExhausted => "effort budget exhausted with no certifiable point or bound",
            Self::NumericsExhausted => "numerical escalation exhausted",
            Self::UncertifiedRelaxation => "unbounded relaxation without an integer-compatible ray",
            Self::UncertifiedFrontier => {
                "a frontier node that could not be certified when no incumbent exists"
            }
            Self::CapExceeded => "a cap exceeded before assembly completed",
        }
    }

    /// Every reason, in the order the normative asset lists them.
    #[must_use]
    pub const fn all() -> [Self; 5] {
        [
            Self::EffortExhausted,
            Self::NumericsExhausted,
            Self::UncertifiedRelaxation,
            Self::UncertifiedFrontier,
            Self::CapExceeded,
        ]
    }
}

impl fmt::Display for NoProgressReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_are_the_published_ones() {
        assert_eq!(Claim::Optimal.to_string(), "optimal");
        assert_eq!(Claim::FeasibleWithGap.to_string(), "feasible(bound,gap)");
        assert_eq!(
            Claim::FeasibleNoBound.to_string(),
            "feasible(no-certified-bound)"
        );
        assert_eq!(Claim::NoProgress.to_string(), "no-progress");
    }

    #[test]
    fn only_the_empty_claim_carries_no_certificate() {
        for claim in Claim::all() {
            assert_eq!(
                claim.carries_certificate(),
                claim != Claim::NoProgress,
                "{claim}"
            );
        }
    }

    #[test]
    fn the_point_only_claim_asserts_no_bound() {
        assert!(Claim::FeasibleNoBound.requires_point());
        assert!(!Claim::FeasibleNoBound.asserts_bound());
        assert!(Claim::FeasibleWithGap.asserts_bound());
        assert!(Claim::Optimal.asserts_bound());
        assert!(!Claim::Unbounded.asserts_bound());
        assert!(!Claim::Infeasible.requires_point());
        assert!(!Claim::NoProgress.requires_point());
    }

    #[test]
    fn the_vocabulary_is_a_set_of_distinct_names() {
        let mut names: Vec<&str> = Claim::all().iter().map(|claim| claim.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Claim::all().len());
        let mut reasons: Vec<&str> = NoProgressReason::all()
            .iter()
            .map(|reason| reason.as_str())
            .collect();
        reasons.sort_unstable();
        reasons.dedup();
        assert_eq!(reasons.len(), NoProgressReason::all().len());
    }
}
