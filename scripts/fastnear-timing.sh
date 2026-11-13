#!/usr/bin/env bash
# The FastNEAR timing test on one coin: for each event, the time from the block that holds its
# receipt to the first public FastNEAR read that shows it, and the testnet seconds per block from
# the launch block to the final head. Start it in its own terminal before
# scripts/option-t.sh --network testnet; it waits for the coin to exist and stops 3 minutes after
# the last new event (or after 30 minutes). Reads only public endpoints, no key; backs off on 429,
# stops on 403. Signs nothing. Needs Go.
#
#   scripts/fastnear-timing.sh <coin account> <report file>
set -euo pipefail
cd "$(dirname "$0")/.."

coin="${1:-}"
out="${2:-}"
case "$coin" in
  *.testnet) ;;
  *) echo "fastnear-timing: give the coin account, a .testnet account, and a report file" >&2; exit 2 ;;
esac
[ -n "$out" ] || { echo "fastnear-timing: give a report file" >&2; exit 2; }

(cd watcher && go build -o ../target/fastnear-timing ./cmd/fastnear-timing)
exec target/fastnear-timing -coin "$coin" -out "$out" -poll 1s
