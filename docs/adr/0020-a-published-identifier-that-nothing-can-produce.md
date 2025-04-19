# ADR-0020: A published identifier that nothing can produce is retired, not left

## Context

The frozen table in `spec/v1/rejections.toml` declares 122 rejection
identifiers. Each names one check and the message the verifier prints when that
check refuses. They are a public interface: `hc verify` prints them, the tamper
corpus pairs every mutation with the one that must answer it, the situation
room shows them, and the case files cite them.

Eight of the 122 could be produced by no implementation in this workspace.

Each was traced before anything was changed, and none of them was a hole in the
verifier. Every property they name is enforced, by an identifier that fires
earlier or by an encoding that makes the failure unrepresentable:

- `HCC-CUT-13`, a cut family outside the closed list. The family *is* the
  record type code, so an unknown family is an unknown record and is refused
  before any cut rule runs.
- `HCC-PARSE-18`, a declared tree depth against the cap. The header declares no
  depth. The cap is enforced during the replay, under `HCC-TREE-13`, with tests
  at the cap and one past it.
- `HCC-SCOPE-03`, a cut cited outside its subtree. Visibility at a node is one
  predicate covering the model's inequalities, the assumptions on the path to
  the root, and the cuts whose scope is on that path. A cut cited outside its
  subtree is a cut that is not visible, refused by `HCC-SCOPE-01`.
- `HCC-TRANSFORM-04` and `-05`, the forward and reverse maps on witness points.
  The forward map is checked and the failure it produces is a disagreement
  between the two witnesses the certificate carries, `HCC-POINT-05`. The
  reverse map is what the solver does to lift a point; the verifier checks the
  correspondence rather than reproducing the lift.
- `HCC-TRANSFORM-08`, the rounding direction on a derived integer bound. A
  transform's result is recomputed and hashed rather than read, so a bound
  rounded the wrong way produces a different hash and is refused by
  `HCC-TRANSFORM-03`.
- `HCC-TRANSFORM-09`, the chain's end against the root's stated model. The same
  check as `HCC-TREE-14`, under a second identifier. The tamper corpus pins it
  at the tree one.
- `HCC-TREE-11`, the finiteness of a frontier bound. A bound is a rational and
  this format has no spelling for an infinite one.

One frozen vector, `[chain.root_hash_mismatch]`, named `HCC-TRANSFORM-09` as
the rejection it expects. It carried no trap and nothing ran it, so it was a
vector waiting for a rejection nothing could produce.

## Decision

**An identifier that no implementation can produce is marked `retired` in the
asset, names the identifier that answers instead, and says why in a sentence.**
It is never removed and never reused: it was published once, and a reader who
saw it has to be able to find out what happened to it.

**A retired identifier is registered by neither implementation.** The constant
goes. A constant nobody names is an identifier nobody can be answered with, and
leaving one in the registry invites its use.

**Three gates hold this.** The rejection scan requires every live identifier to
be registered by some implementation *and* named somewhere outside its
registry, and every retired one to be registered by nobody. The generated-doc
check requires a retired entry to carry `answered_by` and `retired_note`, and
requires `answered_by` to name an identifier that is declared and not itself
retired. And no other asset may cite a retired identifier, which is what would
have caught the vector.

## Consequences

The published table now distinguishes the rules the verifier has from the rules
somebody once meant it to have. `docs/formats/rejections.md` strikes the
retired rows through and lists them again underneath with what answers instead,
so a reader who arrives holding an old identifier lands somewhere useful.

Nothing about what the verifier accepts or refuses changed. The eight
properties were enforced before this record and are enforced after it; what
changed is that the table no longer claims eight rules that were not there.

Adding a rule now costs a constant *and* a use site before the build is green,
which is the right order: a rule with no call site is not a rule.

## Alternatives considered

**Implement the eight checks.** Each would be a second check for a condition an
earlier one already refuses, and the table's own policy is that an identifier
names the *earliest* point at which a defect is detectable. Two identifiers for
one defect would make the corpus's pins ambiguous, which is the property that
makes the corpus worth more than "these are all rejected".

**Delete them from the asset.** They were published. A reader holding
`HCC-TRANSFORM-09` from an older document would find nothing, and the number
could later be reused for an unrelated rule, which is worse than a dangling
reference.

**Leave them and note it in the limitations.** The table is the interface;
LIMITATIONS is a document about the interface. A promise is not repaired by
disclosing it somewhere else.

## Supersedes / superseded by

Neither. It records a defect in the published table and the gates that keep it
from returning.
