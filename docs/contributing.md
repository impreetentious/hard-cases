# Contributing

Hard Cases is a correctness laboratory that happens to optimize. Keep changes
small, deterministic, and readable enough for someone learning how a solver
works to follow.

## Before opening a change

- Run `cargo fmt --all -- --check`.
- Run `cargo clippy --workspace --all-targets -- -D warnings`.
- Run `./scripts/ci/forbidden-grep.sh` and the verifier isolation check.
- Run `./scripts/preflight.sh`. It is the local mirror of the `check` workflow
  and runs the same steps in the same order.
- The suite runs under release inside the preflight, because the campaigns are
  sized by the normative constants and running them unoptimized costs minutes
  per change without covering anything more. The unit tests are run unoptimized
  as well, which is where an arithmetic overflow panics rather than wrapping,
  and continuous integration runs everything both ways.
- Add a named regression test for every known failure mode the change touches.
- Update the public rustdoc for anything you changed.
- The last two preflight steps need a browser and a Node toolchain: the page is
  built from what the Rust build just produced, held to byte budgets, and
  driven against the built bundle. Both say which command installs what they
  are missing. Neither skips.

## The generated documents

Three things in this repository are written by a program and diffed by a test,
and editing any of them by hand fails the suite:

- `docs/formats/` is generated from the inert assets in `spec/v1`. Neither
  implementation gets to be its own specification.
- `docs/scoreboard.md`, `docs/proof-tree.svg`, and the counter block in
  `README.md` come from `cargo run -p hc-bench --bin scoreboard`. The board is
  a real run of the committed corpora, the figure is a real solve with the
  verifier's answer in its caption, and the counters are measured at generation
  time. A change to the search that moves an iteration count is a change that
  updates all three in the same commit.
- Every relative link in every document — and every path inside a shell
  example that looks like a path into this repository — is checked to exist.
  The front page once named six essays, a demo script, a recorded animation and
  two instance files, none of which did.

## The two arithmetics

The solver searches in floating point. Nothing it computes in floating point is
ever a conclusion. A bound exists only as a valid exact inference; an incumbent
exists only after exact screening and an exact check against the original
model. A status value meaning "probably fine" does not exist and must never be
added — when every numerical escape is exhausted, the honest answer is that no
progress was made.

There are no naked tolerances. Numeric literals of the form `1e-*` live only in
the tolerance module, and every one has a name that reads as policy at the call
site. Solver crates use ordered containers only, take no ambient time,
randomness, environment, or filesystem access, and never use fused
multiply-add: effort budgets are counts of iterations and nodes, never clocks.

## The verifier boundary

The verifier is the point of the project. It has no workspace dependencies and
no external dependencies at all — its own big integers, its own hashing, its
own parsers, its own proof rules. It shares only inert specification assets and
hand-worked vectors with the solver, never an executable helper, and callers
may use its API but never the reverse. A shared convenience function that
quietly joins the two parsers destroys the entire claim, so the dependency,
import, and source scans that enforce this are not optional.

If the verifier rejects a certificate the solver produced, the default
assumption is that the solver or the certificate is wrong. Changing a verifier
rule requires a hand-worked derivation and a new decision record; making the
verifier accept something it previously rejected, without that, is forbidden.

## Decisions and formats

Numbered decision records in `docs/adr/` are append-only. If a change crosses a
crate boundary, touches a file or certificate format, or affects what a claimed
status means, write the record before the implementation. Formats carry
versions; changing one bumps the version and regenerates its fixtures.

## The situation room

Everything the page draws comes from a solve the module performed or from a
recorded trace and certificate it replayed, and the header says which at all
times. A panel with nothing behind it stays empty and says so; a panel computed
live under a replay says that too. Adding a panel means answering where its
contents came from before writing it.

The three visual states are float, exact-and-unchecked, and verified, and they
are never drawn the same way. Collapsing the middle one understates what exact
assembly did in one direction and makes the project's central claim in the
other.

Commits, tags, and releases are the repository owner's to make.
