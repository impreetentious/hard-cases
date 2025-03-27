# ADR-0006: Accepted instance input is ASCII with line-feed endings

## Context

Writing the first instance reader forced a question the format rules had left
open: what happens to a byte that is not text?

The specification says a great deal about which sections exist, which bound
types are accepted, and how a range interacts with a row sense. It says nothing
about a byte of value 200, a carriage return, or a null. A reader has to have an
answer, and "whatever the standard library does when asked to treat these bytes
as a string" is not an answer a second implementation could be relied on to
reach independently.

Two constraints already in the frozen rules turn out to settle it.

The canonical dump is ASCII, and it writes names into itself verbatim — a name
is the rest of its line precisely so that a name containing a space needs no
quoting. So a name that cannot be written in ASCII cannot appear in a dump, and
a model whose dump cannot be written is a model the two parsers cannot compare.
The restriction is derived from the comparison rather than chosen for tidiness.

The instance digest covers the file's exact bytes with no normalization of any
kind — no newline conversion, no whitespace strip, no byte-order-mark removal.
So a file with carriage returns is a different instance from the same file
without them. A reader that accepted both and normalized would make two
different digests produce one model, which is the single thing the digest exists
to prevent.

## Decision

**Accepted input is ASCII with line-feed line endings.** A byte at or above 128,
a carriage return, and every other control character except the line feed and
the tab are refused before any line is examined.

**A new rejection identifier, `HC-INSTANCE-29`, names that check.** The
identifier table is append-only by design and this is what appending is for. No
existing identifier fits: none of them is about the byte-level shape of the
input, and reusing one would make the tamper corpus and the case files ambiguous
about which check fired.

**Neither the canonicalization version nor any fixture moves.** The test applied
was: does any file's meaning change? No file that version 1 accepted is now
refused — every frozen vector is ASCII with line-feed endings — and no file it
refused is now accepted. What changes is that a case which was previously
undefined now has a defined answer, in writing, that both implementations can be
held to. A version bump would invalidate every canonical dump in the corpus in
exchange for nothing.

The tab is accepted because the free form is whitespace-delimited and a tab is
whitespace. In the fixed form a tab occupies one column like any other
character, and the window checks treat it as blank, so it cannot smuggle content
past a field boundary.

## Consequences

The reader has a defined answer for arbitrary bytes, which is a precondition for
pointing a fuzz target at it — and a fuzz target is exactly where this gap would
otherwise have been discovered, as a panic rather than as a rejection.

The cost is real and worth naming: instance files that use a non-ASCII character
in a name are refused, and so are files stored with carriage returns, which is
the ordinary state of a file that has been through a checkout on a system that
rewrites line endings. The repository's own attributes file already pins these
files, and the refusal is explicit rather than silent, so the failure mode is a
message naming the rule rather than a model that differs from the one on disk.

The general lesson is the one this project keeps rediscovering: a rule that is
merely implied is not a rule that two independent implementations will agree on.
The implication here was strong enough that both would probably have landed in
the same place — and "probably" is exactly the standard the second
implementation exists to replace.

## Alternatives considered

**Accept UTF-8 and let names carry any character.** Rejected: the dump is ASCII,
so either the dump would have to change — a real contract version bump, with
every fixture regenerated — or names would have to be escaped in the dump, which
would undo the property that a name needs no quoting because it is the rest of
its line.

**Accept carriage returns and strip them.** Rejected outright. The digest covers
the raw bytes, so stripping would let two files with different digests produce
the same model, and a certificate bound to one of them would appear to be about
the other.

**Reuse an existing identifier.** Rejected: the identifiers are a public
interface, each names exactly one check, and the tamper corpus pins the order in
which checks fire. An identifier that means two things is one the case files
cannot cite precisely.

## Supersedes / superseded by

Extends the format rules frozen by [ADR-0002](0002-format-freeze-decisions.md).
No decision is superseded.
