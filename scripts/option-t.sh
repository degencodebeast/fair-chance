#!/usr/bin/env bash
# Option T on a fresh near-sandbox: one creator launches a coin, 20 buyer accounts buy in blocks
# 0 to 17, then all of them try to sell. It ends with one PASS line per check and exits 0; the
# first failed check prints a FAIL line and exits 1. Arguments go to the runner
# (--buy-millinear <n>, --ticker <ticker>; --network testnet needs the builder's yes flag).
# Needs NEAR_SANDBOX_BIN_PATH (near-sandbox 2.13.4) and cargo-near 0.21.1.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${NEAR_SANDBOX_BIN_PATH:?set NEAR_SANDBOX_BIN_PATH to a near-sandbox binary}"

scripts/run-with-lock.sh target/build.lock bash -c '
  set -euo pipefail
  scripts/build-contracts.sh
  cargo build --quiet --locked -p fair-chance-sandbox-tests --bin option-t'

"${CARGO_TARGET_DIR:-target}/debug/option-t" "$@"
