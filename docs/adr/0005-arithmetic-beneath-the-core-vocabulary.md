# ADR-0005: The exact arithmetic sits beneath the shared vocabulary

## Context

The workspace layout placed the shared vocabulary crate at the bottom of the
dependency graph, with the exact arithmetic crate depending on it. Building the
canonical model made that ordering impossible to keep.

The model is exact. Every bound, every coefficient, every objective entry and
the objective constant is a rational, because the whole project rests on a
certificate being about the numbers a file's characters name rather than about
the nearest binary64 to them. So the crate that defines the model must be able
to name a rational. Under the original ordering it could not: the arithmetic
was above it.

The arithmetic, meanwhile, needs nothing from the vocabulary. It has no opinion
about rows, columns, statuses, or identifiers. Nothing was flowing along the
declared edge in the declared direction.

## Decision

**The edge is reversed.** The exact arithmetic crate is the bottom of the
graph and depends on nothing; the shared vocabulary crate depends on it. Every
other edge is unchanged, and the vocabulary crate remains what every solver-side
crate above it shares.

This crosses a crate boundary, so it is recorded here rather than settled by a
comment at an import.

**The vector harness keeps its edge in the other direction, as a development
dependency.** The arithmetic's tests read the frozen vectors through the
vocabulary crate's reader for the asset format, which is a cycle only among
test targets. The package manager permits that, no shipped artifact contains
it, and the alternative — a third reader for the same format, written solely so
that the arithmetic could read its own vectors — would be worse in every way
that matters.

**The verifier is untouched.** It has no dependencies in either direction and
acquires none here.

## Consequences

The graph now matches what the code actually needs, which is the only ordering
that stays stable. A crate below the vocabulary can be reasoned about without
knowing what an instance is, and the model can be exact without a detour.

The cost is the development cycle, which is a real if minor cost: a change to
the vocabulary crate's reader now forces a rebuild of the arithmetic's tests.
That is visible and bounded, and it is the price of having one reader on this
side of the boundary instead of two.

The broader point is that a declared architecture written before any code is a
prediction. Most of this one has held. This edge did not, and the useful
response is to move the edge and say why, rather than to bend the model around
a line in a diagram.

## Alternatives considered

**Put the canonical model in the parser crate**, which depends on both. Rejected:
the linear-algebra and search crates need the model and do not need the parser,
so the model would have dragged file-format code into the middle of the
dependency graph.

**Make the model generic over its coefficient type**, instantiated exactly in
one place and in floating point in another. Rejected as ceremony. There is one
canonical model and it is exact; the floating-point copy the search works on is
derived from it and has a different shape anyway, since it carries scaling and
a basis and the exact model carries neither.

**Move the arithmetic into the vocabulary crate.** Rejected: the arithmetic's
independence from everything else is what lets it be compared against the
verifier's arithmetic as a unit, and burying it inside a crate full of instance
vocabulary would blur that.

## Supersedes / superseded by

Amends the crate graph decided in [ADR-0001](0001-foundation-decisions.md). No
decision is superseded.
