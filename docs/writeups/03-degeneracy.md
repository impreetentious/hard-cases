# Degeneracy, the simplex's haunted house

A simplex iteration picks a column that prices well, works out how far that
column can move before something blocks it, and moves. The textbook proof that
it terminates rests on the objective strictly improving at every step, so no
basis is ever revisited and there are finitely many bases.

A degenerate step moves zero distance. The basis changes; the point does not;
the objective does not. The termination proof is gone, and it is gone in a way
that does not look like anything: the solver is doing work, the iteration count
is climbing, the numbers are all finite, and nothing will ever happen again.

Degeneracy is not exotic. Beale's example is seven columns. Every assignment
problem is riddled with it. It is the normal condition of the models people
actually solve.

## The response is a ladder, not a fix

There is a clean answer — Bland's rule, lowest eligible index, provably cannot
cycle — and it is far too slow to use as the ordinary rule. So this solver
escalates, and the important part is that each rung is a *hypothesis about what
went wrong*:

```
normal  →  refactored  →  perturbed  →  bland
```

**Normal.** Devex pricing, Harris ratio test. What runs when nothing is wrong.

**Refactored.** After two hundred degenerate iterations in a row, the cheapest
explanation is that the numbers stopped being reliable — the basis
factorization has drifted and the reduced costs are describing a basis nobody
is in. So: factor again from the original columns, reset the pricing
framework, carry on. Often that is the whole story.

**Perturbed.** If it stalls again, the ties are real. Every finite bound is
widened outward by a distinct tiny amount drawn from a seeded stream. Widening
bounds outward cannot remove a feasible point — the perturbed region contains
the real one — so anything feasible stays feasible, and the ties that the
search was cycling on are gone.

**Bland.** Two escalations in, stop being clever. Lowest eligible index, which
terminates.

The seed lives in the options rather than in the machine, so a perturbation is
a deterministic function of a number the report already carries: a run that
escalated can be replayed exactly, on any machine.

## The rule the perturbation has to obey

A perturbed bound is a bound nobody wrote. If the search terminates while one
is in force, it has proved something about a model that does not exist.

So nothing terminates under perturbation. When pricing finds nothing better
and a perturbation is still in force, the perturbation is dropped, the ladder
resets to normal, and the search carries on against the real model. Only then
can it report optimal or infeasible. There is a trap in the suite named for
exactly this, and it is one of the ones that was broken deliberately to confirm
it fires.

That discipline is the local case of a rule that runs through the whole
project: a bound nobody wrote must never appear in an inference. It is the same
rule that makes an exact rebuild refuse to certify a float bound it cannot
reproduce, and the same rule that makes a presolve transform prove its
equivalence rather than assert it.

## Where the drift check actually looks

The other half of "the numbers stopped being reliable" is knowing when they
did. The obvious implementation asks the factorization how it is doing, which
is asking the suspect for a character reference.

So the residual is computed against the *original columns of the current
basis*, gathered from the problem, not through the factors that would be
answering. If the residual is too large the basis is factored again, and if the
reason was drift rather than an update failure the pricing weights are reset
too — because those weights were estimated from numbers that had stopped
meaning anything.

## The thing exact arithmetic takes away

Floating point gives one guarantee for free and it is easy to miss until it is
gone: **improvements below the last bit do not exist.** A loop that repeats
while something strictly improves will stop, because eventually the improvement
rounds to nothing.

The presolve reduction ran to a fixpoint on exactly that argument. Six of its
transforms remove a row or a column, so they can fire at most as often as the
model has rows and columns. The seventh tightens a bound and requires a strict
improvement.

In exact arithmetic a strict improvement can be made infinitely often. Two
rows over the same columns tighten each other by ever-smaller exact amounts,
each improvement genuinely strict, the sequence converging on a limit it never
reaches — and because the arithmetic is exact, the rationals grow at every
step. The loop does not merely fail to finish; it gets slower.

A generated four-column model did this on the solver fuzz target's first run.
Every committed instance and every campaign had been fine, because a campaign
that generates integer coefficients almost never produces the fractional cycle
it needs.

The fix is a budget: each side of each column may be tightened eight times in
one reduction, counted per column and per side.
[ADR-0018](../adr/0018-bound-tightening-gets-a-budget.md) records why a budget
and not a threshold — a smallest-improvement-worth-taking is a tolerance, and
there is no room for a tolerance inside an exact reduction. It is
[HC-CASE-0004](../case-files/HC-CASE-0004.md), and its lesson is the sharpest
one in the catalogue: *the comment was the bug.* The termination argument was
written down, confidently, in the module's own words, and because it was
written down with confidence nobody re-derived it.

## What the ladder is really for

Not speed. The escalation ladder exists so that a search which cannot make
progress says so, by name, at the end of a sequence of graded attempts — rather
than running forever, or reporting a status it has not earned.

The vocabulary this project uses for terminal states has a word for having
nothing to say. Most of the machinery in this file exists to make sure that
word is reachable.
