---
id: HC-CASE-0001
crate: hc-io, hc-verify
found-by: the byte-edit sweep in the cross-parser differential
reproduce: cargo test -p hc-io --test cross_parser_lp
fixed-in: v0.11.4
rejection: HC-INSTANCE-27
---

# A file with two defects was refused for whichever check ran first

## Symptom

The differential mutates each frozen vector one byte at a time and hands both
LP readers the result. On the very first run it stopped on a truncation of
`tiny.lp`: the solver's reader refused it as `HC-INSTANCE-27`, a malformed
statement on line 4, and the verifier's refused it as `HC-INSTANCE-26`, input
ending before the terminator.

Both refusals were true. The file had a malformed statement *and* no
terminator.

## Hunt

The two readers were built differently on purpose — one carries a pending
statement and extends a token vector as lines arrive, the other cut the file
into logical lines in one walk and interpreted them in a second. The
disagreement came straight out of that difference: the second walk could not
begin until the first had finished, so a structural fact about the end of the
file was established before any statement was looked at.

The specification is not silent here. Every rejection identifier names *the
earliest point at which the defect is detectable*, and the tamper corpus pins
the order of the checks as well as their existence. A malformed statement on
line 4 is detectable on line 4. A missing terminator is not detectable until
the input ends.

So the solver was right, and the two-pass reader was reporting the later of two
true things.

## Root cause

Interpretation deferred behind structure. The verifier's reader returned its
structural refusals from the first walk, which ran to the end of the file
before the second walk interpreted anything.

## Fix

The first walk stopped returning structural refusals and started handing them
back beside what it had gathered. Every statement it gathered began on an
earlier line than the defect did, so the statements are interpreted first and
the structural refusal is reported only when they are all sound.

The sweep then found two more disagreements of the same kind, in the bound
statements and the integrality lists, and those were closed the same way: by
deciding which of two true things is the earlier one and making both readers
say it.

## Lesson

Two implementations that disagree about *which* rule refused a file have not
disagreed about the file. They have disagreed about the order of the rules, and
the order is part of the specification — it is what a tamper case pins when it
says a forgery must be caught at its intended earliest rule.

An identifier is a public interface. The check that owns it is a claim about
when a defect becomes knowable, and two readers that answer differently are
telling a reader of the output something false about the input.
