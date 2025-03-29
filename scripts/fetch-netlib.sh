#!/usr/bin/env bash
#
# Fetches the netlib subset the scoreboard covers.
#
# The instances are not committed. They belong to a public collection with its
# own provenance, and a vendored copy would make the scoreboard a statement
# about the copy. This script retrieves them and pins them.
#
# Pinning happens on the first fetch and is verified on every one after it. A
# digest cannot honestly be written down before a file has been fetched, so the
# first run *creates* the pin and says so in those words; a later run that finds
# a mismatch fails, and does not update the pin. Updating a pin because the
# remote changed is a decision, not a side effect.
#
# Nothing in the build depends on this having been run. The scoreboard reports
# what is present and names what is not.

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="$root_dir/instances/netlib"
pin="$target/pinned.toml"
base="${HC_NETLIB_BASE:-https://netlib.org/lp/data}"

names=(afiro adlittle sc50a sc50b sc105 kb2 blend share2b sc205 stocfor1 scagr7 israel)

if ! command -v curl >/dev/null 2>&1; then
  echo "fetch-netlib: curl is not available; nothing was fetched" >&2
  exit 1
fi

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
    echo "# Digests of the fetched netlib subset."
    echo "#"
    echo "# Written by the first fetch on this machine and verified by every fetch"
    echo "# after it. A mismatch fails; it never updates this file."
    echo ""
    echo "[meta]"
    echo "version = 1"
    echo "title = 'Fetched netlib digests v1'"
    echo "summary = 'One digest per fetched instance, recorded at first fetch.'"
    echo ""
  } > "$pin"
fi

failures=0
for name in "${names[@]}"; do
  file="$target/$name.mps"
  if [[ ! -f "$file" ]]; then
    # The distribution is compressed with a format-specific packer; the plain
    # MPS mirror is preferred when it is available and the failure is reported
    # rather than worked around.
    if ! curl --fail --silent --show-error --location "$base/$name" --output "$file.raw"; then
      echo "fetch-netlib: could not fetch $name" >&2
      rm -f "$file.raw"
      failures=$((failures + 1))
      continue
    fi
    mv "$file.raw" "$file"
  fi

  actual="$(digest_of "$file")"
  if [[ $creating_pin -eq 1 ]]; then
    printf "[instance.%s]\nfile = '%s.mps'\ndigest = '%s'\n\n" "$name" "$name" "$actual" >> "$pin"
    continue
  fi

  expected="$(grep -A2 "^\[instance\.$name\]$" "$pin" | sed -n "s/^digest = '\(.*\)'$/\1/p" | head -1)"
  if [[ -z "$expected" ]]; then
    printf "[instance.%s]\nfile = '%s.mps'\ndigest = '%s'\n\n" "$name" "$name" "$actual" >> "$pin"
    echo "fetch-netlib: $name was not pinned before; its digest has been recorded, not verified"
    continue
  fi
  if [[ "$actual" != "$expected" ]]; then
    echo "fetch-netlib: $name does not match its pin" >&2
    echo "  pinned:  $expected" >&2
    echo "  fetched: $actual" >&2
    failures=$((failures + 1))
  fi
done

if [[ $creating_pin -eq 1 ]]; then
  echo "fetch-netlib: created $pin from this fetch. These digests were RECORDED, not VERIFIED."
  echo "fetch-netlib: every later fetch is checked against them."
fi

if [[ $failures -ne 0 ]]; then
  echo "fetch-netlib: $failures instance(s) could not be fetched or did not match" >&2
  exit 1
fi

echo "fetch-netlib: OK — ${#names[@]} instance(s) present and pinned"
