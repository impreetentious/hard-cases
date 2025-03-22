#!/usr/bin/env bash
#
# The forbidden-API scan: the numerics constitution, enforced.
#
# clippy covers what a lint can express (unordered maps, ambient clocks). This
# covers what it cannot: a bare tolerance, a fused multiply-add, a wall clock
# smuggled into a budget. Each is invisible in review and corrosive here,
# because the output still looks like a solution.
#
# The perimeter is fail-closed. Every crate under `crates/` is scanned unless
# it is named in HOST_CRATES below, so a new solver crate is covered the day it
# appears rather than the day someone remembers to add it here.
#
# Lines that begin with a comment marker are skipped, so prose may name a
# banned construct. This is a grep, not a parser: a banned call hidden behind a
# trailing comment on a code line will still be caught, but a macro that
# assembles one from fragments will not. That is the known limit.

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root_dir"

# Crates outside the deterministic solver perimeter: they may read clocks,
# touch the filesystem, and report in floating point.
HOST_CRATES=(hc-cli hc-bench hc-wasm hc-fuzz)

solver_paths=()
for crate_dir in crates/*/; do
  crate="$(basename "$crate_dir")"
  skip=0
  for host in "${HOST_CRATES[@]}"; do
    [[ "$crate" == "$host" ]] && skip=1
  done
  [[ $skip -eq 1 ]] || solver_paths+=("$crate_dir")
done

# A perimeter that matches nothing would report success while checking nothing,
# which is the worst outcome a gate can have. Treat it as a failure.
if [[ ${#solver_paths[@]} -eq 0 ]]; then
  echo "forbidden-grep: FAILED — the solver perimeter is empty; the scan would be vacuous"
  exit 1
fi

status=0

# scan <label> <extended-regex> [path-exempt-regex]
#
# The exemption is applied to grep's output rather than through
# `--exclude-dir`, which has no effect on a directory named as a starting path
# and would silently exempt nothing.
scan() {
  local label="$1" pattern="$2" exempt="${3:-}"
  local hits
  hits="$(grep -rnE --include='*.rs' "$pattern" "${solver_paths[@]}" 2>/dev/null \
    | grep -vE ':[0-9]+:[[:space:]]*(//|/\*|\*)' || true)"
  if [[ -n "$hits" && -n "$exempt" ]]; then
    hits="$(printf '%s\n' "$hits" | grep -vE "$exempt" || true)"
  fi
  if [[ -n "$hits" ]]; then
    echo "forbidden: $label"
    echo "$hits" | sed 's/^/  /'
    status=1
  fi
}

# Naked tolerances. Every threshold is a named constant that reads as policy
# at the call site; the only place a literal may live is the tolerance module.
scan "bare 1e-* literal outside hc-core::tol" \
  '[0-9]e-[0-9]' \
  '^crates/hc-core/src/tol\.rs:'

# Fused multiply-add rounds once where two roundings are specified, so a
# native build and a wasm build stop agreeing.
scan "mul_add (FMA is banned; native and wasm diverge)" \
  '\.mul_add\('

# Single precision has no place on a path that produces an exact candidate.
scan "f32 in a solver crate" \
  '\bf32\b'

# Determinism: ordered containers only, no ambient anything.
scan "unordered container in a solver crate" \
  '\b(HashMap|HashSet)\b'

# Effort is counted in iterations and nodes. A clock in a budget check breaks
# determinism and honesty in the same line.
scan "ambient time in a solver crate (effort is counted, never timed)" \
  '\b(SystemTime|Instant)\b'
scan "ambient environment, filesystem, or threads in a solver crate" \
  '\bstd::(env|fs|thread|process)\b'

# The verifier's independence is the entire project. It is checked here rather
# than left to review, because the failure mode is a single convenient import
# that nobody notices and that silently makes the proof circular.
if [[ -d crates/hc-verify ]]; then
  if [[ -f crates/hc-verify/Cargo.toml ]]; then
    deps="$(awk '/^\[(dependencies|dev-dependencies|build-dependencies)\]/{flag=1;next} /^\[/{flag=0} flag && NF && $0 !~ /^[[:space:]]*#/' \
      crates/hc-verify/Cargo.toml || true)"
    if [[ -n "$deps" ]]; then
      echo "forbidden: hc-verify declares dependencies — it must have none"
      echo "$deps" | sed 's/^/  /'
      status=1
    fi
  fi

  imports="$(grep -rnE --include='*.rs' '\b(use|extern crate)[[:space:]]+hc_' crates/hc-verify/ 2>/dev/null \
    | grep -vE ':[0-9]+:[[:space:]]*(//|/\*|\*)' || true)"
  if [[ -n "$imports" ]]; then
    echo "forbidden: hc-verify imports a workspace crate"
    echo "$imports" | sed 's/^/  /'
    status=1
  fi
fi

if [[ $status -ne 0 ]]; then
  echo "forbidden-grep: FAILED"
  exit 1
fi

echo "forbidden-grep: OK — ${#solver_paths[@]} solver crate(s) scanned"
