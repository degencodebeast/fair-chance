#!/usr/bin/env bash
# Checks that scripts/option-t.sh refuses an argument it does not know before it builds or runs
# anything: exit non-zero, a message that names the argument, and no build output.
set -uo pipefail
cd "$(dirname "$0")/.."
failures=0
expect_refused() { # expect_refused <argument>...
  local out code=0
  out="$(scripts/option-t.sh "$@" 2>&1)" || code=$?
  if [ "$code" != 0 ] && printf '%s' "$out" | grep -q "option-t: unknown or incomplete argument" \
    && ! printf '%s' "$out" | grep -q "build-contracts"; then
    echo "ok   refused: $*"
  else
    echo "FAIL not refused: $* (exit $code): $out"
    failures=$((failures + 1))
  fi
}
expect_refused --network=testnet
expect_refused --network
expect_refused --delete-buyer
expect_refused testnet
expect_refused --ticker
expect_refused --name
expect_refused --description --ticker bag
[ "$failures" = 0 ] && echo "option-t arguments: ok" && exit 0
echo "option-t arguments: $failures failed"
exit 1
