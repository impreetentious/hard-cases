# ADR-0013: The exact rebuild belongs to the linear core

## Context

Turning a basis back into exact quantities — the vertex it names, the row
multipliers it implies — was written where its first caller lived, in the
certificate assembler. That was the right place when the assembler was the only
thing that needed it.

Branch and bound needs it too, and needs it for a different reason. Before a
candidate solution may become the incumbent, it has to be established exactly
feasible and exactly integral against the original model, because a
float-feasible point that is not exactly feasible would prune parts of the tree
that contain the answer. Establishing that means rebuilding the node's vertex
in rationals, which is the same rational Gaussian elimination the assembler
performs.

The two callers sit on opposite sides of the crate graph. The search is below
the certificate assembler — the assembler serializes what the search found —
so the search cannot reach up for it. The choices were to duplicate the
elimination, to invert the dependency, or to move the code down.

## Decision

**The exact canonical view of a model, the rational linear system, and the
rebuild of a basis into a vertex and multipliers move into the linear-programming
core, as `hc_lp::exact`.**

The certificate assembler keeps what is genuinely about certificates: the
canonical inequality index, the construction of nonnegative multipliers over
those inequalities, and the recomputation an assembled inference is checked
against. Those have no meaning outside a proof and do not belong in a solver.

The split line is a question, not a convenience: *does this mean anything to a
solver that emits no certificate?* An exact vertex does — it is what an honest
incumbent test needs. A nonnegative multiplier over a numbered axiom does not.

## Consequences

The rational elimination now exists once. It is the piece of this project whose
being right matters most and whose being fast matters least, and two copies of
it would be two chances to get it wrong differently.

The linear core grows an exact half. That is not a contradiction of the numerics
constitution, which says the *search* computes in binary64 and that nothing it
computes there is a conclusion; the exact half is where conclusions are drawn
from what the search found, and it was always going to live somewhere.

Alongside the move, the working copy gains the ability to produce a copy with
some columns' bounds narrowed, which is how a search node differs from its
parent. Narrowing only: a request that would widen a bound is ignored rather
than applied, because a node able to relax its parent's assumptions would break
the one property the tree's correctness rests on, which is that the children
partition the parent.

The certificate assembler's public surface is unchanged. `Gap` and `Wanted` are
re-exported from where they used to live, because the reasons a rebuild declines
are part of the assembler's error vocabulary even though the rebuild is not.

## Alternatives considered

**Duplicate the elimination in the search.** Rejected. Two implementations that
must agree, in the one place where disagreement is worst, with no vectors
holding them together — this project has a name for that arrangement and uses it
deliberately for the solver and the verifier, where the two are written to fail
differently on purpose. Doing it accidentally inside one side is the opposite.

**Let the search depend on the certificate assembler.** Rejected: it inverts the
declared crate graph and makes the thing that produces proofs a prerequisite for
searching, which would mean a search cannot run without the machinery for
writing down what it found.

**Screen incumbents in floating point with a tolerance.** Rejected outright. An
incumbent is the value every prune in the tree is measured against; accepting a
point that is feasible only to within a fence would let the search discard
regions containing the answer, and the certificate would then be a correct proof
of a false statement.

## Supersedes / superseded by

No decision is superseded. The crate graph in the architecture is refined rather
than changed: the edge from the search to the exact arithmetic was always
declared, and this makes it carry something.
