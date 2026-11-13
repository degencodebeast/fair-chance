#!/usr/bin/env bash
# Plays a replay plan on a fresh near-sandbox (sandbox-tests/src/bin/replay-trades-on-sandbox.rs).
# Builds the coin and factory wasm first, as the sandbox refuses a wasm older than its sources.
# Needs NEAR_SANDBOX_BIN_PATH (near-sandbox 2.13.4) and cargo-near 0.21.1.
# Runs at the same time take turns on the build step (lock folder target/build.lock).
# Usage: scripts/replay-trades-on-sandbox.sh play <plan.json> <out.json>
set -euo pipefail
cd "$(dirname "$0")/.."
: "${NEAR_SANDBOX_BIN_PATH:?set NEAR_SANDBOX_BIN_PATH to a near-sandbox binary}"

scripts/run-with-lock.sh target/build.lock bash -c '
  set -euo pipefail
  scripts/build-contracts.sh >/dev/null
  cargo build --quiet --locked -p fair-chance-sandbox-tests --bin replay-trades-on-sandbox'

"${CARGO_TARGET_DIR:-target}/debug/replay-trades-on-sandbox" "$@"
