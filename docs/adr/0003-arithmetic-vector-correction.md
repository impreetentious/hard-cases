# ADR-0003: Correcting a miscomputed rational comparison vector

## Context

The vector corpus under `spec/v1/vectors/` is frozen: it is the only thing the
two independent arithmetic implementations are held against, and neither of
them generated it. That is exactly why an error in it is serious. A vector
stating a false expected value does not merely fail to catch a bug; it demands
one, and the first implementation to satisfy it is wrong in a way the corpus
now certifies as correct.

Building the solver-side rational comparison surfaced such an error.
`[rational.compare_close]` compared `355/113` against `22/7` and stated the
first was the greater. Cross-multiplying gives 2485 against 2486, so the first
is the smaller. The two values differ in the third decimal place, and the
vector's stated result was simply miscomputed by hand.

The accompanying rationale was wrong for a second reason. It claimed the pair
demonstrates that an exact comparison and a floating-point comparison disagree.
They do not: the two values are far enough apart that binary64 separates them
without difficulty, and the vector proved nothing about exactness at all.

## Decision

**The stated result is corrected to `less`,** and the rationale is rewritten to
say what the pair actually pins: an ordering that mental arithmetic reliably
gets backwards.

**A second vector, `[rational.compare_beyond_binary64]`, is added** to cover the
property the first one claimed. It compares one plus two to the minus
fifty-third against one. Both operands round to the same binary64, so a
comparison performed through a float returns equal, and cross-multiplication
returns greater. That is the pair that separates an exact comparison from a
rounded one, and the corpus now contains it.

This is a correction to a vector, not a change to a rule. The comparison rule
in `canonical.toml` — cross-multiplication with strictly positive denominators —
is untouched, so no contract version moves and no fixture regenerates. The
freeze procedure still applies, which is why this record exists: an asset under
`spec/v1` changed, and a change there is never silent.

## Consequences

The corpus is now internally consistent with the arithmetic it describes, and
the exactness property has a vector that genuinely tests it. Any implementation
already written against the old expected value would have failed against real
arithmetic at the first comparison of two close rationals; none existed yet, so
nothing downstream had to move.

The wider consequence is a reminder about hand-worked vectors. Their value is
that no implementation produced them; their risk is that no implementation
checked them either. Every vector whose expected value can be recomputed by an
independent route should be, and the vector harness now does exactly that where
the operation admits it: a comparison is checked against the reconstruction
identity, a quotient against its remainder, and a reduction against its own
divisor.

## Alternatives considered

**Leave the vector and make the implementations match it.** Rejected without
hesitation. A false claim in the corpus that both sides satisfy is the exact
failure the two-implementation design exists to prevent, and satisfying it
deliberately would be worse than the bug it imitates.

**Delete the vector.** Rejected. The operands are good ones and the case is
worth pinning; only the expected value was wrong.

**Correct it silently as a typo.** Rejected. The assets are normative, and a
normative asset that can be edited without a record is not frozen.

## Supersedes / superseded by

Refines the vector corpus frozen by [ADR-0002](0002-format-freeze-decisions.md).
No decision is superseded.
