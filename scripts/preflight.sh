#!/usr/bin/env bash
#
# The local mirror of CI. Run this before opening a change; it is the same set
# of checks the `check` workflow runs, in the same order, so a red pipeline is
# something you already saw on your own machine.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "[1/9] version coherence"
node "$repo_root/scripts/check-version-coherence.mjs"
echo "[2/9] normative assets and generated docs"
node "$repo_root/scripts/gen-docs.mjs" --check
echo "[3/9] cargo fmt"
cargo fmt --all -- --check
echo "[4/9] cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings
echo "[5/9] forbidden API scan and verifier isolation"
"$repo_root/scripts/ci/forbidden-grep.sh"
# The campaigns are the expensive part and they are sized by the normative
# constants, not by what is comfortable. Running them unoptimized costs minutes
# per change and buys nothing the optimized run does not, so the suite runs
# under release here and the debug run — which is where an arithmetic overflow
# turns into a panic rather than a wrap — is kept for the unit tests, where the
# arithmetic actually lives. Continuous integration still runs everything both
# ways; this is the local mirror, and a mirror nobody can bear to look in is
# not a mirror.
echo "[6/9] tests"
cargo test --workspace --all-targets --release
cargo test --workspace --lib
echo "[7/9] standalone verifier and wrapper parity"
"$repo_root/scripts/ci/parity.sh"
# Every artifact twice, in two separate processes, compared byte for byte. Two
# processes rather than two calls: an in-process repeat shares the allocator
# state and the address-space layout that a determinism gate exists to catch.
echo "[8/9] determinism double run"
"$repo_root/scripts/ci/determinism.sh"
# The browser surface, built for the target it ships to and driven in a real
# WebAssembly host. The tests beside the exports run natively and cannot catch
# a pointer that is a different width there.
echo "[9/9] browser surface"
"$repo_root/scripts/ci/wasm.sh"
echo "preflight: PASS"
