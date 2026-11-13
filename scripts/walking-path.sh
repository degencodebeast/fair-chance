#!/usr/bin/env bash
# The walking path on a fresh near-sandbox, in one command:
# launch, buy, sell, fc_sell receipt, watcher alert at the fake sender, page receipt by block scan.
# Needs NEAR_SANDBOX_BIN_PATH (near-sandbox 2.13.4), cargo-near 0.21.1, Go and the Node version
# in web/.nvmrc. Each run starts its own sandbox on a free port with a new temporary data folder.
# Arguments go to the walking-path runner (for example --read-in-browser or --hold).
# Runs at the same time take turns on the build step (lock folder target/build.lock), then walk
# side by side.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${NEAR_SANDBOX_BIN_PATH:?set NEAR_SANDBOX_BIN_PATH to a near-sandbox binary}"

want_node="v$(cat web/.nvmrc)"
have_node="$(node --version 2>/dev/null || true)"
if [ "$have_node" != "$want_node" ]; then
  echo "walking-path: need Node $want_node (web/.nvmrc), found '${have_node:-none}'" >&2
  exit 1
fi

scripts/run-with-lock.sh target/build.lock bash -c '
  set -euo pipefail
  scripts/build-contracts.sh
  (cd watcher && go build -o ../target/watcher ./cmd/watcher)
  cargo build --quiet --locked -p fair-chance-sandbox-tests --bin walking-path'

"${CARGO_TARGET_DIR:-target}/debug/walking-path" "$@"
