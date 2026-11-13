#!/usr/bin/env bash
# The walking path on a fresh near-sandbox, in one command:
# launch, buy, sell, fc_sell receipt, watcher alert at the fake sender, page receipt by block scan.
# Needs NEAR_SANDBOX_BIN_PATH (a near-sandbox binary), cargo-near, Go and Node 24.
# Each run starts its own sandbox on a free port with a new temporary data folder.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${NEAR_SANDBOX_BIN_PATH:?set NEAR_SANDBOX_BIN_PATH to a near-sandbox binary}"
scripts/build-contracts.sh
(cd watcher && go build -o ../target/watcher ./cmd/watcher)
cargo run --quiet --locked -p fair-chance-sandbox-tests --bin walking-path
