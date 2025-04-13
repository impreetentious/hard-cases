---
id: HC-CASE-0003
crate: hc-fuzz
found-by: the fuzz corpus, cross-format invariant
reproduce: cargo run -p hc-fuzz --bin hc-fuzz -- --from 8709371129873690708 1
fixed-in: v0.11.7
rejection: none
---

# The generator wrote a sign the reader was right to read

## Symptom

The parser target renders one drafted model in all four accepted readings and
requires the four to produce the same canonical model. Its first run reported
that a maximizing draft came back with an objective constant of `5` when read
as MPS and `-5` when read as LP or as JSON.

## Hunt

The suspicion was the readers, and it was wrong. The MPS convention for the
objective constant is that a right-hand-side entry on the objective row carries
its *negation*; the maximizing case then negates the whole objective a second
time at the end, as every format does. The other two formats carry the constant
directly, so they take only the second negation.

Working it through: with `c` the constant as the file states it, an MPS file
must write `-c` on the objective row, and it must do so whichever direction the
file states, because the second negation applies to both sides of the identity
equally. The generator was writing `-c` when minimizing and `c` when
maximizing.

The three readers were all correct. So was the writer, which had a round-trip
test holding it to exactly this.

## Root cause

A fourth implementation of the same convention, in the fuzzer's own renderer,
written from the same reasoning that produces the bug: *the file maximizes, so
something must flip.*

## Fix

One line, and a comment saying why the sign does not depend on the direction.

## Lesson

A harness is code. This one was found in its first minute because the invariant
it checks — one model, four files — is strong enough to catch its own author,
and because the defect it found was in the half nobody would have suspected.

A cross-format invariant is worth more than four per-format tests for exactly
this reason. Each format checked against its own expectations agrees with
whatever misunderstanding produced those expectations. Four formats checked
against each other cannot all be wrong in the same direction unless the same
mistake was made four times.
