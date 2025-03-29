# ADR-0008: The certificate's byte-level framing

## Context

The format freeze fixed what a certificate contains: the header fields, the
closed list of record kinds, what each record's payload holds, the order
records must appear in, and the rules a reader applies. It fixed the encoding
of the pieces — little-endian fixed-width header fields, minimally encoded
variable-length integers, rationals as length-prefixed canonical text, arrays
as a count followed by elements.

It did not fix how a record is framed on the wire, or the exact field order
inside each payload. Two implementations could satisfy every stated rule and
produce byte streams neither could read. That is precisely the class of gap the
freeze exists to close, so it is closed here rather than by whichever reader is
written first.

## Decision

**A record is a type byte, then its identifier, then its payload length, then
the payload.** The identifier and the length are minimally encoded
variable-length integers. The type byte first means an unknown record kind is
refused before anything is sized; the length before the payload means a reader
can check the declared size against a cap and against what remains, and can
verify afterwards that the payload consumed exactly what it declared.

**Record identifiers are positional**, starting at zero and increasing by one.
They are written out rather than implied by position, because the check that
they are strictly increasing is one a reader must perform against the bytes it
was handed — a reader that assumed the position could not detect the stream
that lies about it.

**Each payload's fields appear in the order the record's specification lists
them.** Where a field is optional, it is preceded by a single byte: zero for
absent, one for present. A presence byte rather than a sentinel value, because
every sentinel is a legal value of the type it hides in.

**The header is: magic, format version, canonicalization version, format tag,
instance byte length, instance digest, claim kind, declared record count,
declared byte length** — four, two, two, one, eight, thirty-two, one, eight,
eight. The declared byte length counts the whole file including the header and
excluding the four checksum bytes, so a reader can compare it against what it
holds without arithmetic.

**The checksum is the last four bytes and covers everything before them.** It
is checked first and it establishes nothing about the mathematics; a truncated
file should be diagnosed as truncated rather than as an invalid inference.

## Consequences

Two independently written readers can now agree, which is the only reason this
record exists. The framing is redundant in one place on purpose — a record
declares its length and its payload is self-delimiting — and that redundancy is
the check that catches a payload whose fields were written in the wrong order,
which would otherwise be read as a valid record with different contents.

The cost is a few bytes per record. On a certificate large enough for that to
matter, the record count is already against its cap.

No contract version moves. Nothing that version 1 said changes; what changes is
that a question version 1 left open now has an answer, and both implementations
are held to it by the vectors rather than by having been written together.

## Alternatives considered

**A self-describing container.** Rejected without much thought: a format with a
schema in it is a format with two things to get wrong, and the record list is
closed precisely so that nothing has to be described at run time.

**Length-first, then type.** Rejected: an unknown record kind should be refused
before its declared length is used for anything, and putting the type first
makes that ordering the natural one rather than a discipline.

**Implicit identifiers from position.** Rejected as above. The identifier is
what citations refer to, so a reader has to check it rather than compute it.

**Sentinel values for optional fields.** Rejected. There is no rational that
cannot legitimately appear, so any sentinel would be a value the format could
not carry.

## Supersedes / superseded by

Completes the certificate encoding frozen by
[ADR-0002](0002-format-freeze-decisions.md). No decision is superseded.
