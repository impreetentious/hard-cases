# ADR-0007: Basis updates are kept in product form for now

## Context

A revised simplex changes one basis column per iteration. Refactoring the
basis every time would dominate the run, so the factorization is kept and each
change is recorded on top of it. Two representations are standard.

The **product form** stores, for each change, the entering column expressed in
the basis of the moment. That vector is already computed — an iteration needs
it to choose the pivot at all — so an update costs nothing beyond storing it,
and applying the file is one pass of scale-and-subtract per update.

The **Forrest–Tomlin** form instead rewrites the upper factor: the replaced
column leaves a spike, the spike is eliminated by a cyclic permutation and one
row transformation, and what is stored is that row rather than a column. It
keeps the update file substantially sparser over long sequences, and it is
better behaved numerically, because the elimination is performed against the
existing triangular structure rather than accumulated alongside it.

The architecture named Forrest–Tomlin. Implementing it well requires column
access to the upper factor, which the factorization here does not maintain —
it stores the upper factor by row, because that is the order the solves want.
Adding the second index is not difficult; getting the spike elimination and the
permutation right, on a path whose failures are silent and show up much later
as a basis whose solutions satisfy nothing, is where the risk is.

## Decision

**Updates are kept in product form.** The representation, the two solves, and
the refactorization policy are implemented and tested against refactorization
from scratch, on single updates and on chains of them.

**This is a deferral with a stated condition for revisiting, not a
substitution.** The condition is a measurement: when a campaign on real
instances shows the update file's cost — stored coefficients relative to the
factorization underneath it — is what limits the search, the transposed form
is implemented and this record is superseded. Until such a measurement exists,
changing it would be work justified by folklore.

Three things make the deferral affordable, and they are the reason it is
acceptable rather than merely convenient:

1. **The update file is bounded by the refactorization interval.** At a hundred
   updates the file is discarded and the basis is factored again, so the
   sparsity argument that favours the transposed form — which is about long
   sequences — applies weakly at this length.
2. **A third refactorization trigger covers the case the bound does not.** When
   the update file outgrows the factorization beneath it, the basis is factored
   again regardless of how few updates have accumulated. A dense entering column
   against a sparse basis is exactly where the product form is worst, and it is
   caught by size rather than by counting.
3. **Nothing here can make a claim wrong.** A basis is a place to look. Every
   bound this machinery leads to is recomputed exactly, from the original file,
   by code that shares nothing with it. The cost of the product form drifting
   further than the transposed form would is iterations, not truth.

## Consequences

The update path is small enough to read in one sitting, which is worth
something on code whose failure mode is a plausible wrong number. The
refactorization policy gained a reason it would not otherwise have needed, and
that reason is named in the trace rather than folded into a boolean, so a run
that keeps refactoring on growth is visibly telling a different story from one
that refactors on the interval.

The cost is that long runs between refactorizations will be slower than they
need to be, and that instances with dense columns will refactor more often than
the transposed form would require. Both are measurable, and measuring them is
the condition above.

## Alternatives considered

**Implement Forrest–Tomlin now.** Rejected on sequencing rather than on merit.
There is no simplex yet to measure, so the implementation would be tuned
against an imagined workload, and the risk sits in exactly the kind of code
that is hardest to test without one.

**Refactor every iteration and keep no updates at all.** Rejected: it is
obviously correct and it makes the factorization the dominant cost of every
iteration, which would turn every later performance question into a question
about this decision.

**Keep both and choose at run time.** Rejected without hesitation. Two update
schemes means two sets of numerical behaviour reachable from one input, and a
deterministic solver cannot have a path whose arithmetic depends on which one
was picked.

## Supersedes / superseded by

Defers part of the linear-algebra plan recorded in
[ADR-0001](0001-foundation-decisions.md). No decision is superseded.
