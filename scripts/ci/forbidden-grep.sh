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
# The verifier ships an executable, and an executable reads its arguments and
# its input. Exactly one file is allowed to do so; the exemption names that one
# path and nothing else, and the check below pays for it by proving the path
# exists and is the only place in the crate that reaches outside the process.
scan "ambient environment, filesystem, or threads in a solver crate" \
  '\bstd::(env|fs|thread|process)\b' \
  '^crates/hc-verify/src/bin/hc-verify\.rs:'

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

  # A test inside the crate imports the crate; that is not a boundary
  # crossing, and the exemption below names exactly that one crate and nothing
  # else. Everything beginning `hc_` and spelled otherwise is still refused.
  imports="$(grep -rnE --include='*.rs' '\b(use|extern crate)[[:space:]]+hc_' crates/hc-verify/ 2>/dev/null \
    | grep -vE ':[0-9]+:[[:space:]]*(//|/\*|\*)' \
    | grep -vE '\b(use|extern crate)[[:space:]]+hc_verify\b' || true)"
  if [[ -n "$imports" ]]; then
    echo "forbidden: hc-verify imports a workspace crate"
    echo "$imports" | sed 's/^/  /'
    status=1
  fi

  # The manifest and the source can both be clean while the resolved graph is
  # not: a dependency can arrive through a feature, a target-specific table, or
  # a patch. This asks the resolver instead of the text, and the answer must be
  # the crate alone. Development and build edges are included, because a
  # verifier whose test harness pulls in the solver is a verifier whose vectors
  # were checked by the code they are meant to check.
  if command -v cargo >/dev/null 2>&1; then
    tree="$(cargo tree --offline --quiet --package hc-verify --edges normal,build,dev --prefix none 2>/dev/null \
      | sed '/^[[:space:]]*$/d' || true)"
    if [[ -z "$tree" ]]; then
      echo "forbidden: hc-verify dependency graph could not be resolved; the isolation check would be vacuous"
      status=1
    else
      extra="$(printf '%s\n' "$tree" | grep -vE '^hc-verify v' || true)"
      if [[ -n "$extra" ]]; then
        echo "forbidden: hc-verify has something beneath it in the dependency graph"
        printf '%s\n' "$extra" | sed 's/^/  /'
        status=1
      fi
    fi
  fi
fi

# The price of the exemption above. The verifier's executable is allowed to
# reach outside the process; nothing else in the crate is, and the exemption is
# worth only as much as that. The check is fail-closed in both directions: if
# the exempted file has vanished, the exemption is covering nothing and that is
# a failure, and if a second file in the crate reaches outside, the exemption
# has quietly widened and that is a failure too.
if [[ -d crates/hc-verify ]]; then
  entry="crates/hc-verify/src/bin/hc-verify.rs"
  if [[ ! -f "$entry" ]]; then
    echo "forbidden: the verifier's exempted entry point is missing; the exemption covers nothing"
    status=1
  else
    outward="$(grep -rlE --include='*.rs' '\bstd::(env|fs|thread|process)\b' crates/hc-verify/ 2>/dev/null \
      | grep -vE "^$entry$" || true)"
    if [[ -n "$outward" ]]; then
      echo "forbidden: a second file in hc-verify reaches outside the process"
      echo "$outward" | sed 's/^/  /'
      status=1
    fi
    # The entry point is a shell. It reads, it prints, it exits; the checking
    # happens in the library, where the tests can reach it. A binary that grew
    # logic would be logic outside every harness in the project.
    lines="$(grep -cvE '^[[:space:]]*(//|$)' "$entry" || true)"
    if [[ "$lines" -gt 120 ]]; then
      echo "forbidden: the verifier's entry point has grown to $lines lines; it is meant to be a shell"
      status=1
    fi
  fi
fi

# The reference solver is the oracle the production search is checked against.
# An oracle that imports the code it checks agrees with it exactly where
# agreement is worthless, so the reference module may not reach for the
# production linear algebra or search. The scan is fail-closed the other way
# too: if the bench crate exists and the reference module has vanished, the
# check would silently pass while covering nothing, so that is a failure.
if [[ -d crates/hc-bench ]]; then
  ref_paths=()
  [[ -f crates/hc-bench/src/ref_lp.rs ]] && ref_paths+=(crates/hc-bench/src/ref_lp.rs)
  [[ -d crates/hc-bench/src/ref_lp ]] && ref_paths+=(crates/hc-bench/src/ref_lp)
  if [[ ${#ref_paths[@]} -eq 0 ]]; then
    echo "forbidden: crates/hc-bench exists but the reference solver is missing; the oracle check would be vacuous"
    status=1
  else
    borrowed="$(grep -rnE --include='*.rs' '\b(use|extern crate)[[:space:]]+hc_(lp|mip|cuts|heur|presolve)\b' "${ref_paths[@]}" 2>/dev/null \
      | grep -vE ':[0-9]+:[[:space:]]*(//|/\*|\*)' || true)"
    if [[ -n "$borrowed" ]]; then
      echo "forbidden: the reference solver imports production solver code"
      echo "$borrowed" | sed 's/^/  /'
      status=1
    fi
  fi
fi

if [[ $status -ne 0 ]]; then
  echo "forbidden-grep: FAILED"
  exit 1
fi

echo "forbidden-grep: OK — ${#solver_paths[@]} solver crate(s) scanned"
