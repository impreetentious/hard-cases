# Big rationals from nothing

The verifier has no dependencies. Not "no heavy dependencies" — none. That
constraint is the whole point of the crate, and the first thing it costs you is
arithmetic: a proof about a linear program is a proof about rational numbers,
those rationals get wide, and there is no `num-bigint` to reach for.

So there are two arbitrary-precision arithmetics in this repository. The solver
has one. The verifier has another. They share no code, and they are
deliberately built out of different algorithms.

## Deliberately unlike

Independence that comes from copying a file into a second directory is
independence in the file-system sense only. If both sides carry the same
division routine, both sides get the same carry wrong on the same input, and
the second implementation adds nothing except the confidence of having one.

So the two stacks disagree about how to do everything:

| | solver side | verifier side |
|---|---|---|
| limb width | sixty-four bits | thirty-two |
| multiplication | Karatsuba above a threshold | schoolbook only |
| division | Knuth's algorithm D with the add-back correction | a trial digit walked down until its product fits |
| greatest common divisor | binary | Euclid |
| the acceptance fence | decided by rounding | decided by exact comparison against both rounding boundaries |

That last row is the one worth dwelling on. Both sides have to answer the same
question — does this exact rational survive a round trip into binary64 — and
they answer it by different routes: one rounds and compares, the other
compares the exact value against the two boundary rationals that bracket the
nearest double. Same answer, no shared reasoning.

## The vectors are inert

Both stacks read the same frozen test vectors out of `spec/v1/vectors/arith/`,
and neither of them wrote those vectors. They are text assets in a restricted
TOML subset — restricted because the verifier has to read them with a parser of
about the size a dependency-free crate can carry, so an asset this project's
generator accepts but the verifier's reader would not is an asset the verifier
cannot use. The assets are the specification; the code is not.

Beside the vectors runs a differential campaign of ten million small cases
against machine arithmetic, comparing both stacks against `u128` on inputs
where `u128` is exact.

## The vector that was wrong

`[rational.compare_close]` asserted that 355/113 exceeds 22/7.

Cross-multiplying: 355 × 7 = 2485, 22 × 113 = 2486. It does not.

Worse than the arithmetic error was the stated rationale. The vector claimed to
separate exact comparison from floating-point comparison, and the two values
are far enough apart that binary64 separates them without any trouble at all —
so even had the direction been right, the vector was testing nothing it said it
was testing. It is now `less`, with a second vector beside it that does what
the first claimed: one plus two to the minus fifty-third against one, which a
float comparison calls equal and an exact one does not.

[ADR-0003](../adr/0003-arithmetic-vector-correction.md) records it, and the
fix that mattered was not the number. The vector harness now recomputes every
vector it can by a second route, which is what would have caught this without
a human noticing.

## Rationals from text, not from doubles

A model's coefficients are read from the file's decimal text. `0.1` becomes one
tenth. It is worth saying why, because the alternative is so much easier:
parsing to `f64` and converting gives you the binary64 nearest to one tenth,
which is a different number, and a proof about that number is a proof about a
model nobody wrote.

The consequence runs the other way too. A model whose exact values do not
survive a round trip into binary64 is *rejected* rather than solved, because
the search would otherwise be working on a different model from the one the
proof is about. That costs instance coverage, deliberately.

The writers inherit the same discipline. A coefficient whose denominator
carries a prime other than two or five has no finite decimal expansion, and
there is no rounding available on a path a proof is about — so such a model is
refused rather than approximated, and the JSON schema, which carries every
value as text, is the format that can usually still take it.

## What the second implementation actually bought

Two things, and the first is the one people expect.

It caught disagreements. Every vector, every campaign case, both stacks, same
answer required — and a stack that got a carry wrong would have to get it wrong
in a way the other one, built from different algorithms, also got wrong.

The second is less obvious and turned out to matter more. Writing the same
capability twice forces the *specification* to be written down, because the
second implementation cannot be written by reading the first. Every question
the first implementation answered by accident had to be answered on purpose the
second time — and the questions the specification never asked out loud are
exactly where two honest implementations drift apart. That is the lesson filed
in [HC-CASE-0002](../case-files/HC-CASE-0002.md), where two readers disagreed
about a malformed number inside a malformed statement, and the fix was a rule —
a line is lexed, and then it is interpreted — rather than a patch to either
side.

Independence is not freedom. Both stacks are answerable to one specification.
What two implementations buy you is a machine for finding the places that
specification is silent.
