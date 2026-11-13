#!/usr/bin/env bash
# Option T on a fresh near-sandbox: one creator launches a coin, 20 buyer accounts buy in blocks
# 0 to 17, then all of them try to sell. It ends with one PASS line per check and exits 0; the
# first failed check prints a FAIL line and exits 1. Arguments go to the runner
# (sandbox-tests/src/bin/option-t.rs lists them). --network testnet signs real transactions and
# runs only with the builder's yes flag; for it this script builds the keychain signer
# (feature testnet-keychain). --delete-buyers is the separate delete step.
# Needs NEAR_SANDBOX_BIN_PATH (near-sandbox 2.13.4) and cargo-near 0.21.1.
set -euo pipefail
cd "$(dirname "$0")/.."

features=""
previous=""
for arg in "$@"; do
  if [ "$previous" = "--network" ] && [ "$arg" = "testnet" ]; then features="--features testnet-keychain"; fi
  previous="$arg"
done
if [ -z "$features" ]; then
  : "${NEAR_SANDBOX_BIN_PATH:?set NEAR_SANDBOX_BIN_PATH to a near-sandbox binary}"
fi

FC_OPTION_T_FEATURES="$features" scripts/run-with-lock.sh target/build.lock bash -c '
  set -euo pipefail
  scripts/build-contracts.sh
  cargo build --quiet --locked -p fair-chance-sandbox-tests --bin option-t $FC_OPTION_T_FEATURES'

"${CARGO_TARGET_DIR:-target}/debug/option-t" "$@"
