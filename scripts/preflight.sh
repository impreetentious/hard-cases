#!/usr/bin/env bash
#
# The local mirror of CI. Run this before opening a change; it is the `check`
# workflow's always-on steps, in the same order, so a red pipeline is something
# you already saw on your own machine.
#
# It is not the whole workflow, and the difference is named here rather than
# discovered. Continuous integration also runs the workspace tests unoptimized,
# the certification, tiny integer, and dual simplex campaigns at their nightly
# sizes, the fuzzer at its nightly budget, and `cargo doc`. Those are sized for
# a machine with nothing else to do, and a mirror nobody can bear to look in is
# not a mirror. `docs/contributing.md` carries the same list.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "[1/12] version coherence"
node "$repo_root/scripts/check-version-coherence.mjs"
# Every relative link in every document points at something that exists. It is
# the front page that this catches: an intention typed as a promise is a false
# statement by the time somebody reads it.
echo "[2/12] documentation links"
node "$repo_root/scripts/ci/links.mjs"
# The contracts the assets name are real ones: every published rejection
# identifier is one some implementation can produce, every retired one is
# registered by nobody, and every trap an asset names is a test of that name.
# A promise in an asset that nothing keeps is worse than no promise.
echo "[3/12] specification contracts"
node "$repo_root/scripts/ci/contracts.mjs"
echo "[4/12] normative assets and generated docs"
node "$repo_root/scripts/gen-docs.mjs" --check
echo "[5/12] cargo fmt"
cargo fmt --all -- --check
echo "[6/12] cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings
echo "[7/12] forbidden API scan and verifier isolation"
"$repo_root/scripts/ci/forbidden-grep.sh"
# The campaigns are the expensive part and they are sized by the normative
# constants, not by what is comfortable. Running them unoptimized costs minutes
# per change and buys nothing the optimized run does not, so the suite runs
# under release here and the debug run — which is where an arithmetic overflow
# turns into a panic rather than a wrap — is kept for the unit tests, where the
# arithmetic actually lives. Continuous integration still runs everything both
# ways; this is the local mirror, and a mirror nobody can bear to look in is
# not a mirror.
echo "[8/12] tests"
cargo test --workspace --all-targets --release
cargo test --workspace --lib
echo "[9/12] standalone verifier and wrapper parity"
"$repo_root/scripts/ci/parity.sh"
# Every artifact twice, in two separate processes, compared byte for byte. Two
# processes rather than two calls: an in-process repeat shares the allocator
# state and the address-space layout that a determinism gate exists to catch.
echo "[10/12] determinism double run"
"$repo_root/scripts/ci/determinism.sh"
# The browser surface, built for the target it ships to and driven in a real
# WebAssembly host. The tests beside the exports run natively and cannot catch
# a pointer that is a different width there.
echo "[11/12] browser surface"
"$repo_root/scripts/ci/wasm.sh"
# The page itself: built from what the Rust build just produced, type-checked,
# held to its byte budgets, and driven in a real browser. It is last because it
# is the only step that needs a browser on the machine, and a failure here is
# about the page rather than about the solver.
echo "[12/12] situation room"
cargo build --release -p hc-cli
"$repo_root/scripts/ci/theater.sh"
echo "preflight: PASS"
