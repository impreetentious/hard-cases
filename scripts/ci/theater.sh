#!/usr/bin/env bash
#
# The situation room, built and driven.
#
# Four things, in the order they can fail. The page's assets are collected from
# what the Rust build just produced, so a page bundled around a stale module is
# caught before anything else runs. The types are checked. The page is built.
# Then it is driven in a real browser against that build, because what ships is
# the build and a page that worked under a module graph and broke under a
# bundle would be a page nobody had tested.
#
# The budget is checked on the built bytes rather than declared in a document.
# A static page about a solver has no business being a megabyte of JavaScript,
# and the only way that stays true is if something refuses when it stops being
# true.
#
# Missing tools are failures rather than skips, and each one prints the command
# that fixes it. A check that quietly does nothing on a machine that cannot run
# it is a check that reports green from a machine that tested nothing.

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root_dir"

theater="$root_dir/theater"

if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
  echo "theater: FAILED — node and npm are needed to build the page" >&2
  exit 1
fi

module="$root_dir/target/wasm32-unknown-unknown/release/hc_wasm.wasm"
if [[ ! -f "$module" ]]; then
  echo "theater: FAILED — the WebAssembly module has not been built" >&2
  echo "theater: cargo build --release --target wasm32-unknown-unknown -p hc-wasm" >&2
  exit 1
fi
if [[ ! -x "$root_dir/target/release/hc" ]]; then
  echo "theater: FAILED — the command line has not been built" >&2
  echo "theater: the page ships a recording that only it can make" >&2
  echo "theater: cargo build --release -p hc-cli" >&2
  exit 1
fi

if [[ ! -d "$theater/node_modules" ]]; then
  echo "theater: installing dependencies"
  npm --prefix "$theater" ci --no-audit --no-fund >/dev/null
fi

# The assets, collected from what the Rust build produced. This also replays
# the shipped recording through the module the page will load, so a broken
# button is a failed build rather than something a visitor finds.
node "$theater/scripts/prepare.mjs"

npm --prefix "$theater" run check --silent
npm --prefix "$theater" run build --silent >/dev/null

# The budgets. Generous enough that ordinary work never touches them and tight
# enough that a dependency nobody meant to add cannot hide.
# Plain variables rather than an associative array: the system shell on one of
# the two platforms this is run on is old enough not to have them, and a check
# that only runs on half the machines is half a check.
script_budget=262144
style_budget=32768
module_budget=1835008
measure() {
  local total=0
  for file in $1; do
    [[ -f "$file" ]] || continue
    total=$((total + $(wc -c < "$file" | tr -d ' ')))
  done
  echo "$total"
}
script_bytes="$(measure "$theater/dist/assets/*.js")"
style_bytes="$(measure "$theater/dist/assets/*.css")"
module_bytes="$(measure "$module")"
failed=0
check_budget() {
  local name="$1" got="$2" cap="$3"
  if (( got > cap )); then
    echo "theater: FAILED — $name is $got bytes and the budget is $cap" >&2
    failed=1
  fi
}
check_budget script "$script_bytes" "$script_budget"
check_budget style "$style_bytes" "$style_budget"
check_budget module "$module_bytes" "$module_budget"
(( failed == 0 )) || exit 1

# And driven. A missing browser is a failure with the command that fixes it,
# for the same reason a missing compilation target is.
if ! (cd "$theater" && npx playwright test --reporter=line); then
  echo "theater: FAILED — the browser suite did not pass" >&2
  echo "theater: if no browser is installed, run" >&2
  echo "theater:   npx --prefix theater playwright install chromium" >&2
  exit 1
fi

echo "theater: OK — ${script_bytes} B of script, ${style_bytes} B of style, ${module_bytes} B of module, driven in a browser"
