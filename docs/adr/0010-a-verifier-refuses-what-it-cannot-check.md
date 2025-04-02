# ADR-0010: A verifier refuses record kinds it cannot yet check

## Context

The certificate format is frozen and complete: it has records for presolve
transforms, cuts, branches, and leaves, because the format had to be settled
before any of the machinery that produces those records was written. The
verifier is not complete and cannot be, because it is built in the same order
as the thing it checks.

That leaves a gap with two possible shapes, and they are not equally safe. A
verifier can ignore a record kind it does not understand, or it can refuse the
certificate that carries one.

Ignoring is the ordinary choice in data formats and it is catastrophic here.
Consider a certificate whose transform chain reduces a model and whose bound is
proved on the reduced model. A verifier that skipped the transform records
would check the bound against the *original* model, find that the multipliers
happen to sum to something, and either accept a proof about a different problem
or reject a sound one for the wrong reason. Neither outcome is a verification.
The general form of the hazard is that proof records are not independent: every
record changes what the later ones mean.

## Decision

**Unknown and unimplemented record kinds are refused, and the refusal says so
in those words.** There is no forward-compatible ignore rule anywhere in the
certificate reader or the closure, and there will not be one.

**The refusal is reported as a claim this implementation does not close**, not
as a defect in the certificate. The two are different findings and a reader
needs to be able to tell them apart: one means the solver produced something
wrong, the other means the verifier has not caught up. The message names the
record type it stopped at.

**Growth is one-directional.** A later change may make the verifier accept a
certificate it previously refused, by implementing the rule that certificate
needed. No change may make it accept a certificate it previously refused by
loosening a check — that door is the one the constitution nails shut, and the
distinction between the two is exactly the distinction between implementing a
rule and deleting one.

## Consequences

Today the verifier closes the linear-program forms: an optimal claim, an
infeasibility, an original-space ray, a certified gap, and a point with no
bound. A certificate carrying a transform, a cut, a branch, or a leaf is
refused. Every one of those refusals is expected to become an acceptance as the
corresponding rule lands, and each landing brings the vectors that show the rule
fires.

The cost is that the verifier's coverage has to be stated rather than assumed,
which is why it is stated: `docs/LIMITATIONS.md` names what the verifier checks
and what it does not, and that file is part of the release rather than a note.

An honest consequence worth naming: a solver that emitted a tree certificate
today would find its own work refused. That is the correct order. The proof
gate precedes the feature it guards, so the feature arrives into a checker that
was ready for it rather than into one that will be extended to accommodate it.

## Alternatives considered

**Accept unknown records and warn.** Rejected on the argument above: the records
are not independent, so an unchecked record is not a missing check but a wrong
one.

**Refuse only the kinds that change meaning, and skip the inert ones.** Rejected
because the set of inert kinds is empty. Even a leaf record that the closure
would not consult changes the tree's shape, and a cut nobody cites still asserts
that a derivation was performed.

**Leave the record kinds out of the format until their rules exist.** Rejected
at the freeze and reaffirmed here: a format that grows a record kind per feature
is a format that is renegotiated per feature, and the whole reason to freeze the
proof calculus before writing the solver is that a calculus designed alongside
its subject accommodates it.

## Supersedes / superseded by

Refines the proof-boundary decisions in
[ADR-0001](0001-foundation-decisions.md) and the framing in
[ADR-0008](0008-certificate-framing.md). No decision is superseded.
