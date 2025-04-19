# A proof you can tamper with

The claim this project makes is that an independent program checks the solver's
answers. There is exactly one honest way to demonstrate that claim, and it is
not a passing test suite: it is to break a proof in front of somebody and watch
the checker refuse it by name.

That is what the tamper corpus is, and what the tamper lab in the browser page
is. This essay is about what has to be true of the format and of the verifier
before either of them means anything.

## The verifier's position

`hc-verify` is handed two things: a certificate and the original instance file.
It is handed nothing else — not the solver's model, not its basis, not a dump,
not a hint.

It parses the instance itself, with its own reader. It canonicalizes and hashes
what it parsed, and checks that hash against the one the certificate states.
Everything after that is about the model that hash names, and if the
certificate was detached from its file and reattached to another one, nothing
past the header runs.

Then it reads the records. A field whose name in the reader is `stated` is one
the rules must recompute rather than adopt; the name is a reminder at every use
site. A dual bound states multipliers, and the verifier forms the combination
itself and compares. A cut states coefficients, and the verifier rederives them
from the combination the record names. A transform states the hash of the model
it produces, and the verifier applies the rule itself and hashes the result.

## Why the tree is recursive and not a bag of leaves

The tempting encoding for a branch-and-bound proof is a list of leaves, each
with a bound. It is compact and it is unverifiable: nothing in that list says
the leaves *cover* the feasible region, and a proof that has stopped covering
the region is a proof with a hole in it, which is exactly where a wrong answer
lives.

So the certificate carries the tree. Every node is either split into two
children — at a declared integer variable, at an exact integer, so that
`x ≤ k` and `x ≥ k+1` leave nothing between them — or closed by something
citable: a dual bound, a contradiction, or an integral point attaining its own
bound. The verifier walks that structure and requires every node to have one
parent, to be reachable from the root, and to be branched or terminated exactly
once.

The walk is iterative with an explicit depth cap rather than recursive, so a
legal proof deeper than the process call stack is *checked* rather than crashed
on. A synthetic tree a hundred thousand nodes deep is answered by a rule.

And a leaf must cite an inference made at *its own node*. An inference is sound
and it is about somewhere else; a bound proved in one region says nothing about
another. That is the shape a copied-and-pasted proof takes, and it is one of
the thirty-one entries.

## The corpus

Thirty-one named mutations of genuine certificates, each paired with the
rejection identifier that must answer it — from a repository of 114 live
rejection identifiers, each one a documented rule rather than an error string,
and a gate that refuses the build if one of them stops being reachable.

The pairing is the entire point. A corpus that asserted only "these must all be
rejected" would pass while every one of them was caught by the checksum, which
would establish nothing whatsoever about the proof calculus, and would go on
passing the day a proof rule was deleted.

So **most entries are re-sealed**: the trailing checksum is correct for the
bytes they ship. Anyone holding the format specification can repair a checksum,
so the interesting forgery is the one that is well-formed and still wrong, and
the mathematics has to be what refuses it. The few entries that are not
re-sealed exist to pin the reader's own rules, and they say so.

A sample of what they cover:

- **A negated multiplier.** The single mutation that turns the dual bound rule
  from sound into arbitrary, since a negative multiplier reverses the
  inequality it is applied to. It is checked before the sum is formed, because
  after the sum it is indistinguishable from arithmetic that merely came out
  wrong.
- **A dropped bound multiplier.** The classic omission: a Farkas witness over
  rows only, with variable bounds assumed available by convention. Here a bound
  is a canonical inequality with an identifier exactly as a row side is, there
  is no convention to lean on, and the coefficients fail to cancel.
- **A misstated objective.** The point is genuinely feasible and the number
  attached to it is wrong. The verifier recomputes the objective from the point
  and the model rather than reading it.
- **A fractional split.** Add one half to a branch threshold, and the two
  children stop covering their parent — every integer point in the gap escapes
  the proof.
- **A deleted leaf.** A node the certificate stops accounting for. It applies
  only to a tree that branched, and the manifest says why: removing the single
  leaf of a one-node tree leaves a root, a bound and an integral point
  attaining it, which is a sound linear proof of the same statement. Accepting
  *that* is correct rather than a hole.

## The identifier is the earliest rule, on the models it was written for

An entry names the earliest rule that can see its defect. Earliest matters: an
entry caught late is an entry that got past a check that should have stopped
it, so the corpus pins the *order* of the checks as well as their existence.

But earliest is a property of the model as well as of the mutation. Moving an
incumbent a thousand units is caught by the row check on a model with a bound
in the way and by the objective check on one without. Running the corpus in the
browser, against whatever certificate is on the page, two entries came back
answered by an earlier rule than the corpus pins — correctly.

So the pin is held to on the harness's own models, where it says something
about the order of the checks. What holds on every model, and what the browser
gate asserts, is that a forgery is refused at all. The page shows both: the rule
that answered, and the rule the corpus pins it to, with a sentence when they
differ.

## The lab

The corpus is a product surface rather than a test fixture. In the situation
room you pick an entry and watch it get refused; and then you can do your own.

The field editor lets you change any byte of any record and repair the checksum
— and it repairs it *in the page*, with its own implementation of the
polynomial, rather than asking the module to. That is deliberate. A lab that
asked the solver to re-seal a forgery would be demonstrating that the solver
cooperates with forgery. A lab that seals its own lies is demonstrating that
the verifier survives them.

What comes back is the rule that stopped it and the exact arithmetic behind the
refusal — `HCC-SCOPE-01: inequality 17 is not visible here`, on a certificate
whose checksum is perfectly correct.

## The two the verifier caught for real

The corpus is forgeries. The interesting refusals were not.

[HC-CASE-0005](../case-files/HC-CASE-0005.md): the verifier refused a
certificate the solver had just produced. A column held three, above its own
upper bound of one. The model had crossed bounds on a column no row mentioned,
and the empty-column rule chose the nearer endpoint without asking whether the
bounds crossed at all. Both sides decline now — the verifier has its own copy
of the transform rules, and a rule it accepts that the solver refuses is a rule
a forged certificate could still use.

The lesson filed with it is the one to keep. The verifier caught it three
stages downstream, in a check about points, on a model that had been reduced,
solved and lifted since. Had the lifted value landed inside its bounds by
coincidence, the same reduction would have passed.

**The independent checker is a net and not a design.**
