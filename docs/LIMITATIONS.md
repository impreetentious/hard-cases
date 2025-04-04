# Limitations

What this project does not do, cannot do, and does not yet do — stated plainly,
because a claim about correctness is worth exactly as much as the boundary
drawn around it.

This file is part of the release, not a footnote to it. It is updated in the
same change as whatever moves a line in it.

## The trust boundary

**What the verifier's acceptance means.** `hc-verify` decides whether a
certificate is a valid proof under the published calculus. It does not decide
whether the calculus is right, and it does not decide whether its own
implementation is right. Those are separate questions and this project answers
neither by fiat.

**What is inside the trusted base.** The verifier's own source at the printed
revision, the pinned Rust compiler and standard library, and the target's
integer and memory semantics. Nothing else. The solver, the certificate
assembler, and everything that produces a certificate are outside it: their
being wrong can cause a certificate to be rejected, and cannot cause a wrong
certificate to be accepted.

**What independence means here, and what it does not.** The verifier shares no
executable code with the solver — its own arbitrary-precision arithmetic, its
own parser, its own model, its own rules, and no dependencies at all. It does
not have independent *authorship*: the same person wrote both sides. Two
implementations by one author fail differently far more often than one
implementation fails twice, which is the entire benefit being claimed, and it is
less than the benefit of two authors. It is also not formal verification: no
part of this is machine-checked against a specification.

## Where floating point stops

The search runs in binary64 and nothing it computes is ever a conclusion. Every
bound in a certificate is an exact rational inference over the original model's
exact values; every point is checked exactly. A floating-point result that
cannot be rebuilt exactly produces a weaker true statement, not a stronger
unproved one.

The exact values come from the file's decimal text rather than from a float
round-trip, so a coefficient written `0.1` is one tenth and not the binary64
nearest to it. A model whose exact values do not survive a round-trip into
binary64 is rejected rather than solved, because the search would otherwise be
working on a different model than the proof is about.

## What the verifier checks today

Certificates for linear programs: an optimal claim, an infeasibility, an
original-space unbounded ray, a certified gap between an exact bound and an
exact point, and a point carrying no bound at all.

Certificates for cut-free branch-and-bound trees: the recursive cover, replayed
iteratively with an explicit depth cap so a legal proof deeper than the process
call stack is checked rather than crashed on. Every node must have one parent
and be reachable from the root, every node must be either branched or terminated
exactly once, every leaf must cite an inference made at its own node, and every
branch must split a declared integer variable at an exact integer.

## What the verifier does not check yet

Presolve transform replay and cut derivations. A certificate carrying either of
those records is **refused**, and refused with a message saying that this
implementation does not close it — see
[ADR-0010](adr/0010-a-verifier-refuses-what-it-cannot-check.md) for why that is
a refusal rather than a skip. Each of those rules brings its own hand-worked
vectors when it lands.

## What the campaigns establish

Two campaigns run on every change and again at a larger size in the scheduled
build. The certification campaign generates linear programs, solves them,
assembles certificates, and requires the standalone verifier to accept every one
and the accepted claim to agree with a dense reference solver. The tiny
integer campaign does the same for small integer programs and adds a third
answer: enumeration over every point in the box, which shares no idea with the
search at all.

They establish that the implementations agree on the instances generated. They
do not establish anything about instances outside those shapes, and both
campaigns publish what they saw — how many of each claim, and how often
assembly declined — so that a run which quietly stopped certifying would be
visible rather than clean.

## What the tamper corpus establishes

`tamper/manifest.toml` names a set of mutations of genuine certificates, each
paired with the exact rejection identifier that must answer it, and a gate runs
every one of them against the standalone verifier on every preflight. It
establishes that those mutations are caught, by the rule that should catch them.
It does not establish that no other mutation exists — no finite corpus can — and
an incident that turns one up becomes an entry rather than a footnote.

## What the solver does not do

- No quadratic, conic, or nonlinear anything. No special ordered sets, indicator
  constraints, or semicontinuous variables. No callbacks, no lazy constraints,
  no column generation.
- Single-threaded search. Parallelism exists only across instances in the bench
  harness.
- No presolve probing and no symmetry detection.
- No modelling language. Files in, results out.

## What the input readers accept

Fixed-form and free-form MPS, with the bound types `LO UP FX FR MI PL BV LI UI`
and no others. Three readings that the format leaves genuinely ambiguous are
**rejected rather than guessed**: a negative `UP` bound on a column still at the
default lower bound, the implicit binary upper bound on an otherwise unbounded
integer column, and the reading of `MI` that also zeroes the upper bound. This
costs instance coverage, deliberately, because a solver that guesses at the
model is proving something about a model nobody wrote.

At most one named vector is accepted in each of the right-hand-side, ranges, and
bounds sections. Repeated records under that one name are ordinary; a second
distinct name is refused rather than silently chosen between.

## What the receipt publishes

`./scripts/receipt.sh` measures the trusted base at the moment it runs: the
files, the counted lines, the hash of that exact source, the declared
dependencies, and the uses of unsafe code. It then verifies one real
certificate and reports what that cost. Nothing in it is transcribed from a
previous run, and a receipt taken over a working tree with uncommitted changes
says so, because otherwise it would name source that is in no commit.

## What a work figure means

The verifier reports its cost in work units: one per byte parsed and one per
limb-word step of arithmetic. The figure is an **upper bound** rather than an
exact tally — a reduction to lowest terms takes a number of steps that depends
on its inputs, and it is charged its widest case. See
[ADR-0011](adr/0011-verifier-work-is-an-upper-bound-charged-at-a-metered-facade.md)
for why a ceiling is the safe direction. The figure is deterministic: the same
certificate and instance produce the same number on every machine.

## Sizes and caps

A certificate that exceeds a published size, bit, or work cap does not receive a
proof-bearing status. The observed value and the limit are both reported. Caps
are absolute and are never expressed as a ratio against how long the solve took.

## On speed

There are no wall-clock comparisons against other solvers anywhere in this
project, and there will not be any. Work is budgeted in deterministic counts —
simplex iterations, nodes, cut rounds on the solver's side; bytes and limb
operations on the verifier's. Wall time and peak memory are disclosed as
operational measurements and gate nothing.
