# ADR-0015: A cut's displacements are measured from the model, not from a basis

## Context

The Gomory derivation is written over *displacements*. A nonbasic variable
enters the tableau equation as a nonnegative quantity — `w = x - l` measured up
from a lower bound, or `w = u - x` measured down from an upper one — and every
term of the formula assumes the displacement is nonnegative.

Which of the two a variable uses is, in a solver, a fact about the basis: a
nonbasic column rests at one bound or the other, and the tableau is written in
those terms. The frozen specification says the record carries that bound state
and the verifier checks it. The frozen *encoding* carries no such field, and it
cannot grow one: adding a field to a record type is a format version bump, and
the format was frozen before any of this was written precisely so that it would
not be renegotiated feature by feature.

So the record has to be checkable without it.

## Decision

**The displacement is a property of the model, not of the basis. The verifier
chooses it, by a fixed rule, and the record says nothing about it.**

For each nonbasic variable in the reconstructed equation:

- if the variable has a finite lower bound, the displacement is `w = x - l`;
- otherwise, if it has a finite upper bound, the displacement is `w = u - x`;
- otherwise the derivation is refused, because there is no displacement to
  write and the formula has no meaning without one.

An integer variable's endpoint must be an integer, so that a unit step in the
displacement is a unit step in the variable. That check is unchanged.

## Consequences

**The cut is valid whatever the solver's basis said**, which is the substance of
this record. `w = x - l` is nonnegative at every point of the model, not merely
at the vertex the search happened to be sitting on. The Gomory inequality
derived from an equation in nonnegative displacements is valid for every
integer-feasible point of that model, so the verifier's acceptance does not
depend on believing anything about a basis — and a basis is exactly the kind of
thing this project refuses to believe.

**It is a restriction on which cuts can be certified, and a small one.** A cut
the solver derived by measuring a two-bounded variable down from its upper bound
cannot be written in this format; the solver must derive it from the lower bound
instead. That costs some cuts and no correctness, and it removes a class of
disagreement — two implementations quietly picking different displacements for
the same variable and both being locally reasonable — that would have been very
hard to find.

**Nothing about the encoding changes**, so no version moves. The specification's
sentence about a recorded bound state is corrected to describe the rule above,
which is a correction of a description rather than a change of a contract: the
records a conforming certificate contains are exactly the records it contained
before.

**A cut derived under one displacement and applied under another is still a
different inequality**, and that hazard is now impossible rather than checked:
there is one displacement per variable per model, and both implementations
compute it from the model.

## Alternatives considered

**Add the bound state to the record.** Rejected: a format version bump, migration
fixtures, and a new field whose only purpose is to let the certificate assert
something the verifier can derive. A proof format should carry what cannot be
recomputed and nothing else.

**Have the verifier try both displacements and accept if either works.**
Rejected. It doubles the search in the checker to accommodate a choice the
solver made, and it makes the accepted set depend on an exhaustive search rather
than on a rule — which is the kind of thing that is fine until the day two
displacements both work and produce different cuts.

**Infer the bound state from the recorded cut coefficients.** Rejected outright:
that is reading the answer to decide how to check the answer.

## Supersedes / superseded by

Corrects the displacement rule in the cut specification, which described a field
the encoding does not have. No decision is superseded and no format version
moves.
