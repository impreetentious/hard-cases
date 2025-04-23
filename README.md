# Hard Cases

Hard Cases is a from-scratch linear and mixed-integer optimizer. The solver is
the excuse; the certificate it emits with every claim, and the independent
verifier that reads the original bytes and rechecks the proof, are the point.

Optimization solvers are trusted the way oracles are trusted. An industrial
MILP solver prints `optimal` and nobody can check it — the engine is closed, the
arithmetic is floating, and the search tree is gone. This solver is built around
one refusal: **it never claims optimality it cannot prove.**

The search runs in floating point, and nothing it computes there is ever a
conclusion. A bound exists only as a valid exact dual inference; an incumbent
exists only after exact screening against the original model. Every terminal
claim ships as an `HCC` certificate — exact dual and contradiction inferences,
scoped cut derivations, checked presolve transforms, and a complete recursive
cover of the branch tree — which `hc-verify` then checks by reparsing the
*original problem file* with its own parser, its own big integers, and its own
proof rules. It shares no executable code with the solver, has no dependencies
at all, and its trust boundary is published rather than described as magic. What
that boundary does and does not include is spelled out in
[limitations](docs/LIMITATIONS.md).

![The certified proof tree of a small fixed-charge model: five nodes, two of
them split on a binary column, and each of the three leaves closed by a
contradiction, by a bound, or by an integral point attaining its own bound —
with the certificate's size and the verifier's work underneath.](docs/proof-tree.svg)

That picture is generated, not drawn: `cargo run -p hc-bench --bin scoreboard`
solves the instance, assembles the certificate, has the standalone verifier
recheck it, and renders what happened. A test regenerates it and diffs, so a
change to the search that changes the tree redraws the figure in the same
commit.

The [situation room](theater/README.md) runs the same engine in the browser,
compiled to WebAssembly with no server behind it: watch a real solve build its
tree, click any node to see why it was closed, pause the search after one node
and certify the gap that is left, then open the tamper lab, edit a byte of the
proof, repair its checksum, and watch the verifier name the rule that refuses
it.

Everything the page draws comes from a solve it just performed or from a run
recorded by the command line and replayed, and it says which at all times.
Nothing on it is a mock.

<!-- counters:begin -->
| measured | |
|---|---:|
| verifier source, excluding its own tests | 8426 lines in 23 files |
| dependencies the verifier declares | 0 |
| uses of unsafe inside the verifier | 0 |
| committed instances a gate consumes | 12 |
| linear ones answered identically by two unrelated solvers | 8 of 8 |
| mixed-integer ones whose certificate the standalone verifier accepted | 4 of 4 |
| named mutations in the tamper corpus | 31 |
| rejection identifiers an implementation can answer with | 114 |
| of those, retired and answered by an earlier rule | 8 |
| separation rounds every surface runs | 4 |
<!-- counters:end -->

The table above is written by `cargo run -p hc-bench --bin scoreboard`, which a
test then regenerates and diffs. A number on a front page is a claim like any
other, and these are the ones most likely to quietly stop being true.

## Try the current workspace

```sh
cargo run -p hc-cli -- solve instances/integer/knap9.mps --format mps-free --cert out.hcc
cargo run -p hc-verify --bin hc-verify -- out.hcc --instance instances/integer/knap9.mps
cargo run -p hc-cli -- explain instances/integer/parity5.mps --format mps-free --tree --proof
cargo run -p hc-cli -- receipt
./scripts/preflight.sh
```

The second command is the point of the project. It links none of the solver,
reparses the model from disk, and ends in either `CERTIFIED OPTIMAL (exact
arithmetic)` or a rejection naming the node and the rule — for example
`REJECTED: node 47: claimed bound does not equal its nonnegative inequality
combination (HCC-DUAL-03)`.

The repository contains:

- `hc`, a command line that solves, verifies, explains, benchmarks, and
  generates instances, reading MPS (fixed and free), an LP-format subset, and a
  small JSON schema;
- `hc-verify`, a standalone zero-dependency verifier binary and crate with its
  own big integers, rationals, hashing, parsers, and rule engine, so
  verification never requires linking the solver;
- from-scratch exact arithmetic on both sides — two unrelated implementations,
  cross-checked against the same inert hand-worked vectors, with rationals built
  from the original file's decimal text rather than a float round-trip;
- a revised simplex over bounded variables: Markowitz LU with product-form
  updates, devex pricing, a Harris ratio test, and a named escalation ladder
  through refactorization and perturbation to Bland's rule — at the end of which
  the honest status is that no progress was made;
- branch and bound with reliability pseudocost branching, plunging, exact
  incumbent screening, and a recursive proof in which every integral leaf
  carries a dual bound;
- a closed set of presolve transforms whose forward and reverse maps the
  verifier replays as exact equivalences, and Gomory mixed-integer and
  exact-lifted cover cuts whose scope is enforced in the solver and again by the
  verifier;
- the `HCC` certificate format covering exact optimality, infeasibility,
  integer-compatible unboundedness, certified gaps, and — when no bound can be
  certified — a plain feasible point the verifier checks against the original
  model;
- the tamper corpus: thirty-one named mutations of genuine certificates, each
  paired with the rule that must catch it, shipped as a product surface rather
  than a test fixture — the situation room offers every one of them to a
  visitor;
- committed instance corpora that gates actually consume: eight linear models
  run by two independently written solvers, and four mixed-integer ones whose
  optima are also found by enumerating every point in their integer boxes;
- the situation room, and the case files of real defects found by the
  differential, fuzz and certification campaigns, each with the seed or the
  command that reproduces it.

**No speed races, ever.** This project publishes no wall-clock comparison
against another solver. Correctness cross-checks against published optimal
*values* are required; performance is compared only against earlier versions of
this solver, and work is budgeted in deterministic effort units — simplex
iterations and node counts — never in seconds. Supported targets are Linux
x86_64, macOS arm64, and WebAssembly; Windows is not a supported CI target. A
false terminal claim is a severity-zero defect. See
[limitations](docs/LIMITATIONS.md) before interpreting any result.

## Going deeper

The write-ups are the point of the project — each one names a failure mode and
shows the machinery that catches it, out of the code and the incidents in this
repository rather than out of a general argument.

The verifier's own trust boundary is not described in a document that could go
stale: `hc receipt` measures it — how much code did the checking, what it
depends on, the hash of that source, and what one real verification cost in
units that are not seconds.

- [`docs/LIMITATIONS.md`](docs/LIMITATIONS.md) — what this does and does not
  prove; read it before interpreting any result
- [`docs/writeups/`](docs/writeups/) — the six essays:
  [the solver that shows its work](docs/writeups/01-shows-its-work.md),
  [big rationals from nothing](docs/writeups/02-big-rationals.md),
  [degeneracy, the simplex's haunted house](docs/writeups/03-degeneracy.md),
  [a proof you can tamper with](docs/writeups/04-tamper.md),
  [the cut that lies](docs/writeups/05-cut-validity.md), and
  [honest scoreboards](docs/writeups/06-honest-scoreboards.md)
- [`docs/scoreboard.md`](docs/scoreboard.md) — what this solver does on the
  committed corpora, generated and diffed on every run of the suite, with the
  fetched collections named rather than covered up
- [`docs/formats/`](docs/formats/) — the certificate grammar, the accepted
  MPS and LP subsets, and the JSON schema, generated from the inert assets
- [`docs/case-files/`](docs/case-files/) — real bugs, minimized, with the
  instance or the seed that exposed them, and a test that replays each one
- [`docs/adr/`](docs/adr/) — the numbered decisions, append-only
- [`spec/`](spec/) — the inert proof calculus: canonicalization, rules,
  rejection identifiers, and hand-worked vectors. Code is never the source of
  truth
- [`tamper/README.md`](tamper/README.md) — the corrupted-certificate corpus
- [`theater/README.md`](theater/README.md) — the situation room

## Contributing

See [the contribution guide](docs/contributing.md), the numbered decisions in
[`docs/adr/`](docs/adr/), and the [pull-request
checklist](.github/PULL_REQUEST_TEMPLATE.md). Every change needs tests and a
clean local preflight. The verifier acquires no dependency and no shared
code, ever; changing a proof rule requires a hand-worked derivation and a
versioned decision record.

## License

Apache-2.0 © 2024-2025 Sidakpreet Singh — see [LICENSE](LICENSE). Solvers are
infrastructure, and embedding is the point.

---

**Version:** v0.13.4
