# ADR-0011: Verifier work is an upper bound charged at a metered façade

## Context

Verification has to be budgeted, and it cannot be budgeted in seconds. A
wall-clock limit measures the machine rather than the proof, and it makes a
release receipt unreproducible. The published unit is therefore a count of what
the verifier actually does: one unit per byte parsed, and one per limb-word
step of arithmetic.

The freeze stated that the counter would be incremented centrally inside the
arithmetic and parser primitives, so that no call site could forget to pay. That
sentence was written before the arithmetic existed, and meeting it literally in
Rust turns out to require one of two things, both bad:

- **A process-global mutable counter.** Deterministic under a single thread, and
  ambient state in the one crate whose whole argument is that it has none. It
  would also make two verifications running in one process interfere, and the
  release harness runs many.
- **A meter parameter threaded through every arithmetic signature.** `Nat`,
  `Int`, and `Frac` are the base of the trusted computing base, and their
  plainness is a property a reader is supposed to be able to check by reading
  them. Adding an out-parameter to every operation in them makes the code that
  most needs to be readable the least readable, in exchange for a count.

There is a third problem underneath both. Some real step counts are
data-dependent — Euclid's algorithm takes a number of divisions that depends on
its inputs — so an *exact* count is only obtainable by instrumenting the loops,
which is the option just rejected.

## Decision

**Work is charged at a metered façade, and the count is an upper bound.**

`vwork::Work` performs every arithmetic operation the proof rules need and
charges each one a cost computed from the operands' limb widths, using the step
counts the underlying primitives would perform at their widest. The rules take
a meter or they do nothing: a rule with no meter has no arithmetic, which is the
"cannot forget to pay" property the freeze was after, obtained structurally
instead of by convention.

Bytes are charged one for one, for the certificate and the instance both, at the
entry point before either is interpreted.

**The bound is generous on purpose, and the report says it is a bound.** The
reduction cost charged after each rational operation is the square of the widest
possible result rather than the actual number of Euclid steps, because the
former is computable from sizes and the latter is not.

## Consequences

The published figure is a ceiling on the real cost. That is the safe direction
and it is worth being explicit about why: the count gates nothing about
mathematics. A cap that fires early can only refuse to complete a verification;
it can never cause an unsound proof to be accepted. An *under*-count could let a
verification exceed a published budget while reporting that it had not, which is
the failure a work meter must not have — and it is the failure an exact-looking
count computed by a mistaken instrumentation would produce silently.

Two properties survive and both are load-bearing. The count is deterministic:
the same certificate and the same instance produce the same number on every
machine, and a test asserts it. And the count is monotone in the size of the
values, so a proof that grows cannot get cheaper.

The budget asset's description of the counter is corrected to describe what is
implemented. That edit touches a budget definition and not the proof calculus,
the certificate encoding, or the canonicalization, so no format version moves
with it — the numbers a certificate contains and the rules that check them are
untouched.

Solver effort and verifier work remain different currencies and are never
combined. Wall time and peak memory stay disclosed operational measurements that
gate nothing.

## Alternatives considered

**A global counter.** Rejected above: ambient state in the crate that must have
none, and it makes concurrent verifications interfere.

**A meter parameter inside the arithmetic.** Rejected above: it damages the
readability of the trusted base, which is one of the few defences the trusted
base actually has.

**Drop the counting and publish nothing.** Rejected. A verifier with no reported
cost invites the assumption that verification is free, and the honest answer to
"how much does checking this proof cost" is a number rather than a shrug.

**Count operations rather than limb-words.** Rejected: it would report the same
figure for a proof over small integers and a proof over million-bit rationals,
which is precisely the difference the cap exists to notice.

## Supersedes / superseded by

Refines the work-unit definition seeded by
[ADR-0001](0001-foundation-decisions.md). No decision is superseded.
