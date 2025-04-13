#!/usr/bin/env bash
#
# Fetches the mixed-integer subset the scoreboard covers.
#
# The same shape as the linear fetch beside it, and the same rules. The files
# are not committed; the first fetch on a machine *records* a pin and says so in
# those words; every fetch after it verifies against that pin and fails on a
# mismatch, never updating it. Updating a pin because the remote changed is a
# decision, not a side effect.
#
# The distribution is gzip-compressed, so this needs `gunzip`. That is a
# requirement of the collection rather than a dependency of the project:
# nothing in the build depends on this script having been run, and the harness
# reports what is present and names what is not.

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="$root_dir/instances/miplib"
pin="$target/pinned.toml"
base="${HC_MIPLIB_BASE:-https://miplib.zib.de/WebData/instances}"

names=(enlight_hard gen-ip002 gen-ip054 flugpl gr4x6 stein27 ej markshare_4_0 mas74 mas76 misc03 mzzv11 neos5 noswot p0033 p0201 pk1 stein45 timtab1 tr12-30)

for tool in curl gunzip; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "fetch-miplib: $tool is not available; nothing was fetched" >&2
    exit 1
  fi
done

digest_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

mkdir -p "$target"
creating_pin=0
if [[ ! -f "$pin" ]]; then
  creating_pin=1
  {
    echo "# Digests of the fetched mixed-integer subset."
    echo "#"
    echo "# Written by the first fetch on this machine and verified by every fetch"
    echo "# after it. A mismatch fails; it never updates this file."
    echo ""
    echo "[meta]"
    echo "version = 1"
    echo "title = 'Fetched mixed-integer digests v1'"
    echo "summary = 'One digest per fetched instance, recorded at first fetch.'"
    echo ""
  } > "$pin"
fi

failures=0
for name in "${names[@]}"; do
  file="$target/$name.mps"
  if [[ ! -f "$file" ]]; then
    if ! curl --fail --silent --show-error --location "$base/$name.mps.gz" --output "$file.gz"; then
      echo "fetch-miplib: could not fetch $name" >&2
      rm -f "$file.gz"
      failures=$((failures + 1))
      continue
    fi
    if ! gunzip --force "$file.gz"; then
      echo "fetch-miplib: could not decompress $name" >&2
      rm -f "$file.gz" "$file"
      failures=$((failures + 1))
      continue
    fi
  fi

  actual="$(digest_of "$file")"
  if [[ $creating_pin -eq 1 ]]; then
    printf "[instance.%s]\nfile = '%s.mps'\ndigest = '%s'\n\n" "$name" "$name" "$actual" >> "$pin"
    continue
  fi

  expected="$(grep -A2 "^\[instance\.$name\]$" "$pin" | sed -n "s/^digest = '\(.*\)'$/\1/p" | head -1)"
  if [[ -z "$expected" ]]; then
    printf "[instance.%s]\nfile = '%s.mps'\ndigest = '%s'\n\n" "$name" "$name" "$actual" >> "$pin"
    echo "fetch-miplib: $name was not pinned before; its digest has been recorded, not verified"
    continue
  fi
  if [[ "$actual" != "$expected" ]]; then
    echo "fetch-miplib: $name does not match its pin" >&2
    echo "  pinned:  $expected" >&2
    echo "  fetched: $actual" >&2
    failures=$((failures + 1))
  fi
done

if [[ $creating_pin -eq 1 ]]; then
  echo "fetch-miplib: created $pin from this fetch. These digests were RECORDED, not VERIFIED."
  echo "fetch-miplib: every later fetch is checked against them."
fi

if [[ $failures -ne 0 ]]; then
  echo "fetch-miplib: $failures instance(s) could not be fetched or did not match" >&2
  exit 1
fi

echo "fetch-miplib: OK — ${#names[@]} instance(s) present and pinned"
