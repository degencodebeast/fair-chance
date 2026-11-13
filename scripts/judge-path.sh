#!/usr/bin/env bash
# The judge path on a fresh near-sandbox, in one command: the factory refuses an opening buy above
# 4%, launches the demo coin, refuses an early-window sell; then a large buy, a refused send, a
# sell that fills in part, a sell that fills the real NEAR cap, the receipt on the coin page in
# headless Chrome, and the watcher alerts. It ends with one PASS line per check and exits 0; the
# first failed check prints a FAIL line and exits 1.
# Needs NEAR_SANDBOX_BIN_PATH (near-sandbox 2.13.4), cargo-near 0.21.1, Go, the Node version in
# web/.nvmrc and Google Chrome (FC_BROWSER_PATH overrides the Chrome binary).
set -euo pipefail
cd "$(dirname "$0")/.."
: "${NEAR_SANDBOX_BIN_PATH:?set NEAR_SANDBOX_BIN_PATH to a near-sandbox binary}"

want_node="v$(cat web/.nvmrc)"
have_node="$(node --version 2>/dev/null || true)"
if [ "$have_node" != "$want_node" ]; then
  echo "judge-path: need Node $want_node (web/.nvmrc), found '${have_node:-none}'" >&2
  exit 1
fi

# npm ci only when node_modules does not match the lock file (a clean checkout has none).
if ! cmp -s web/package-lock.json web/node_modules/.verify-installed-lock.json 2>/dev/null; then
  scripts/run-with-lock.sh target/npm-install.lock bash -c '
    cmp -s web/package-lock.json web/node_modules/.verify-installed-lock.json 2>/dev/null && exit 0
    (cd web && npm ci --no-audit --no-fund --loglevel=error) &&
      cp web/package-lock.json web/node_modules/.verify-installed-lock.json'
fi

scripts/run-with-lock.sh target/build.lock bash -c '
  set -euo pipefail
  scripts/build-contracts.sh
  (cd watcher && go build -o ../target/watcher ./cmd/watcher)
  cargo build --quiet --locked -p fair-chance-sandbox-tests --bin judge-path'

"${CARGO_TARGET_DIR:-target}/debug/judge-path" "$@"
