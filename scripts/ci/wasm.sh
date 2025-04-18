#!/usr/bin/env bash
#
# The browser surface, built and driven.
#
# The module is built for the target it ships to and then run in a real
# WebAssembly host, because the two things that can only go wrong there are the
# two things that matter: an export that does not exist under that name, and a
# pointer that is a different width from the one the tests on the host saw.
#
# A missing target is a failure rather than a skip. The toolchain file declares
# it, so a checkout that cannot build it is a checkout that has not been set up
# the way this project says it is.

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root_dir"

if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
  echo "wasm: FAILED — the wasm32-unknown-unknown target is not installed" >&2
  echo "wasm: rust-toolchain.toml declares it; run rustup target add wasm32-unknown-unknown" >&2
  exit 1
fi
if ! command -v node >/dev/null 2>&1; then
  echo "wasm: FAILED — node is not available and the module cannot be driven" >&2
  exit 1
fi

cargo build --quiet --release --target wasm32-unknown-unknown -p hc-wasm
module="$root_dir/target/wasm32-unknown-unknown/release/hc_wasm.wasm"
if [[ ! -f "$module" ]]; then
  echo "wasm: FAILED — the module was not produced" >&2
  exit 1
fi

node "$root_dir/scripts/ci/wasm-smoke.mjs" "$module" "$root_dir/spec/v1/vectors/mps/tiny.mps"
node "$root_dir/scripts/ci/wasm-smoke.mjs" "$module" "$root_dir/instances/local/cover6.mps"

size="$(wc -c < "$module" | tr -d ' ')"
echo "wasm: OK — $size byte(s), driven in a WebAssembly host"
