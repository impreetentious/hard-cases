---
id: HC-CASE-0004
crate: hc-presolve
found-by: the fuzz corpus, solver target
reproduce: cargo run -p hc-fuzz --bin hc-fuzz -- --from 45 1
fixed-in: v0.11.11
rejection: none
---

# The reduction that never finished

## Symptom

The solver target hung. Not slowly — at a hundred percent of one core, with no
output, for as long as it was left. A sample of the stopped process put every
frame inside `hc_presolve::chain::reduce`, and beneath that in
`Rational::add`, and beneath that in the greatest common divisor of two
integers that were getting longer.

The instance was four columns and four rows, generated from seed 45. Nothing
about it looks unusual.

## Hunt

The termination argument was written down in the module, which made this quick:

> Every transform removes a row or a column, or strictly tightens a bound, and
> the tightening rule requires a strict improvement — so no rule can fire twice
> on the same situation and the fixpoint stops.

Seven eighths of that is sound. Six rules remove a row or a column and one
fixes a column, so between them they can fire at most as often as the model has
rows and columns.

The eighth is not an argument at all, and reads exactly like one. **A strict
improvement can be made infinitely often.** Two rows over the same columns
tighten each other's bounds by ever-smaller exact amounts, each improvement
strict, the sequence converging on a limit it never reaches. In floating point
this would stop when the improvements fell below the last bit. In exact
rational arithmetic it does not stop, and the numbers get longer at every step,
so the loop does not merely fail to finish — it decelerates.

## Root cause

A termination proof that held for seven rules and was assumed for the eighth.

## Fix

Each side of each column's bound may now be tightened at most eight times in
one reduction. ADR-0018 records why a budget rather than a threshold: a
smallest-improvement-worth-taking is a tolerance, and a tolerance inside an
exact reduction would make the reduced model depend on a number nobody derived.
Counting is honest instead — presolve is an optimization, and a reduction it
declines to make leaves a weaker reduced model that is still exactly equivalent.

The module's termination argument was rewritten to say what is actually true,
beside the code that now makes it true.

## Lesson

The comment was the bug. It stated a proof, the proof was wrong in one of its
cases, and because it was written down with confidence nobody re-derived it —
including, for a while, the person reading it during this hunt.

Exact arithmetic removes a termination argument that floating point gives away
for free. Every loop that stops "when the improvement gets small" is really
stopping because the improvement eventually rounds to zero. Take the rounding
away and the loop needs a reason of its own, and every such loop in a project
like this one has to be looked at again with that in mind.
