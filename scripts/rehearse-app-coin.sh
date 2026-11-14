#!/usr/bin/env bash
# Sandbox rehearsal of the run on a coin the app launches. Each run starts a fresh near-sandbox at
# about one block every 577 ms, with a 3,700 ms wait before each burst send (the public testnet
# RPC on 14 Nov). The runner makes 10 buyers and a later buyer, signs the burst and waits; a
# stand-in launches the coin from alice at a random time up to 20 s later; the runner sends the
# burst when it finds the coin. The same amounts as the testnet plan: 0.5 NEAR buys, 0.2 NEAR
# extra per buyer, a 0.3 NEAR later buy.
#
#   scripts/rehearse-app-coin.sh [runs]     (default 5)
#
# It prints, per run, the stand-in delay, the detection line (launch block, detect block, expected
# buy block), the blocks the buys landed in and PASS or FAIL, then how many runs had every
# buy and early sell inside blocks 0 to 17. Exit 0 only when every run passed every check.
# Needs NEAR_SANDBOX_BIN_PATH (near-sandbox 2.13.4). Sandbox only: it never signs on testnet.
set -uo pipefail
cd "$(dirname "$0")/.."
# Only a missing argument takes the default; an empty one is refused like any other bad count.
runs="${1-5}"
case "$runs" in ''|*[!0-9]*|0*) echo "rehearse-app-coin: runs takes a whole number from 1, with no leading zero" >&2; exit 2 ;; esac
: "${NEAR_SANDBOX_BIN_PATH:?set NEAR_SANDBOX_BIN_PATH to a near-sandbox binary}"
logs="$(mktemp -d)"
passed=0
in_window=0
for i in $(seq 1 "$runs"); do
  log="$logs/run-$i.log"
  scripts/option-t.sh --coin cabal3.factory.sandbox --buyers 10 --buy-millinear 500 \
    --buyer-extra-millinear 200 --later-buy-millinear 300 \
    --sandbox-block-ms 577 --sandbox-send-delay-ms 3700 > "$log" 2>&1
  code=$?
  buys="$(grep -o 'landed in blocks [0-9]* to [0-9]* after the launch' "$log" | head -1)"
  found="$(grep -o 'launch block [0-9]*, detect block [0-9]* ([0-9]* after the launch block), buys expected by block [0-9]*' "$log" | head -1)"
  delay="$(grep -o 'launches cabal3 as alice.sandbox in [0-9.]* s' "$log" | head -1)"
  if grep -q '^PASS 10 buyers bought in blocks 0 to 17' "$log" \
    && grep -q '^PASS every early sell is refused with FC_EARLY_WINDOW' "$log"; then
    in_window=$((in_window + 1))
  fi
  if [ "$code" = 0 ]; then
    passed=$((passed + 1))
    echo "run $i: PASS; ${delay:-no stand-in line}; ${found:-not found}; buys ${buys:-?}"
  else
    echo "run $i: FAIL (exit $code); ${delay:-no stand-in line}; ${found:-not found}; buys ${buys:-?}; $(grep '^FAIL' "$log" | head -1)"
  fi
done
echo "rehearse-app-coin: $in_window of $runs runs had every buy and early sell in blocks 0 to 17; $passed of $runs passed every check; logs in $logs"
[ "$passed" -eq "$runs" ]
