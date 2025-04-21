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

Unboundedness of a mixed-integer model, which is a stronger statement than
unboundedness of its relaxation and is checked as one. The certificate carries
an integral feasible point, a recession direction, and a positive integer step;
the verifier requires that the step be whole, that it clear every denominator
the direction holds on an integer column, that the base point be integral where
the model declares it, and that the objective strictly fall along the direction.
A relaxation's ray offered on its own is refused.

Certificates for branch-and-bound trees, cuts included: the recursive cover, replayed
iteratively with an explicit depth cap so a legal proof deeper than the process
call stack is checked rather than crashed on. Every node must have one parent
and be reachable from the root, every node must be either branched or terminated
exactly once, every leaf must cite an inference made at its own node, and every
branch must split a declared integer variable at an exact integer.

Certificates whose proof begins with a presolve reduction: every stage is
recomputed from the original bytes by a second implementation of the eight
transforms, the hash each record states is compared against what that
recomputation produces, and the original-space witness is checked against the
original model and then mapped forward through the chain onto the reduced one.

## What the verifier does not check yet

Nothing, at present. Every record kind the format defines now has a rule that
closes it, cut derivations included: a Gomory or cover record is recomputed from
the node's own inequalities rather than read, and a derivation that does not
reproduce is refused.

The arrangement that made this section necessary has not gone away. A record
kind this implementation cannot close is **refused**, with a message saying so,
rather than skipped — see
[ADR-0010](adr/0010-a-verifier-refuses-what-it-cannot-check.md) for why a
refusal is the honest response and a skip is not. The section stays because the
format is expected to grow again, and a rule that lands without its hand-worked
vectors lands here first.

## What the committed corpora establish, and what is absent

Twelve instance files are committed and gates consume all of them. Eight are
linear and are solved twice, once by the revised simplex and once by a dense
textbook tableau that links none of the production linear algebra. Four are
mixed-integer: every one must assemble a certificate the standalone verifier
accepts, three are small enough that every assignment in their integer boxes is
enumerated and the enumerated answer is compared by exact value, and all four
are solved with separation and without and must agree about the value.

They are small, and they are small on purpose: an instance a person can hold in
their head is an instance whose answer a person can check. They establish
nothing about the shapes they do not contain.

**The two fetched collections are absent.** The netlib linear programs and a
fixed MIPLIB 2017 subset each have a manifest naming exactly which instances
they cover, a fetch script that records a digest on first fetch and verifies it
afterwards, and a harness that runs whatever is present and names what is not.
No file from either has been fetched into this repository, so no result from
either appears anywhere in it, and `docs/scoreboard.md` says so rather than
covering only what is committed and calling that the board. Until they are
fetched and run, every claim this project makes about how it behaves on
instances it did not generate rests on twelve hand-written models.

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
- An unbounded relaxation found under presolve produces no unboundedness claim.
  The certificate's ray form is checked directly against the original bytes and
  admits no transform records, and a direction does not lift through a transform
  chain the way a point does — lifting is defined on points, and a direction is
  the homogeneous part of one. Rather than invent a lift, such a search reports
  no progress. Solving the same model with presolve off produces the claim.
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

The LP-format subset is narrower than the files in the world. **Every objective
and every constraint carries a label**, and a line that does not begin with one
continues the statement above it; that is what makes the continuation rule
total, and it refuses files whose constraints were relying on a generated name
nobody wrote down. A constant inside a constraint is refused rather than moved
across the operator. A one-sided bound names its variable first, so `0 <= x` has
to be written `x >= 0`. An infinity spelling is a bound endpoint and nothing
else: in a coefficient or a right-hand side it is a malformed number, so there
is one numeric grammar across all three formats rather than one with a local
exception. The semicontinuous, special-ordered-set, indicator, and quadratic
sections are named and refused rather than ignored.

The JSON schema has no optional members and no floating-point literals. Absence
is spelled `null`; a member that is simply missing is refused, because a default
is a second reading of the same document. **No model value is a JSON number** —
coefficients, bounds, right-hand sides, ranges, and the objective constant are
all decimal strings, because a JSON number is a binary64 in nearly every reader
and a value that has passed through one is not the value the document wrote.

The format is named on the command line and never guessed, from the content or
from the file name. It is part of the instance digest, so the same bytes read
under two formats are two instances, and a heuristic there would make the
identity of a model depend on a guess.

The writers refuse what they cannot spell exactly. A name carrying whitespace
has no free-form MPS spelling and no LP spelling; a coefficient whose
denominator carries a prime other than two or five has no finite decimal
expansion, and there is no rounding available on a path a proof is about. Such a
model is refused rather than approximated, and the JSON schema — which quotes
names and carries every value as text — is the one that can usually still take
it.

## What the browser surface is

One WebAssembly module. Three entry points produce results — a bounded live
solve, a replay of a recorded trace and certificate, and the verifier alone
behind a thin wrapper — and every result says which of the first two produced
it, because a panel that could not tell a live solve from a recording would
label one as the other.

Three more exist to look at things rather than to produce them. One runs the
root relaxation under the textbook pricing rule and reports every pivot it
made; it is the ordinary search with a recording trail attached, not a second
implementation, and nothing it reports is exact or is a claim. One reads a
certificate through the verifier's own reader and reports the structure beside
the verdict, so the page draws what the checker saw rather than what the
assembler meant. One applies a named mutation from the tamper corpus and
re-verifies, so a visitor can break a proof without a toolchain.

The page's node budget is the only limit on a live solve, and it is a limit on
nodes rather than on seconds so that the same instance behaves the same way on
every machine. A solve that reaches it produces a certified gap rather than an
unproved number.

The page will also take a model of the reader's own. The format is named on a
control rather than guessed from the file, for the same reason the command line
names it: the format tag is part of the instance digest, so the same bytes read
two ways are two instances. Such a file is read by the browser and handed to
the module in the same tab; there is no server behind the page and nothing is
uploaded, so the address bar — which otherwise carries the whole view — cannot
carry it, and the share control says so instead of producing a link that would
open on something else.

A replay is refused unless the trace is about *this* certificate as well as
about this instance: a trace records the length and the trailing checksum of the
proof its own run assembled, and two genuine artifacts of two different runs of
the same model are the easiest facade to assemble here by accident.

A replay rechecks the certificate rather than displaying the verdict it was
recorded with. A recorded verdict is a claim about a run nobody in the room
performed.

The verifier being compiled into the same module as the solver is not the same
as sharing anything with it. The crate has no dependencies, its own arithmetic,
its own parsers, and a gate that fails the build if either stops being true.

There is no binding generator and no glue crate. The exports are plain
functions over a pointer and a length, answers are UTF-8 behind their own
length, and the page reads them out of the module's memory in about forty
lines. Those forty lines are the ones the gate runs.

**Unsafe code exists in exactly one file**, the one holding that C interface,
and a scan fails the build if a second crate allows it or if that file stops
being the only place it appears. Everything the unsafe blocks do is turn a
pointer and a length the caller received from this module back into the slice
it came from.

## What a trace is, and is not

A trace is what the search did. It is not a proof and it never becomes one: a
reader learns which nodes were opened and in what order, which cuts entered the
pool, where the incumbent came from, and learns nothing about whether the answer
is right. The certificate is the only thing that says that.

Everything in a trace comes from something the run recorded. An event the
assembler cannot source is an event it does not write, and a schema field it has
no value for is a reason not to write that event at all — so a trace is short
where a run was, and no panel reading one can draw a number nobody measured.

Two of the events the schema reserves are not produced. The linear search counts
its escalations but does not record which rung each one reached, so no
escalation event can be written. The tree search tracks no running global bound
— it compares each node against the incumbent and never forms the minimum over
the open ones — so no search-gap event can be written either. Both could be
computed after the fact from what *is* recorded, and computing them is exactly
what this rule forbids: an event assembled from a derivation is an event about a
search that did not report it. The schema carries them because each has a name
and a place, and the assembler writes neither until the search keeps them.

What the assembler does write, it writes from exact quantities the search
already holds: a node closed by a bound carries the bound the basis established,
an integral leaf carries the objective its point attains, and a branch carries
the integer the split was taken at. Those are rationals, they are the numbers
the certificate cites for the same nodes, and none of them is recomputed here.

## What the determinism gate compares

Every artifact is produced twice, in two separate processes, and compared byte
for byte: the canonical dump, the certificate, the solve report, the
explanation, the trace, and the verifier's verdict and work units. Two
processes rather than two calls in one, because an in-process repeat shares the
allocator state and the address-space layout that this gate exists to catch.

It runs on one machine at a time. Equality between platforms is asserted by
construction — no ambient clock, no environment, no unordered container, no
fused multiply-add, one thread — and is not yet measured by a job that runs the
same instance on two of them and compares. Until that exists, cross-platform
determinism is a design claim here and not a tested one.

## What the fuzzing establishes, and what it does not

The fuzz corpus is generated from seeds rather than grown from coverage
feedback, so every case is a function of its seed and reproduces anywhere by
running that seed. What it checks is that no reader panics, that the two
independent readers agree on every input — the same canonical dump where both
accept, the same identifier where both refuse — that one drafted model rendered
in all four accepted readings produces one model, and that a damaged certificate
is answered with a verdict or an identifier rather than a crash.

**There is no coverage feedback.** The corpus reaches what its generators were
written to reach. A region of a reader that nobody thought to emit for is a
region nobody is fuzzing, and no counter here will report that. The decision and
what it buys are recorded in ADR-0017.

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
