# ADR-0018: Bound tightening gets a budget

## Context

The reduction chain runs eight transforms to a fixpoint. Its termination
argument was written down when it was built, in the module's own words: every
transform removes a row or a column, or strictly tightens a bound, and the
tightening rule requires a strict improvement, so no rule can fire twice on the
same situation and the fixpoint stops.

Seven eighths of that argument is sound. A rule that removes a row or a column
can fire at most as often as the model has rows and columns, and there are six
such rules plus one that fixes columns.

The eighth is not an argument at all. A strict improvement can be made
infinitely often. Two rows over the same columns can tighten each other's
bounds by ever-smaller exact amounts, each improvement strict, the sequence
converging on a limit it never reaches. Nothing stops it, and because the
arithmetic is exact rather than floating point, the rationals grow at every
step: the loop does not merely fail to finish, it gets slower.

A generated four-column model did exactly this. It was found by the solver fuzz
target on its first run, in a case that had already been solved fine by every
committed instance and every campaign, because a campaign that generates
integer coefficients almost never produces the fractional cycle that is needed.

## Decision

**Each side of each column's bound may be tightened at most eight times in one
reduction.** The budget is counted per column and per side, in the fixpoint
loop, and a tightening candidate whose budget is spent is skipped rather than
applied.

A budget rather than a threshold. The alternative — accept a tightening only
when it improves the bound by at least some amount — is a tolerance, and a
tolerance inside an exact reduction is precisely the thing this project has no
room for: it would make the reduced model depend on a number nobody derived,
and two implementations with different numbers would produce different models
from one file.

Counting is honest in a way a threshold is not. Presolve is an optimization. A
reduction it declines to make costs effort and nothing else — the reduced model
is weaker and remains exactly equivalent to the original, which is the property
the transform invariants and the verifier's replay both check. There is no
correctness claim anywhere that depends on the chain being maximal.

Eight is generous for a model that converges and immediately fatal to one that
does not. Every genuine tightening of a column comes from a different row or
from a neighbour's bound having moved; a column whose bound has moved eight
times on one side is being chased around a cycle rather than reduced.

## Consequences

**`reduce` is total.** Six rules are bounded by the model's size, one is
bounded by its columns, and the eighth is now bounded by its columns times
sixteen. The chain has a length bound that can be written down, which it did
not have before.

**The verifier is unaffected.** It replays the chain it is handed and checks
each step; a shorter chain is a shorter replay. No record changed, no format
version moved, and no frozen vector was touched.

**Some models are reduced slightly less far.** No committed instance, campaign,
or fixture reduces differently: the budget is reached only by the cycling case
this was written for. If a real instance ever hits it, the reduced model is
still exactly equivalent and the search simply starts from a slightly weaker
one.

**The module's termination argument is now written correctly**, beside the code
that makes it true. The old one was wrong in a way that reads as obviously
right, which is worth leaving a note about.

## Alternatives considered

**A total step cap on the chain.** Simpler, and wrong in the shape it fails: a
cap large enough for a ten-thousand-column model does not stop a cycle in a
four-column one, and a cap small enough to stop the cycle stops a large model
from being reduced at all. The cycle is a property of one column, so the budget
belongs to one column.

**Requiring a minimum improvement.** A tolerance, addressed above.

**Refusing to tighten a continuous column at all.** This would remove the cycle
entirely, since integer columns round their derived bounds and therefore
converge in finitely many steps. It also removes most of what the rule is for:
tightening continuous bounds is what makes a relaxation smaller, which is the
whole point of doing this before a search.

## Supersedes / superseded by

Nothing. It corrects a termination argument stated informally in the chain
module and repairs the code that argument was about.
