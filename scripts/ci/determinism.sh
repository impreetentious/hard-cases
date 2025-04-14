#!/usr/bin/env bash
#
# The double run.
#
# Every artifact this project produces is a function of its inputs and nothing
# else. That is not a preference: a certificate whose bytes depend on where the
# heap happened to put a table cannot be compared between two versions, a trace
# whose event order depends on hash iteration cannot be replayed, and a work
# figure that moves cannot be a budget.
#
# So each artifact is produced twice, in two separate processes, and compared
# byte for byte. Two processes rather than two calls in one: an in-process
# repeat shares its allocator state, its address-space layout, and any lazily
# initialized table with the first run, which is exactly the shared state a
# determinism gate exists to catch.
#
# The comparison covers what a reader of this repository is asked to trust: the
# canonical dump both parsers must agree on, the certificate the verifier
# checks, the verdict and the work units it reports, the explanation drawn from
# the certificate, and the trace of the run.

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root_dir"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cargo build --quiet --bin hc --bin hc-verify

hc="$root_dir/target/debug/hc"
hc_verify="$root_dir/target/debug/hc-verify"
for binary in "$hc" "$hc_verify"; do
  if [[ ! -x "$binary" ]]; then
    echo "determinism: FAILED — $binary was not built; the comparison would be vacuous"
    exit 1
  fi
done

# One instance per shape the artifacts can take: a linear program, a mixed
# integer program with a tree, an infeasible model, an unbounded one, and one
# of each of the other two accepted formats.
instances=(
  "instances/local/transport33.mps mps-fixed"
  "instances/local/cover6.mps mps-fixed"
  "instances/local/infeasible4.mps mps-fixed"
  "instances/local/unbounded3.mps mps-fixed"
  "spec/v1/vectors/lp/ranges.lp lp"
  "spec/v1/vectors/json/exact.json json"
)

compared=0
differed=0

# same <name> <first> <second>
same() {
  local name="$1" first="$2" second="$3"
  compared=$((compared + 1))
  if ! cmp -s "$first" "$second"; then
    echo "determinism: FAILED — $name differs between two runs"
    diff "$first" "$second" | head -20 || true
    differed=$((differed + 1))
  fi
}

for entry in "${instances[@]}"; do
  set -- $entry
  instance="$1"
  format="$2"
  stem="$(basename "${instance%.*}").$format"

  # Each pass runs in its own directory with identical arguments, because the
  # reports name the files they were given and two runs that were handed
  # different names would differ for a reason that has nothing to do with the
  # solver. The instance is named absolutely, which is the same string in both.
  for pass in 1 2; do
    mkdir -p "$work/$pass"
    (
      cd "$work/$pass"
      "$hc" check "$root_dir/$instance" --format "$format" > dump
      "$hc" solve "$root_dir/$instance" --format "$format" --cert proof.hcc > solve
      # The explanation and the trace of the same run, which is where a search
      # that visited nodes in a different order would show up.
      "$hc" explain "$root_dir/$instance" --format "$format" --trace run.hct \
        > explain 2>/dev/null || true
      "$hc_verify" proof.hcc --instance "$root_dir/$instance" > verify
    )
  done

  same "$stem canonical dump" "$work/1/dump" "$work/2/dump"
  same "$stem certificate" "$work/1/proof.hcc" "$work/2/proof.hcc"
  same "$stem solve report" "$work/1/solve" "$work/2/solve"
  same "$stem explanation" "$work/1/explain" "$work/2/explain"
  same "$stem verifier verdict" "$work/1/verify" "$work/2/verify"
  if [[ -s "$work/1/run.hct" || -s "$work/2/run.hct" ]]; then
    same "$stem trace" "$work/1/run.hct" "$work/2/run.hct"
  fi

  # A verdict that carried no work figure would compare equal while measuring
  # nothing, so the figure is required to be there and to be nonzero.
  if ! grep -qE '^work +[1-9][0-9]* units' "$work/1/verify"; then
    echo "determinism: FAILED — $stem reports no verifier work units"
    cat "$work/1/verify"
    exit 1
  fi
  rm -rf "$work/1" "$work/2"
done

if [[ $compared -eq 0 ]]; then
  echo "determinism: FAILED — nothing was compared"
  exit 1
fi
if [[ $differed -ne 0 ]]; then
  echo "determinism: FAILED — $differed artifact(s) differ between two runs"
  exit 1
fi

echo "determinism: OK — $compared artifact(s) identical across two separate runs"
