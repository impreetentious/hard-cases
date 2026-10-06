# Hard Cases — Writeups

Six essays. Each names a failure mode this project is built against and shows
the machinery that catches it, using the code and the incidents in this
repository rather than a general argument.

1. [The solver that shows its work](#1-the-solver-that-shows-its-work) — why a certificate,
   and what a certificate has to be before it is worth anything.
2. [Big rationals from nothing](#2-big-rationals-from-nothing) — two arbitrary-precision
   arithmetics by one author, and why that is the cheap half of independence.
3. [Degeneracy, the simplex's haunted house](#3-degeneracy-the-simplexs-haunted-house) — the one way a
   simplex fails to terminate, and a ladder rather than a hack.
4. [A proof you can tamper with](#4-a-proof-you-can-tamper-with) — the certificate format, the
   verifier's position, and thirty-one ways to lie to it.
5. [The cut that lies](#5-the-cut-that-lies) — the one defect in this kind of
   solver that a correct-looking answer can hide.
6. [Honest scoreboards](#6-honest-scoreboards) — what a board may report,
   what it may not, and what this one leaves blank.


## 1. The solver that shows its work

An industrial mixed-integer solver prints `optimal` and there is nothing you
can do about it. The engine is closed, the arithmetic is floating point, the
search tree is gone the moment the process exits, and the number on the last
line is an assertion by a program you did not write and cannot inspect. In
practice everybody accepts this, because the alternative — rebuilding the model
in a second solver and hoping the two agree — costs more than the answer is
worth and establishes less than it looks like it does.

This project is one refusal: **it never claims optimality it cannot prove.**

Not "tries hard to be right". Not "extensively tested". Every terminal claim
ships with a certificate, and a separate program reads the *original problem
file* — not the solver's internal model, not a dump, the bytes on disk — and
either accepts the proof or names the rule that stopped it.

### What a certificate has to be before it is worth anything

Three properties, and dropping any one of them turns the exercise into
theatre.

**It has to be checkable by something that did not produce it.** `hc-verify`
is 8,423 lines across 23 files with **zero dependencies** — not zero external
crates, zero: its own arbitrary-precision integers, its own rationals, its own
SHA-256, its own parsers for all three input formats, its own model, its own
rule engine. `cargo tree -p hc-verify` is one line. A gate fails the build if
the resolved dependency graph, development and build edges included, ever
contains anything but the crate itself, and a second gate fails it if any file
under the crate imports another crate in this workspace.

**It has to be about the file, not about the solve.** A certificate that
proved something about the solver's internal representation would be proving
something about a model nobody wrote. So the verifier reparses the original
bytes with its own reader, recomputes the canonical form, hashes it, and checks
that hash against the one the certificate states. Everything downstream is
about the model that hash names.

**It has to be refusable.** A checker that skips what it does not understand
is a checker that accepts anything you can phrase unusually. Every record kind
the format defines has a rule that closes it, and a record kind this
implementation cannot close is *refused* — with a message saying so — rather
than passed over. That is [decision 0010](SOLVER-SPEC.md#20-decision-log),
and it is the difference between a verifier and a linter.

### Where floating point stops

The search is floating point, and it should be: a simplex over rationals is a
simplex that never finishes. The rule is not "avoid floats", it is **nothing
computed in floating point is ever a conclusion.**

A node bound exists as a claim only once it has been rebuilt as an exact
nonnegative combination of inequalities the model actually contains, in exact
rational arithmetic, over values read from the file's decimal text rather than
round-tripped through binary64. A coefficient written `0.1` is one tenth here,
not the double nearest to it. An incumbent exists only after the point has been
checked exactly against every row and every bound.

When the exact rebuild does not reproduce what the search believed, the answer
is a *weaker true statement* — a certified gap instead of an optimum, or a
feasible point with no bound at all — never a stronger unproved one. The status
vocabulary has a word for having nothing to say, and the solver uses it.

### The part that keeps it honest

An arrangement like this fails in a specific way: the checker quietly stops
checking. A rule gets softened to make a stubborn instance pass; a status gets
reported when assembly declined; a campaign passes because its corpus is
absent. Every one of those leaves the printed output looking exactly the same.

So the defences are all about *visibility rather than intent*:

- **A tamper corpus, not a test fixture.** Thirty-one named mutations of
  genuine certificates, each paired with the rejection identifier that must
  answer it — not merely "must be rejected", which would pass while every one
  of them was caught by the checksum and would go on passing after a proof rule
  was deleted. Most are re-sealed with a correct checksum, so the mathematics
  has to be what refuses them.
- **Absent corpora are named, not skipped.** A harness whose instances have
  not been fetched reports each one by name. A gate that quietly passes on an
  empty directory has proved that somebody once intended to test something.
- **Every gate has been broken on purpose.** A check that has never failed has
  not been shown to work.
- **Case files.** Six real defects in this repository, minimized, each with the
  seed or the command that reproduces it and a test that replays it forever.
  Two of them were the verifier catching the solver. One was the verifier
  catching a defect three stages downstream of where it happened, on a model
  that had been reduced, solved and lifted since — and the lesson recorded is
  not that the net works, but that had the wrong value landed inside its bounds
  by coincidence, the same reduction would have passed.

### What this is not

It is not formal verification. Nothing here is machine-checked against a
specification, and the calculus itself is a design that could be wrong.

It is not two authors. The solver and the verifier share no executable code and
they are structurally unlike each other on purpose, but the same person wrote
both. Two implementations by one author fail differently far more often than
one implementation fails twice — that is the entire benefit being claimed, and
it is less than the benefit of two authors. That sentence is in
[LIMITATIONS](SOLVER-SPEC.md) rather than in a footnote, because a trust
boundary you have to infer is not a trust boundary.

It is not fast. There are no wall-clock comparisons against other solvers
anywhere in this project and there will not be any. Work is budgeted in
deterministic counts — simplex iterations, nodes, cut rounds; bytes and limb
operations on the verifier's side — because a budget in seconds makes the
program's own behaviour depend on the machine it ran on.

What it is: a solver where "optimal" is a sentence somebody else can check, and
where every place that sentence could quietly become untrue has something
standing in front of it.

## 2. Big rationals from nothing

The verifier has no dependencies. Not "no heavy dependencies" — none. That
constraint is the whole point of the crate, and the first thing it costs you is
arithmetic: a proof about a linear program is a proof about rational numbers,
those rationals get wide, and there is no `num-bigint` to reach for.

So there are two arbitrary-precision arithmetics in this repository. The solver
has one. The verifier has another. They share no code, and they are
deliberately built out of different algorithms.

### Deliberately unlike

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

### The vectors are inert

Both stacks read the same frozen test vectors out of `spec/v1/vectors/arith/`,
and neither of them wrote those vectors. They are text assets in a restricted
TOML subset — restricted because the verifier has to read them with a parser of
about the size a dependency-free crate can carry, so an asset this project's
generator accepts but the verifier's reader would not is an asset the verifier
cannot use. The assets are the specification; the code is not.

Beside the vectors runs a differential campaign of ten million small cases
against machine arithmetic, comparing both stacks against `u128` on inputs
where `u128` is exact.

### The vector that was wrong

`[rational.compare_close]` asserted that 355/113 exceeds 22/7.

Cross-multiplying: 355 × 7 = 2485, 22 × 113 = 2486. It does not.

Worse than the arithmetic error was the stated rationale. The vector claimed to
separate exact comparison from floating-point comparison, and the two values
are far enough apart that binary64 separates them without any trouble at all —
so even had the direction been right, the vector was testing nothing it said it
was testing. It is now `less`, with a second vector beside it that does what
the first claimed: one plus two to the minus fifty-third against one, which a
float comparison calls equal and an exact one does not.

[Decision 0003](SOLVER-SPEC.md#20-decision-log) records it, and the
fix that mattered was not the number. The vector harness now recomputes every
vector it can by a second route, which is what would have caught this without
a human noticing.

### Rationals from text, not from doubles

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

### What the second implementation actually bought

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
in [HC-CASE-0002](case-files/HC-CASE-0002.md), where two readers disagreed
about a malformed number inside a malformed statement, and the fix was a rule —
a line is lexed, and then it is interpreted — rather than a patch to either
side.

Independence is not freedom. Both stacks are answerable to one specification.
What two implementations buy you is a machine for finding the places that
specification is silent.

## 3. Degeneracy, the simplex's haunted house

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

### The response is a ladder, not a fix

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

### The rule the perturbation has to obey

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

### Where the drift check actually looks

The other half of "the numbers stopped being reliable" is knowing when they
did. The obvious implementation asks the factorization how it is doing, which
is asking the suspect for a character reference.

So the residual is computed against the *original columns of the current
basis*, gathered from the problem, not through the factors that would be
answering. If the residual is too large the basis is factored again, and if the
reason was drift rather than an update failure the pricing weights are reset
too — because those weights were estimated from numbers that had stopped
meaning anything.

### The thing exact arithmetic takes away

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
[Decision 0018](SOLVER-SPEC.md#20-decision-log) records why a budget
and not a threshold — a smallest-improvement-worth-taking is a tolerance, and
there is no room for a tolerance inside an exact reduction. It is
[HC-CASE-0004](case-files/HC-CASE-0004.md), and its lesson is the sharpest
one in the catalogue: *the comment was the bug.* The termination argument was
written down, confidently, in the module's own words, and because it was
written down with confidence nobody re-derived it.

### What the ladder is really for

Not speed. The escalation ladder exists so that a search which cannot make
progress says so, by name, at the end of a sequence of graded attempts — rather
than running forever, or reporting a status it has not earned.

The vocabulary this project uses for terminal states has a word for having
nothing to say. Most of the machinery in this file exists to make sure that
word is reachable.

## 4. A proof you can tamper with

The claim this project makes is that an independent program checks the solver's
answers. There is exactly one honest way to demonstrate that claim, and it is
not a passing test suite: it is to break a proof in front of somebody and watch
the checker refuse it by name.

That is what the tamper corpus is, and what the tamper lab in the browser page
is. This essay is about what has to be true of the format and of the verifier
before either of them means anything.

### The verifier's position

`hc-verify` is handed two things: a certificate and the original instance file.
It is handed nothing else — not the solver's model, not its basis, not a dump,
not a hint.

It parses the instance itself, with its own reader. It canonicalizes and hashes
what it parsed, and checks that hash against the one the certificate states.
Everything after that is about the model that hash names, and if the
certificate was detached from its file and reattached to another one, nothing
past the header runs.

Then it reads the records. A field whose name in the reader is `stated` is one
the rules must recompute rather than adopt; the name is a reminder at every use
site. A dual bound states multipliers, and the verifier forms the combination
itself and compares. A cut states coefficients, and the verifier rederives them
from the combination the record names. A transform states the hash of the model
it produces, and the verifier applies the rule itself and hashes the result.

### Why the tree is recursive and not a bag of leaves

The tempting encoding for a branch-and-bound proof is a list of leaves, each
with a bound. It is compact and it is unverifiable: nothing in that list says
the leaves *cover* the feasible region, and a proof that has stopped covering
the region is a proof with a hole in it, which is exactly where a wrong answer
lives.

So the certificate carries the tree. Every node is either split into two
children — at a declared integer variable, at an exact integer, so that
`x ≤ k` and `x ≥ k+1` leave nothing between them — or closed by something
citable: a dual bound, a contradiction, or an integral point attaining its own
bound. The verifier walks that structure and requires every node to have one
parent, to be reachable from the root, and to be branched or terminated exactly
once.

The walk is iterative with an explicit depth cap rather than recursive, so a
legal proof deeper than the process call stack is *checked* rather than crashed
on. A synthetic tree a hundred thousand nodes deep is answered by a rule.

And a leaf must cite an inference made at *its own node*. An inference is sound
and it is about somewhere else; a bound proved in one region says nothing about
another. That is the shape a copied-and-pasted proof takes, and it is one of
the thirty-one entries.

### The corpus

Thirty-one named mutations of genuine certificates, each paired with the
rejection identifier that must answer it — from a repository of 114 live
rejection identifiers, each one a documented rule rather than an error string,
and a gate that refuses the build if one of them stops being reachable.

The pairing is the entire point. A corpus that asserted only "these must all be
rejected" would pass while every one of them was caught by the checksum, which
would establish nothing whatsoever about the proof calculus, and would go on
passing the day a proof rule was deleted.

So **most entries are re-sealed**: the trailing checksum is correct for the
bytes they ship. Anyone holding the format specification can repair a checksum,
so the interesting forgery is the one that is well-formed and still wrong, and
the mathematics has to be what refuses it. The few entries that are not
re-sealed exist to pin the reader's own rules, and they say so.

A sample of what they cover:

- **A negated multiplier.** The single mutation that turns the dual bound rule
  from sound into arbitrary, since a negative multiplier reverses the
  inequality it is applied to. It is checked before the sum is formed, because
  after the sum it is indistinguishable from arithmetic that merely came out
  wrong.
- **A dropped bound multiplier.** The classic omission: a Farkas witness over
  rows only, with variable bounds assumed available by convention. Here a bound
  is a canonical inequality with an identifier exactly as a row side is, there
  is no convention to lean on, and the coefficients fail to cancel.
- **A misstated objective.** The point is genuinely feasible and the number
  attached to it is wrong. The verifier recomputes the objective from the point
  and the model rather than reading it.
- **A fractional split.** Add one half to a branch threshold, and the two
  children stop covering their parent — every integer point in the gap escapes
  the proof.
- **A deleted leaf.** A node the certificate stops accounting for. It applies
  only to a tree that branched, and the manifest says why: removing the single
  leaf of a one-node tree leaves a root, a bound and an integral point
  attaining it, which is a sound linear proof of the same statement. Accepting
  *that* is correct rather than a hole.

### The identifier is the earliest rule, on the models it was written for

An entry names the earliest rule that can see its defect. Earliest matters: an
entry caught late is an entry that got past a check that should have stopped
it, so the corpus pins the *order* of the checks as well as their existence.

But earliest is a property of the model as well as of the mutation. Moving an
incumbent a thousand units is caught by the row check on a model with a bound
in the way and by the objective check on one without. Running the corpus in the
browser, against whatever certificate is on the page, two entries came back
answered by an earlier rule than the corpus pins — correctly.

So the pin is held to on the harness's own models, where it says something
about the order of the checks. What holds on every model, and what the browser
gate asserts, is that a forgery is refused at all. The page shows both: the rule
that answered, and the rule the corpus pins it to, with a sentence when they
differ.

### The lab

The corpus is a product surface rather than a test fixture. In the situation
room you pick an entry and watch it get refused; and then you can do your own.

The field editor lets you change any byte of any record and repair the checksum
— and it repairs it *in the page*, with its own implementation of the
polynomial, rather than asking the module to. That is deliberate. A lab that
asked the solver to re-seal a forgery would be demonstrating that the solver
cooperates with forgery. A lab that seals its own lies is demonstrating that
the verifier survives them.

What comes back is the rule that stopped it and the exact arithmetic behind the
refusal — `HCC-SCOPE-01: inequality 17 is not visible here`, on a certificate
whose checksum is perfectly correct.

### The two the verifier caught for real

The corpus is forgeries. The interesting refusals were not.

[HC-CASE-0005](case-files/HC-CASE-0005.md): the verifier refused a
certificate the solver had just produced. A column held three, above its own
upper bound of one. The model had crossed bounds on a column no row mentioned,
and the empty-column rule chose the nearer endpoint without asking whether the
bounds crossed at all. Both sides decline now — the verifier has its own copy
of the transform rules, and a rule it accepts that the solver refuses is a rule
a forged certificate could still use.

The lesson filed with it is the one to keep. The verifier caught it three
stages downstream, in a check about points, on a model that had been reduced,
solved and lifted since. Had the lifted value landed inside its bounds by
coincidence, the same reduction would have passed.

**The independent checker is a net and not a design.**

## 5. The cut that lies

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

### Two ways to be wrong

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

### Both are checked twice

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

### The campaign that was a shape check

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

### Cuts are the reason to have a node budget

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
[HC-CASE-0006](case-files/HC-CASE-0006.md) is a defect where two of this
program's own subcommands ran different solvers, because one took a derived
default and the derived default had separation off — and the browser surface
took the same default, so the page had been demonstrating this solver with its
cuts switched off. Nobody noticed, because a nine-node tree looks better than a
one-node tree.

### The playbook

The rules for an invalid cut are written down in advance, because it is not a
defect anybody wants to be reasoning about for the first time when it happens:

> Invalidate every descendant certificate that used the pool entry. Disable
> family promotion. Add a sibling tamper case. Then find the bug.

Invalidate first. The certificates are the artifact; a proof built on a bad
inequality is not a proof, whatever it verified as at the time.

### What separates this from a slower solver

None of the above makes the cuts weaker. Gomory mixed-integer cuts and
exact-lifted cover cuts are the ordinary families, derived the ordinary way.

What is different is that every one of them is a *claim* rather than a
convenience: derived in exact arithmetic, scoped in the solver, scoped again in
the verifier, rederived from the combination it names, and held against the
feasible points of a hundred thousand models by a program with no simplex in it.

An invalid cut is the one defect here that a correct-looking answer can hide.
Everything in this file exists because of that sentence.

## 6. Honest scoreboards

Every solver benchmark you have read is a comparison of two numbers produced on
one machine by somebody with a preference. That is not an accusation of bad
faith. It is a description of what the format permits: a table of times, a
geometric mean, a bar chart, and no way for a reader to tell the difference
between an engineering result and a tuning result.

This project publishes a board and takes three things off the table.

### No wall clock, ever

There are no wall-clock comparisons against other solvers anywhere in this
repository and there will not be any.

Not because timing is uninteresting — it is the most interesting thing about a
solver — but because a time is a statement about a machine, and a table of
times is a statement about the machine that generated it. A reader who cannot
reproduce the machine cannot check the table, and a number nobody can check is
the thing this whole project exists to refuse.

So effort is counted. Simplex iterations, nodes opened, cut rounds on the
solver's side; bytes parsed and limb-word steps on the verifier's. Two runs of
the board on two machines spend exactly the same amount, because there is no
ambient clock anywhere in the deterministic perimeter, no unordered container,
no fused multiply-add, and one thread. A budget in seconds would make the
program's own *behaviour* depend on the hardware.

Wall time and peak memory are disclosed as operational measurements. They gate
nothing and they are never divided into anything.

### A status is what the verifier accepted

The board has two columns where most have one. What the search concluded and
what the independent verifier accepted are separate facts, and they can differ:
a search can end `optimal` and have its certificate refused. That is a defect,
and a board that averaged the two into a single number would hide exactly the
defect it should be reporting.

The vocabulary is closed and it includes the honest failures. `optimal`.
`infeasible`. `unbounded`. Feasible with a certified gap — an exact lower
bound, an exact point, and the exact rational distance between them. Feasible
with *no* certified bound, which is a real status: here is a point that
verifiably satisfies your model and I cannot prove anything about how good it
is. And no progress, for a search that has nothing to say.

That fifth status is the one that makes the rest of it honest. A solver without
a word for "I could not prove anything" will always find something to print.

### What is missing is named

An instance that could not be fetched, could not be read, or could not be
solved stays on the board with the reason beside it. It is not dropped, and the
denominator does not quietly shrink.

This matters more than it sounds like it does, because the failure mode of a
harness is not producing wrong numbers — it is producing no numbers and looking
green. A test whose corpus directory is empty passes. A campaign whose
generator stopped generating passes. Every one of those leaves the output
looking exactly the same as success.

So the netlib harness in this repository walks its manifest and reports each
instance it cannot find *by name*, and the mixed-integer harness does the same,
and both check their own scaffolding unconditionally: the manifest parses, the
fetch script names exactly the instances the manifest does, the split names are
disjoint. Whatever is present is run and must not be a finding.

### What this board says today

`docs/scoreboard.md` is generated from the committed corpora by a binary in the
bench crate, and a test regenerates it and diffs, so a change to the solver that
moves an iteration count is a change that updates the board in the same commit.
A board edited by hand fails the suite.

It covers twelve files. Eight linear ones, each solved twice — once by the
revised simplex this project ships and once by a dense textbook tableau that
links none of the production linear algebra, a separation a source gate
enforces. Four mixed-integer ones: every one assembles a certificate the
standalone verifier accepts against the original bytes, three are small enough
that every assignment in their integer boxes is enumerated and the enumerated
answer compared by exact value, and all four are solved with separation and
without and must agree.

Twelve. That is a small number and the board does not dress it up.

### What this board leaves blank, and why that is the honest part

The netlib linear programs and a fixed MIPLIB 2017 subset are pinned:
manifests naming exactly which instances, fetch scripts that record a digest on
first fetch and verify it afterwards, harnesses that run whatever is present.

**No file from either has been fetched into this repository.** So there is no
row for either collection, and the board says that in a paragraph instead of
covering only what is committed and calling that the board.

It would have been easy to do otherwise. Fetch them once, run the harness,
paste the table in. It would look considerably more impressive than twelve
files, and it would be a statement about what one machine downloaded on one
day, published long after anybody could reproduce it. The generator prints that
board on demand, to a terminal, and writes nothing.

The manifests also deliberately do not carry published optimal values. A
cross-check against a published optimum is real evidence and it is only real if
the published value came from a source somebody can name; writing one from
memory would be the exact failure this project is built against. So the file
those values belong in is described, optional, and empty.

### The rule underneath all of it

A board is a claim about a program, and the standard for a claim here is the
same as everywhere else in this repository: somebody else has to be able to
check it.

That is why the numbers are counts rather than times, why the status column
belongs to the verifier rather than to the search, why the absent collections
are named rather than dropped, why the file is generated rather than written,
and why the one number that matters on it — findings: zero — is produced by
programs that do not share ideas with the one being measured.

A scoreboard that flattered this solver would be the easiest thing in the
repository to write, and it would undo everything else in it.
