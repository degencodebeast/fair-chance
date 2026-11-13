#!/usr/bin/env bash
# The factory deploy step: the coin code into global storage by code hash, then factory.<funder>
# made, deployed and started in one transaction. Arguments go to the runner
# (sandbox-tests/src/bin/deploy-factory.rs): exactly --dry-run, or --network testnet, which signs
# real transactions and runs only with the builder's yes flag. Testnet deploys only the coin wasm
# from scripts/build-coin-reproducible.sh, built from HEAD of a clean tree.
# Needs FC_TESTNET_FUNDER; testnet also needs FC_TESTNET_RPC_URL. Needs cargo-near 0.21.1.
set -euo pipefail
cd "$(dirname "$0")/.."

features=""
case "$*" in
  "--dry-run") ;;
  "--network testnet") features="--features testnet-keychain" ;;
  *) echo "deploy-factory: use exactly --dry-run or --network testnet" >&2; exit 2 ;;
esac

FC_DEPLOY_FEATURES="$features" scripts/run-with-lock.sh target/build.lock bash -c '
  set -euo pipefail
  scripts/build-contracts.sh
  cargo build --quiet --locked -p fair-chance-sandbox-tests --bin deploy-factory $FC_DEPLOY_FEATURES'

"${CARGO_TARGET_DIR:-target}/debug/deploy-factory" "$@"
