#!/usr/bin/env bash
#
# The two front doors, compared.
#
# A certificate can be rechecked two ways: by the standalone `hc-verify`
# executable, which links none of the solver, and by `hc verify`, which is a
# wrapper the solver's command line offers for convenience. The specification
# requires them to produce identical output, and this is where that is
# established on the real executables rather than on the library call beneath
# them.
#
# The comparison is on bytes. Two reports that differ by a space are two
# reports, and a release script that parsed one of them would be parsing
# something the other does not produce.
#
# The script also proves the arrangement it is named for: the standalone binary
# is run on a certificate and an instance file with nothing else involved, which
# is what "verification never requires linking the solver" means in practice.

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
    echo "parity: FAILED — $binary was not built; the comparison would be vacuous"
    exit 1
  fi
done

# One instance per terminal shape the linear-program calculus can reach, so a
# regression in any one of them fails here rather than in whichever happens to
# be exercised first.
instances=(
  instances/local/transport33.mps
  instances/local/blend4.mps
  instances/local/infeasible4.mps
  instances/local/unbounded3.mps
  spec/v1/vectors/mps/tiny.mps
)

checked=0
for instance in "${instances[@]}"; do
  certificate="$work/$(basename "$instance" .mps).hcc"
  "$hc" solve "$instance" --cert "$certificate" > "$work/solve.out"

  if [[ ! -s "$certificate" ]]; then
    echo "parity: FAILED — solving $instance produced no certificate"
    cat "$work/solve.out"
    exit 1
  fi

  "$hc_verify" "$certificate" --instance "$instance" > "$work/standalone.out"
  "$hc" verify "$certificate" --instance "$instance" > "$work/wrapper.out"

  if ! cmp -s "$work/standalone.out" "$work/wrapper.out"; then
    echo "parity: FAILED — the two front doors disagree on $instance"
    diff "$work/standalone.out" "$work/wrapper.out" || true
    exit 1
  fi

  if ! grep -q '^CERTIFIED ' "$work/standalone.out"; then
    echo "parity: FAILED — $instance did not certify"
    cat "$work/standalone.out"
    exit 1
  fi
  checked=$((checked + 1))
done

# A tampered certificate must be refused by both, with the same identifier, and
# the exit code must say so. A verifier that says no and exits zero has not
# said no to anything that reads exit codes.
victim="$work/tamper.hcc"
cp "$work/transport33.hcc" "$victim"
python3 - "$victim" <<'PY'
import sys

path = sys.argv[1]
data = bytearray(open(path, "rb").read())
# Change one digit of one rational. The checksum is left alone deliberately:
# this case establishes that corruption is diagnosed, and the mathematical
# cases live in the test suite where the checksum is repaired first.
for index, byte in enumerate(data):
    if 0x31 <= byte <= 0x38:
        data[index] = byte + 1
        break
else:
    raise SystemExit("the certificate carries no decimal digit to change")
open(path, "wb").write(bytes(data))
PY

set +e
"$hc_verify" "$victim" --instance instances/local/transport33.mps > "$work/standalone.bad"
standalone_code=$?
"$hc" verify "$victim" --instance instances/local/transport33.mps > "$work/wrapper.bad"
wrapper_code=$?
set -e

if [[ $standalone_code -eq 0 || $wrapper_code -eq 0 ]]; then
  echo "parity: FAILED — a tampered certificate was accepted"
  exit 1
fi
if [[ $standalone_code -ne $wrapper_code ]]; then
  echo "parity: FAILED — the two front doors exited $standalone_code and $wrapper_code"
  exit 1
fi
if ! cmp -s "$work/standalone.bad" "$work/wrapper.bad"; then
  echo "parity: FAILED — the two front doors report the refusal differently"
  diff "$work/standalone.bad" "$work/wrapper.bad" || true
  exit 1
fi
if ! grep -q '^REJECTED: ' "$work/standalone.bad"; then
  echo "parity: FAILED — the refusal does not name a rule"
  cat "$work/standalone.bad"
  exit 1
fi

echo "parity: OK — $checked certificate(s) agree on both front doors, and a tampered one is refused by both"
