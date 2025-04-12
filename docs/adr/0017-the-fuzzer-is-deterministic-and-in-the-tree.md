# ADR-0017: The fuzzer is deterministic and lives in the tree

## Context

The specification asks for fuzz targets over the parsers, the certificate
format, and the search: no panics, no hangs, caps checked before allocation,
and no disagreement between the two independent readers. It also asks that
findings become case files with a regression instance beside them.

The default way to do this in Rust is `cargo-fuzz`: libFuzzer behind a nightly
toolchain, with a coverage-guided corpus that grows on disk as the engine
discovers new paths. It is a good tool and it is the wrong one here, for three
reasons that are specific to this project rather than general complaints.

**A finding has to be reproducible from a number.** Everything else here is:
the campaigns are seeded from the instance digest, the search is single
threaded, the determinism gate compares bytes across two runs. A corpus that
grows out of coverage feedback and timing produces inputs nobody can regenerate
— they can only be kept. A case file whose reproduction step is "restore this
binary blob" is a story about a bug rather than a regression test for it.

**The interesting inputs are not random bytes.** They are files that are almost
right: a statement continued one line too far, a bound whose sides cross, a
token one character over its cap, a section keyword sharing its line. Reaching
those by flipping bits in a valid file takes an enormous budget, and a
structure-aware generator reaches them immediately because it knows the grammar
it is bending.

**It would be the largest thing in the build.** The workspace has no
dependencies at all, and the verifier has none by construction. Adding a
nightly compiler, a linker plugin, and a sanitizer runtime to run the tests
would make the fuzzing harness heavier than the solver it fuzzes.

## Decision

**`hc-fuzz` is an ordinary workspace crate. Its corpus is generated from seeds
by the project's own frozen random-number generator, and every case is a
function of its seed and nothing else.**

The generators emit a small abstract model and render it in all four accepted
readings — fixed and free MPS, LP format, and the JSON schema — with the
spelling choices each grammar leaves open drawn from the same seed. The
mutations then walk each rendering just outside its grammar: a character
replaced by one the grammars use, a line dropped or repeated or exchanged, a
file truncated, a token or a name widened past its cap.

Four properties are checked.

1. **No panic.** Every reader call runs under a caught unwind, because a panic
   in a parser is a way to stop the tool with a file rather than a defect
   somewhere harmless.
2. **The two implementations agree.** Where both accept, the canonical dumps
   must be identical bytes; where both refuse, the identifiers must match; and
   one accepting what the other refuses is the finding the whole arrangement
   exists to produce.
3. **One draft, four files, one model.** A draft rendered in every reading must
   produce the same canonical model below the lines that identify the file.
   No single-format harness can see this.
4. **A certificate answers.** Real certificates, assembled from generated
   instances, are damaged and re-verified: the verifier must answer with a
   verdict or an identifier, the same way twice, and must never verify a
   certificate against a file it is not about.

Budgets are counted in cases, never in seconds. The per-change budget runs in
the ordinary test suite; the nightly budget is the same targets over more
seeds, so a nightly finding reproduces locally by running the seed it names.

## Consequences

**The fuzzer paid for itself immediately.** Its first three runs found a sign
error in its own MPS renderer, and then two genuine disagreements between the
two independent readers: one about whether a malformed numeric token is
reported before a malformed statement shape, and one about whether a
fixed-form record whose content leaves its windows is refused before the
question of which section it is in. Both were resolved by adopting one rule on
both sides — **a line is lexed, and then it is interpreted** — which is now
written where each reader implements it.

**There is no coverage feedback.** The corpus reaches what its generators were
written to reach. A region of a reader nobody thought to emit for is a region
nobody is fuzzing, and no counter will report that. This is a real limitation
and it is recorded in `docs/LIMITATIONS.md` rather than left implied.

**Findings are cheap to file.** A case file names a seed, and the seed
regenerates the bytes on any machine at any budget.

## Alternatives considered

**`cargo-fuzz` alongside this, for the coverage it would add.** Rejected for
v1 rather than forever: it would need a second toolchain in the pinned
environment and a second corpus format for case files, and the honest accounting
is that nobody would maintain both.

**Property-based testing with a shrinker instead.** The workspace already
declines `proptest` outside the verifier. A shrinker is genuinely useful, and
what it would shrink here — a generated instance — is already small, because
the generators draw at most four columns and four rows.

## Supersedes / superseded by

Nothing.
