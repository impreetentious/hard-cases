#!/usr/bin/env bash
#
# The local mirror of CI. Run this before opening a change; it is the same set
# of checks the `check` workflow runs, in the same order, so a red pipeline is
# something you already saw on your own machine.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "[1/6] version coherence"
node "$repo_root/scripts/check-version-coherence.mjs"
echo "[2/6] normative assets and generated docs"
node "$repo_root/scripts/gen-docs.mjs" --check
echo "[3/6] cargo fmt"
cargo fmt --all -- --check
echo "[4/6] cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings
echo "[5/6] forbidden API scan and verifier isolation"
"$repo_root/scripts/ci/forbidden-grep.sh"
echo "[6/6] tests"
cargo test --workspace --all-targets
echo "preflight: PASS"
