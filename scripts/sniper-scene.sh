#!/usr/bin/env bash
# The sniper cabal scene on a fresh near-sandbox, in one command:
#   scripts/sniper-scene.sh [out dir]      (default: target/sniper-scene/<run id>/, git-ignored)
# A creator launches a coin; 10 sniper wallets funded by one wallet buy in the first blocks; their
# sells in the early window are refused with FC_EARLY_WINDOW; after the window they share one
# allowance; 3 ordinary buyers buy and sell in full; last, the creator sells its whole bag. Then
# it writes run.json, summary.md, derived.json and the two video charts (SVG, and PNG when Chrome
# is found; FC_BROWSER_PATH overrides the Chrome binary).
# Throwaway sandbox keys only: no testnet, no mainnet. Exit 0 only when every check passes; a
# failed check (for example an early sell after block 17) prints FAIL and no numbers.
# Needs NEAR_SANDBOX_BIN_PATH (near-sandbox 2.13.4), cargo-near 0.21.1 and python3.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${NEAR_SANDBOX_BIN_PATH:?set NEAR_SANDBOX_BIN_PATH to a near-sandbox binary}"

out="${1:-target/sniper-scene/$(date -u +%Y%m%dT%H%M%SZ)-$$}"
mkdir -p "$out"

scripts/run-with-lock.sh target/build.lock bash -c '
  set -euo pipefail
  scripts/build-contracts.sh
  cargo build --quiet --locked -p fair-chance-sandbox-tests --bin sniper-scene'

start=$(date +%s)
if ! "${CARGO_TARGET_DIR:-target}/debug/sniper-scene" "$out/run.json"; then
  echo "sniper-scene: FAIL, the scene did not pass its checks; no numbers printed" >&2
  exit 1
fi
python3 -I analysis/sniper-scene/analyze.py "$out/run.json" "$out"
echo "sniper-scene: done in $(( $(date +%s) - start )) s"
echo
cat "$out/summary.md"
echo "sniper-scene: files in $out"
