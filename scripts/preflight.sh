#!/usr/bin/env bash
#
# The local mirror of CI. Run this before opening a change; it is the same set
# of checks the `check` workflow runs, in the same order, so a red pipeline is
# something you already saw on your own machine.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "[1/7] version coherence"
node "$repo_root/scripts/check-version-coherence.mjs"
echo "[2/7] normative assets and generated docs"
node "$repo_root/scripts/gen-docs.mjs" --check
echo "[3/7] cargo fmt"
cargo fmt --all -- --check
echo "[4/7] cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings
echo "[5/7] forbidden API scan and verifier isolation"
"$repo_root/scripts/ci/forbidden-grep.sh"
echo "[6/7] tests"
cargo test --workspace --all-targets
echo "[7/7] standalone verifier and wrapper parity"
"$repo_root/scripts/ci/parity.sh"
echo "preflight: PASS"
