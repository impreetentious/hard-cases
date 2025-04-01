# ADR-0009: A model's hash is the digest of its canonical dump

## Context

A certificate's root record names the model its tree is about. After presolve
that model is not the one in the file — a chain of transforms has been applied
to it — so the name cannot be the instance digest, and both implementations
have to compute the same name from a model neither of them can hand to the
other.

There is exactly one rendering of a model that two independently written
implementations already agree on byte for byte: the canonical dump. It exists
for precisely this purpose in a different guise, as the thing the two parsers
are compared on, and it is total — every ordering in it is forced, so there is
never a choice to make and never a place where two implementations could each
be reasonable and differ.

## Decision

**A model's canonical hash is SHA-256 over the bytes of its canonical dump**,
with nothing prepended and nothing appended. The rule is added to the
canonicalization asset, where the dump is already specified, so that it is
frozen alongside the thing it hashes rather than living in whichever
implementation was written first.

**It is distinct from the instance digest and both continue to exist.** The
instance digest names a *file*: the format tag followed by the file's exact
bytes, which is what binds a certificate to the thing on disk. The model hash
names a *model*, which is what a root record needs after transforms have
changed it. For a model read straight from a file with no transform applied,
both exist and both are checked, because they answer different questions and a
certificate that got one right and the other wrong would be a certificate about
the wrong thing in a way neither check alone would notice.

No contract version moves. The canonicalization rules that existed are
unchanged; a rule that was needed and absent has been added, and no dump's
bytes differ as a result.

## Consequences

The root record is now checkable by an implementation that shares nothing with
the one that wrote it, which is the property the whole verifier arrangement
exists to have. The cost is that computing a model hash renders the dump, which
is proportional to the model rather than to a summary of it; on the sizes the
caps admit that is not the expensive part of verification, and it buys the
property that the hash cannot drift from the comparison the two parsers already
perform.

There is a second, quieter benefit worth naming. Because the hash is over the
dump, any disagreement about a model hash can be reduced immediately to a
disagreement about the dump — and a dump difference is a diff a person can
read, not a hash that says only that something is wrong somewhere.

## Alternatives considered

**Hash an implementation's model structure directly.** Rejected outright. The
two implementations do not share a model type, so this would require
specifying a serialization anyway — which is the canonical dump with a worse
name and no second use.

**Reuse the instance digest for the root.** Rejected: it names a file, and
after presolve the root is not about a file. Using it would make the root
record either wrong or meaningless the moment a transform is applied.

**Hash only the parts of the dump a proof reads.** Rejected. The subset would
have to be specified, it would drift as the calculus grows, and the property
being bought — that the hash is over something both sides already agree on — is
lost the moment the hashed thing stops being the thing they compare.

## Supersedes / superseded by

Extends the canonicalization rules frozen by
[ADR-0002](0002-format-freeze-decisions.md). No decision is superseded.
