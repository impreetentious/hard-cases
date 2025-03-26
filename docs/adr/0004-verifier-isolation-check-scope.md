# ADR-0004: What the verifier isolation check refuses, precisely

## Context

The verifier's independence is the project's load-bearing claim, and the check
that enforces it was written before the verifier existed. It refused any source
line under `crates/hc-verify/` matching an import of a crate whose name begins
`hc_`.

Adding the verifier's own test harness showed the check was refusing the wrong
thing. An integration test lives outside the crate it tests and reaches it by
name, so a test for `hc-verify` necessarily contains `use hc_verify::…`. Under
the original pattern the crate could not be tested at all — and a verifier
whose arithmetic is never held against the frozen vectors is worse than one
that imports something it should not.

A pattern that forbids what it must permit is not a strict check. It is a check
that will be turned off, and the pressure to turn it off arrives at exactly the
moment it is least convenient to think carefully.

## Decision

**The import ban now exempts the crate's own name and nothing else.** Any line
importing a crate beginning `hc_` and spelled other than `hc_verify` still
fails the build. The exemption is a literal name with a word boundary after it,
so a crate named `hc_verify_helpers` would not slip through.

**The same change adds a stronger check the original did not have.** Reading
the manifest and grepping the sources both examine text, and a dependency can
arrive without touching either: through a feature, through a target-specific
table, through a patch section elsewhere in the workspace. So the resolved
dependency graph is asked directly, and the answer must be the crate alone,
with development and build edges included. A verifier whose test harness pulls
in the solver has had its vectors checked by the code those vectors exist to
check.

**A graph that cannot be resolved fails the check** rather than passing it. A
check that silently reports success when it could not run is worse than no
check, because it also reports success in the one case that matters.

Both halves were confirmed by breaking them: an import of a workspace crate,
and a declared dependency on one. Each failed the build, and the dependency
case failed twice — once at the manifest and once at the graph.

## Consequences

The perimeter is narrower where it was wrong and wider where it was thin. The
cost is that the check now invokes the package manager, which makes it slower
and gives it a way to fail for reasons unrelated to isolation; that failure
mode is handled by treating an unresolvable graph as a failure and saying so in
those words, rather than by skipping the check.

The general lesson is worth writing down, because this will not be the last
gate to meet its first real subject: a gate written before the thing it guards
exists is a hypothesis about that thing. When the subject arrives and the gate
refuses something legitimate, the repair is to make the gate say what it means
— and to charge that repair the price of a stronger check somewhere adjacent,
so that narrowing is never free.

## Consequences for the ambient-input ban

The verifier will eventually ship a binary, and a binary reads its arguments
and its input file. The forbidden-API scan currently refuses the standard
library's environment and filesystem modules everywhere under `crates/`
except a named list of host crates, and `hc-verify` is deliberately not on that
list. That collision is real and is not resolved here; it is resolved when the
binary lands, and the resolution will follow the same rule as this one — the
exemption is a single named file, and it is paid for by a check that the file
is the only one in the crate reaching outside the process.

## Alternatives considered

**Put the verifier's tests inside the library as unit tests**, where no import
is needed. Rejected: unit tests can reach private internals, and a harness that
checks the verifier through its private surface is not checking the surface a
caller uses. The vectors should be read the way a consumer reads the crate.

**Move the vector harness into a separate crate.** Rejected: that crate would
depend on `hc-verify`, which is permitted, but it would put the harness outside
the perimeter the isolation check scans, so the harness could then import the
solver's arithmetic without anything noticing.

**Drop the source scan and rely on the dependency graph alone.** Rejected: the
graph catches a declared edge, and the scan catches an import that would fail
to compile — which is the state a change is in while someone is trying to make
it work, and the moment where the temptation to add the edge appears.

## Supersedes / superseded by

Refines the isolation gate established by [ADR-0001](0001-foundation-decisions.md).
No decision is superseded.
