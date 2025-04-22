---
id: HC-CASE-0006
crate: hc-cli
found-by: the situation room's build step, which replays the recording it ships through the module the page loads
reproduce: cargo test -p hc-cli solving_and_explaining_are_the_same_run
fixed-in: v0.12.3
rejection: none
---

# Two subcommands were running two different solvers

## Symptom

The page that ships with this repository can replay a run recorded natively,
so its build step makes one: `hc solve --cert` writes the certificate and
`hc explain --trace` writes the trace, and the pair is handed to the browser
module before anything is published.

The module refused it. A trace records the length and the trailing checksum of
the certificate its own run assembled, and the two did not match — 312 bytes
against 481, two different checksums. Both artifacts were internally
consistent, both verified, and they were about different runs of the same file.

## Hunt

The obvious explanations were all wrong. The runs were deterministic; the
double-run gate had been comparing every artifact byte for byte in separate
processes for a while and was green. The instance was the same file. The
version was the same binary.

The difference was in the options each subcommand asked for. `solve` builds its
search options from the flags a person typed and `explain` took
`Options::default()`, and those are not the same thing: the default has
separation switched off. So `hc solve` was branching with cuts and `hc explain`
was branching without them, on the same model, and drawing a tree for a run
that the solve had not performed.

Following the number that separated them turned up a second instance of the
same shape. The browser surface also took the bare default, so the page had
been demonstrating this solver with its separation disabled — which is why the
first mixed-integer instance put on it produced a nine-node tree and no cuts at
all, and why nobody thought that was strange.

## Root cause

Two defaults, neither of them wrong on its own, meaning different things.

`Options::default()` in the search is deliberately the simplest search there
is: no separation, so that the campaign comparing cuts-on against cuts-off has
a side to compare against. That is a reasonable default for a library.

`Settings::default()` in the command line was *derived*, which gives every
field its zero — and the command line's own defaults are not zero. Typing
`hc solve file.mps` turns presolve and separation on; the flags that exist are
`--no-presolve` and `--no-cuts`. So the derived default was a configuration no
user of this program has ever run, and every caller that reached for it was
quietly exercising a different solver from the one that ships.

The separation rounds themselves were a bare `4` written at each call site, so
there was nothing for a reader to notice was missing at the fifth.

## Fix

Three changes, and the third is the one that matters.

`hc explain` now runs the same search `hc solve` does. The browser surface does
too.

`Settings::default()` is written out by hand and is what the command line does
when it is told nothing: presolve on, separation on. The comment above it says
why it is not derived.

And the number is one constant in one place, `hc_mip::search::CUT_ROUNDS`, used
by the command line, the browser surface and the harness. A fourth surface that
wants the product's search now has one thing to ask for.

The regression is a test that solves and explains the same instance and
requires the trace of the explanation to name the certificate of the solve —
which is exactly the check that found it, moved into the suite.

## Lesson

A default is a claim about what the thing does when nobody says otherwise, and
`#[derive(Default)]` makes that claim out of whatever the field types happen to
zero to. Here it claimed the program solves without presolve and without cuts,
which was never true of the program and was true of every test that used it.

The deeper one is about what caught it. No unit test could have: each
subcommand was correct about its own run, and each certificate verified. It
took an artifact from one run being placed beside an artifact from another and
something insisting they be about the same thing. The binding between a trace
and its certificate was added so a page could not draw a recording beside an
unrelated proof; it found a defect in the command line the same day.
