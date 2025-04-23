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
#
# The third column is what `explain` is expected to do, stated per instance
# rather than tolerated in general. `explain` is the one command here that
# legitimately refuses: a search that reaches no conclusion has no certificate
# to explain, which is what an unbounded relaxation found under presolve does
# — see docs/LIMITATIONS.md. That refusal used to be swallowed with `|| true`,
# and swallowing it swallowed every other one with it: a regression that broke
# `explain` on all six would have left this gate green while it compared two
# identical error messages and skipped the trace comparison entirely, which is
# the vacuous pass this file exists to prevent. It is checked both ways, so an
# instance that starts explaining fails here as a stale table rather than
# passing quietly.
instances=(
  "instances/local/transport33.mps mps-fixed explains"
  "instances/local/cover6.mps mps-fixed explains"
  "instances/local/infeasible4.mps mps-fixed explains"
  "instances/local/unbounded3.mps mps-fixed refuses"
  "spec/v1/vectors/lp/ranges.lp lp explains"
  "spec/v1/vectors/json/exact.json json explains"
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
  explaining="$3"
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
      # that visited nodes in a different order would show up. The status is
      # recorded rather than discarded, and both streams are captured into the
      # compared file, because two runs that refused differently refused for a
      # reason and that reason is what this gate is looking for.
      set +e
      "$hc" explain "$root_dir/$instance" --format "$format" --trace run.hct \
        > explain 2>&1
      echo $? > explain.code
      set -e
      "$hc_verify" proof.hcc --instance "$root_dir/$instance" > verify
    )
  done

  same "$stem canonical dump" "$work/1/dump" "$work/2/dump"
  same "$stem certificate" "$work/1/proof.hcc" "$work/2/proof.hcc"
  same "$stem solve report" "$work/1/solve" "$work/2/solve"
  same "$stem explanation" "$work/1/explain" "$work/2/explain"
  same "$stem verifier verdict" "$work/1/verify" "$work/2/verify"

  # What `explain` did, held against what the table says it does. A status that
  # moved between two runs of the same bytes is a determinism failure on its
  # own, before the question of which of the two was the right status.
  first_status="$(cat "$work/1/explain.code")"
  second_status="$(cat "$work/2/explain.code")"
  if [[ "$first_status" != "$second_status" ]]; then
    echo "determinism: FAILED — $stem explained with status $first_status and then $second_status"
    exit 1
  fi
  if [[ "$explaining" == "explains" ]]; then
    if [[ "$first_status" -ne 0 ]]; then
      echo "determinism: FAILED — $stem is expected to explain and exited $first_status"
      cat "$work/1/explain"
      exit 1
    fi
    # The trace is compared unconditionally once it is required to exist. The
    # emptiness test that used to guard it would skip the comparison on the
    # very regression that stopped traces being written at all.
    if [[ ! -s "$work/1/run.hct" || ! -s "$work/2/run.hct" ]]; then
      echo "determinism: FAILED — $stem explained but wrote no trace; the trace comparison would be vacuous"
      exit 1
    fi
    same "$stem trace" "$work/1/run.hct" "$work/2/run.hct"
  elif [[ "$first_status" -eq 0 ]]; then
    echo "determinism: FAILED — $stem is listed as reaching no conclusion and explained anyway; the table is out of date"
    exit 1
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
