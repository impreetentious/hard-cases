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

The situation room mid-solve: the tree growing, bound and incumbent strips converging, then a tampered certificate field rejected with the exact failing inequality highlighted.

**[Open the situation room →](https://itsmonarch04.github.io/hard-cases/)** The
same engine runs in your browser: watch a real solve build its tree, click any
node to see why it was pruned, pause the search and certify the current
frontier, then open the tamper lab, edit a proof, and watch the verifier name
the rule that lies. No toolchain required.

## Try the current workspace

```sh
cargo run -p hc-cli -- solve instances/demo/sudoku16.mps --cert out.hcc
cargo run -p hc-verify --bin hc-verify -- out.hcc --instance instances/demo/sudoku16.mps
cargo run -p hc-cli -- explain instances/demo/knapsack.lp --tree --proof
./scripts/preflight.sh
./scripts/demo.sh
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
- the tamper corpus: deliberately corrupted certificates with expected rejection
  identifiers, shipped as a product surface rather than a test fixture;
- the situation room, the case files of real bugs found by differential and
  fuzz campaigns, and permanent scoreboards on netlib and a fixed MIPLIB subset.

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
shows the machinery that catches it.

- [`docs/LIMITATIONS.md`](docs/LIMITATIONS.md) — what this does and does not
  prove; read it before interpreting any result
- [`docs/LIMITATIONS.md`](docs/LIMITATIONS.md) — what this kernel will not claim
- [`docs/adr/0010-a-verifier-refuses-what-it-cannot-check.md`](docs/adr/0010-a-verifier-refuses-what-it-cannot-check.md) — the verifier's trust boundary
  source hash, and the words "independent" is and is not doing here
- [`docs/formats/`](docs/formats/) — the certificate grammar, the accepted
  MPS and LP subsets, and the JSON schema, generated from the inert assets
- [`docs/case-files/`](docs/case-files/) — real bugs, minimized, with the
  instance and certificate that exposed them
- [`docs/adr/`](docs/adr/) — the numbered decisions, append-only
- [`spec/`](spec/) — the inert proof calculus: canonicalization, rules,
  rejection identifiers, and hand-worked vectors. Code is never the source of
  truth
- [`tamper/README.md`](tamper/README.md) — the corrupted-certificate corpus

## Contributing

See [the contribution guide](docs/contributing.md), the numbered decisions in
[`docs/adr/`](docs/adr/), and the pull-request checklist. Every change needs
tests and a clean local preflight. The verifier acquires no dependency and no
shared code, ever; changing a proof rule requires a hand-worked derivation and a
versioned decision record.

## License

Copyright (c) 2024-2025 Sidakpreet Singh.

Hard Cases is released under the MIT License — solvers are infrastructure, and
embedding is the point. The complete license text is in [LICENSE](LICENSE); the
SPDX identifier is `MIT`.

---

**Version:** v0.11.6
