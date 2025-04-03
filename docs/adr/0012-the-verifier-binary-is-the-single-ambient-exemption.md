# ADR-0012: The verifier's executable is the single exemption from the ambient-input ban

## Context

The forbidden-API scan refuses the standard library's environment, filesystem,
process, and thread modules in every crate that is not on a short list of hosts,
and `hc-verify` is deliberately not on that list. The reasons are determinism
and the fact that the verifier is the trusted base: a checker that reads the
environment can behave differently on two machines given the same proof.

A verifier also has to ship as a runnable program, because "verification never
requires linking the solver" is only true if there is something to run. A
program reads its arguments and reads two files.

[ADR-0004](0004-verifier-isolation-check-scope.md) foresaw this collision and
stated in advance how it would be settled: the exemption would be a single named
file, and it would be paid for by a check that the file is the only one in the
crate reaching outside the process. This record is that settlement.

## Decision

**`crates/hc-verify/src/bin/hc-verify.rs` is exempt from the ambient-input ban.
Nothing else in the crate is.**

The exemption is a path, not a pattern. A second binary, a helper module, or a
test that reached for the filesystem would be refused, and the scan says so by
name.

**The exemption is paid for by three checks, all fail-closed.**

1. If the exempted file does not exist, the scan fails. An exemption covering
   nothing is worse than no exemption, because it reports success in the one
   case that would matter.
2. If any other file in the crate reaches outside the process, the scan fails
   and names it.
3. If the exempted file grows past a small line count, the scan fails. A binary
   that accumulated logic would be logic living outside every harness in the
   project — the tests exercise the library, and the executable is a shell.

Both the second and third checks were confirmed by breaking them: a helper
reaching for the environment failed the build, and removing the entry point
failed it too.

**Everything the binary could get wrong lives in the library.** The argument
grammar and the report rendering are both in `vcli`, with tests, because they
are the two things that fail quietly. An option silently ignored would make the
tool appear to have checked more than it did, and a report listing a check that
never ran would be a description of a verification that did not happen.

## Consequences

The verifier remains dependency-free, and the resolved dependency graph is still
checked to be the crate alone with development and build edges included. It now
also produces an executable that a release script can run against a certificate
and an instance file with nothing else on the machine.

The rendering living in the library has a second payoff that is really the
reason for it: the solver's command line will wrap the same API, and the two
front doors are required to produce byte-identical output. One formatter makes
that true by construction; two would make it true on the day they were written.

The line cap on the entry point will eventually be reached if the tool grows
options. When it is, the answer is to move the new logic into the library, not
to raise the cap — the cap is the mechanism that keeps the shell a shell.

## Alternatives considered

**Put `hc-verify` on the host-crate list.** Rejected: that removes the ban from
the whole crate to permit one file, which is the opposite of paying for an
exemption.

**Ship the verifier as a library only and let the solver's command line be the
one front door.** Rejected outright. It would make verification require the
solver on the machine, which is the single property the project is arranged
around.

**Put the argument parsing in the binary where it naturally lives.** Rejected:
it is the part of a command-line tool most likely to be wrong in a way nobody
notices, and it would sit outside the test harness.

## Supersedes / superseded by

Settles the open consequence recorded in
[ADR-0004](0004-verifier-isolation-check-scope.md). No decision is superseded.
