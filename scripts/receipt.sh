#!/usr/bin/env bash
#
# The release receipt.
#
# A verifier asks to be believed, and this is what it offers in exchange: how
# much code did the checking, what that code depends on, the hash of the exact
# source, and what one real verification cost in units that are not seconds.
#
# Everything printed here is measured at the moment of printing. Nothing is
# transcribed from a previous run and nothing is a target. Wall time is shown
# because hiding it would be coy, and it is labelled as disclosure because it
# gates nothing and is never divided into anything.

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

instance="${1:-instances/local/transport33.mps}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cargo build --quiet --bin hc --bin hc-verify

echo "hard cases release receipt"
echo
"$root_dir/target/debug/hc" --version | sed 's/^/  /'
if revision="$(git rev-parse HEAD 2>/dev/null)"; then
  echo "  revision  $revision"
  if ! git diff --quiet HEAD 2>/dev/null; then
    # A receipt taken over a dirty tree names source that is not in any commit.
    # Saying so is the difference between a receipt and a decoration.
    echo "  revision  (working tree has uncommitted changes)"
  fi
fi
echo

"$root_dir/target/debug/hc" receipt crates/hc-verify

echo
echo "one verification"
echo "  instance       $instance"
"$root_dir/target/debug/hc" solve "$instance" --cert "$work/receipt.hcc" > /dev/null

started="$(date +%s)"
"$root_dir/target/debug/hc-verify" "$work/receipt.hcc" --instance "$instance" > "$work/verified.out"
finished="$(date +%s)"

grep -E '^(certificate|work|widest|CERTIFIED|REJECTED)' "$work/verified.out" | sed 's/^/  /'
echo "  wall clock     $((finished - started))s (disclosed; gates nothing, normalized into nothing)"
