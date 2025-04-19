# The cut that lies

Every defect in this kind of solver announces itself except one.

A wrong bound gets caught: the exact rebuild fails to reproduce it and the
claim is downgraded. A wrong incumbent gets caught: exact screening against the
original rows refuses the point. A broken tree gets caught: the recursive cover
does not close. A crash gets caught by being a crash.

An invalid cut does none of that. A cutting plane is an inequality added to a
node's relaxation, and it is *supposed* to remove points — that is what it is
for. If it also removes a feasible integer point, the search never notices. The
relaxation gets tighter, the node closes sooner, the tree gets smaller, the run
gets faster, and the answer is wrong. It looks like exactly what a good cut
looks like.

## Two ways to be wrong

**The derivation is wrong.** A Gomory mixed-integer cut is a formula over a
tableau row: split the columns by whether their fractional part exceeds the
row's, apply one expression to each side. Swap the two branches and you get an
inequality that is stronger than the valid cut — and stronger than valid means
it removes integer points. There is a frozen vector in `spec/v1` for exactly
that mutation, and the note beside it says what it is: *the mutation the
quadrant trap exists for.*

**The scope is wrong.** A cut derived at a node is sound over that node's
region and nowhere else, because it was derived from that node's bounds, which
include the branching restrictions above it. Let it escape into a sibling
subtree and it is an arbitrary inequality applied to a region it says nothing
about — with the same silent consequence.

## Both are checked twice

The solver enforces the scope. Every pool entry carries the node it was derived
at, ancestry decides visibility, and a node's relaxation sees only cuts whose
scope is on its path to the root.

The verifier enforces it again, independently. A leaf citing an inference that
uses a cut invisible at that node is refused — `HCC-SCOPE-01: inequality 17 is
not visible here`. Enforcing it once in the solver would mean the guarantee
disappears the moment the solver is wrong, which is the case the guarantee is
for.

And the derivation is rederived, not read. A Gomory record states a combination
over visible inequalities and the coefficients it claims follow; the verifier
computes the coefficients from the combination and compares. A cover record
states a cover and a lifting order; the verifier runs the lifting dynamic
program itself. A record whose stated coefficients do not reproduce is refused.

## The campaign that was a shape check

This is the part worth admitting.

For a while there was a "cut validity campaign", and it was green, and it
checked that every cut in a certificate named a real node and had coefficients.

That is a shape check. It would pass on a cut that removed half the feasible
region.

What replaced it enumerates each generated model's own feasible integer points
— from the model, by looking — and holds every cut against the ones inside its
scope. Not the cut the solver said was valid: every integer point in the
region, against every cut visible there. It consults nothing the solver
derived.

Across a hundred thousand tiny instances, twenty-two thousand of which cut,
that campaign has removed zero feasible integer points, across nearly two
hundred thousand individual cut-and-point checks. The number that matters is
the zero, and the reason the number means anything is that the check does not
share an idea with the thing it is checking.

That is the same pattern as the enumeration oracle in the committed
mixed-integer corpus: three of those four instances have every assignment in
their integer box walked, and the best feasible one found by looking, with the
result compared against the search by exact value. A program with no simplex in
it agreeing with one that has a simplex in it is worth more than two programs
with simplexes agreeing.

## Cuts are the reason to have a node budget

There is a second-order effect worth naming. Separation is on by default —
four rounds at a node, one constant that the command line, the browser page and
the scoreboard all ask for. Cuts make trees smaller, sometimes dramatically:
one of the committed instances closes at the root with two cover cuts and takes
nine nodes without them.

That is good for the solver and awkward for a demonstration, and the temptation
is obvious: turn cuts off somewhere so the tree looks impressive. This project
does the opposite. Every surface a person uses runs the same configuration, and
the situation room ships an instance that still branches with separation on.

That temptation had already been acted on by accident.
[HC-CASE-0006](../case-files/HC-CASE-0006.md) is a defect where two of this
program's own subcommands ran different solvers, because one took a derived
default and the derived default had separation off — and the browser surface
took the same default, so the page had been demonstrating this solver with its
cuts switched off. Nobody noticed, because a nine-node tree looks better than a
one-node tree.

## The playbook

The rules for an invalid cut are written down in advance, because it is not a
defect anybody wants to be reasoning about for the first time when it happens:

> Invalidate every descendant certificate that used the pool entry. Disable
> family promotion. Add a sibling tamper case. Then find the bug.

Invalidate first. The certificates are the artifact; a proof built on a bad
inequality is not a proof, whatever it verified as at the time.

## What separates this from a slower solver

None of the above makes the cuts weaker. Gomory mixed-integer cuts and
exact-lifted cover cuts are the ordinary families, derived the ordinary way.

What is different is that every one of them is a *claim* rather than a
convenience: derived in exact arithmetic, scoped in the solver, scoped again in
the verifier, rederived from the combination it names, and held against the
feasible points of a hundred thousand models by a program with no simplex in it.

An invalid cut is the one defect here that a correct-looking answer can hide.
Everything in this file exists because of that sentence.
