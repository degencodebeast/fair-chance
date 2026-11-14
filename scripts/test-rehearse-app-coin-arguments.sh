#!/usr/bin/env bash
# Checks that scripts/rehearse-app-coin.sh refuses a run count that is not a plain whole number
# from 1 (no leading zero, no sign, not empty) before it builds or starts anything: exit 2, a
# message that names the rule, and no build output. A dummy sandbox path keeps any start harmless.
set -uo pipefail
cd "$(dirname "$0")/.."
failures=0
for runs in "" 0 00 02 007 -1 +3 x 3x " 3" 1.5; do
  code=0
  out="$(NEAR_SANDBOX_BIN_PATH=/nonexistent/near-sandbox scripts/rehearse-app-coin.sh "$runs" 2>&1)" || code=$?
  if [ "$code" = 2 ] && printf '%s' "$out" | grep -q "rehearse-app-coin: runs takes a whole number from 1" \
    && ! printf '%s' "$out" | grep -q "build-contracts"; then
    echo "ok   refused runs \"$runs\""
  else
    echo "FAIL not refused runs \"$runs\" (exit $code): $out"
    failures=$((failures + 1))
  fi
done
[ "$failures" = 0 ] && echo "rehearse-app-coin arguments: ok" || { echo "rehearse-app-coin arguments: $failures failed"; exit 1; }
