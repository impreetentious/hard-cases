# ADR-0002: Decisions made in freezing the v1 formats

## Context

ADR-0001 decided that the proof calculus would be frozen as inert assets before
any solver code existed. Carrying that out surfaced a set of choices that
ADR-0001 named in principle but did not settle in detail: how the assets are
encoded, how MPS ambiguities are resolved, what the canonical dump looks like
byte for byte, and how the certificate is laid out on disk.

Each of these touches a file format or the certificate, so under the ambiguity
rule none of them may be settled by a comment at a call site. They are recorded
here together because they were made together, and because reversing any one of
them requires the same machinery: a version bump on the contract that moved and
regenerated fixtures.

## Decision

**The assets are valid TOML restricted to a small subset.** Table headers,
single-quoted literal strings, decimal integers, booleans, and single-line arrays
of literal strings. No basic strings, inline tables, arrays of tables, dotted
keys, or trailing comments.

The constraint driving this is that `hc-verify` has no dependencies and must be
able to read the specification it implements. A specification the verifier can
only read through a parser it is forbidden to link is not a specification it has.
The subset is small enough to read in a few dozen lines on either side of the
boundary, and it stays valid TOML so ordinary tooling still works on it.
Literal strings rather than basic strings because the cut formulas are full of
characters that a basic string's escape rules would silently consume.

**Format tags are 1 for fixed MPS, 2 for free MPS, 3 for the LP subset, and 4
for JSON.** The tag is prepended to the raw bytes before hashing. Fixed and free
MPS get different tags specifically because a file can be legal under both and
mean different things, and the digest must not be able to say they are the same
model.

**An MPS right-hand side entry on the objective row supplies the negation of the
objective constant.** This is the convention the major readers share, and it is
the disagreement in the format most likely to shift every reported objective by a
constant without anything looking wrong.

**Three MPS readings are rejected rather than chosen.** A negative `UP` bound on
a column still at the default lower bound of zero; the convention giving an
unbounded integer column an implicit upper bound of one; and the historical
reading of `MI` that also sets the upper bound to zero. Real readers disagree on
the first two and the third has no modern users. Rejecting costs instance
coverage and is the honest position: the alternative is picking one reading and
solving a model the author may not have written. The lenient mode applies the
negative-infinity reading for the first case and labels the parse, and a lenient
parse cannot carry a certificate unless the verifier's strict parser
independently accepts the same bytes.

**The canonical dump is a line-oriented ASCII rendering compared as bytes**, with
names last on each line so a name containing a space needs no quoting, nonzeros
ordered by column and then row to match the compressed-column storage, and an
explicit `zerosum` line where duplicate entries cancelled. The cancellation note
exists because a cancelled entry and an absent entry are the same matrix and
different files, and two parsers must agree about which one they read.

**The certificate header is fixed-width and the record types are numbered.**
Magic, two version fields, the format tag, the instance byte length, the digest,
the claim kind, and the declared record and byte counts, followed by
type-length-payload records. Record type codes are grouped by kind so a new
record within a group does not disturb the others. LEB128 values must be
minimally encoded: a second spelling of a number is a second byte stream for one
proof, and determinism means there is exactly one.

**Instance-parsing rejections get their own identifier group, `HC-INSTANCE`,
alongside the nine `HCC` groups.** The differential test between the two
independent parsers has to compare more than accept against reject; when both
reject, it has to know whether they rejected for the same reason. That requires
shared identifiers for instance parsing, which the certificate groups do not
provide.

## Consequences

The subset restriction means the asset reader is written twice, once on each side
of the verifier boundary, which is the same duplication the project accepts
everywhere else and for the same reason. The generated documentation is checked
by regenerating into a temporary directory and diffing, so a rule and its prose
cannot drift; editing a page under `docs/formats` has no effect and fails
continuous integration.

The MPS rejections will show up as instances this reader declines that other
readers accept. That is a real cost, it will be visible on the scoreboard, and
`docs/LIMITATIONS.md` names it rather than leaving it to be discovered.

Every decision here is versioned. Changing the canonical dump, the digest input,
or an MPS reading bumps the canonicalization version; changing the header, a
record code, or an encoding rule bumps the HCC version. Either one requires
regenerated fixtures under `spec/v1/vectors/` and `tamper/`.

## Alternatives considered

JSON or a bespoke line format for the assets, both rejected because TOML tables
already express what is needed and a bespoke format would need its own
specification. YAML, rejected because its parsing rules are larger than the
verifier's entire trusted base.

Sniffing fixed against free MPS from the file content, rejected because it would
make the format tag, and therefore the instance digest, depend on a heuristic.

Accepting the ambiguous MPS bound readings with a recorded choice, rejected
because the record would document which reading was taken without making the
resulting model any more likely to be the one the author meant.

A certificate encoding in a self-describing format such as CBOR, rejected because
unknown-field tolerance is a reasonable property for data interchange and a hole
in a proof format, and because it would add a dependency or a second parser to
the trusted base.

## Supersedes / superseded by

Extends ADR-0001, which decided that the freeze would happen. Nothing here
reverses it.
