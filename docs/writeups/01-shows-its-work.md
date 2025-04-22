# The solver that shows its work

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

## What a certificate has to be before it is worth anything

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
than passed over. That is [ADR-0010](../adr/0010-a-verifier-refuses-what-it-cannot-check.md),
and it is the difference between a verifier and a linter.

## Where floating point stops

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

## The part that keeps it honest

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

## What this is not

It is not formal verification. Nothing here is machine-checked against a
specification, and the calculus itself is a design that could be wrong.

It is not two authors. The solver and the verifier share no executable code and
they are structurally unlike each other on purpose, but the same person wrote
both. Two implementations by one author fail differently far more often than
one implementation fails twice — that is the entire benefit being claimed, and
it is less than the benefit of two authors. That sentence is in
[LIMITATIONS](../LIMITATIONS.md) rather than in a footnote, because a trust
boundary you have to infer is not a trust boundary.

It is not fast. There are no wall-clock comparisons against other solvers
anywhere in this project and there will not be any. Work is budgeted in
deterministic counts — simplex iterations, nodes, cut rounds; bytes and limb
operations on the verifier's side — because a budget in seconds makes the
program's own behaviour depend on the machine it ran on.

What it is: a solver where "optimal" is a sentence somebody else can check, and
where every place that sentence could quietly become untrue has something
standing in front of it.
