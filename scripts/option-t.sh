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

# Refuse an unknown or incomplete argument before anything builds or starts. The runner checks
# the same rules again (option_t::parse_arguments), with duplicates and number formats too.
refuse() { echo "option-t: unknown or incomplete argument: $1" >&2; exit 2; }
features=""
args=("$@")
i=0
while [ "$i" -lt "${#args[@]}" ]; do
  arg="${args[$i]}"
  case "$arg" in
    --delete-buyers|--sells-only|--return-to-funder|--continue) ;;
    --network|--ticker|--name|--description|--start-balance|--spend-cap|--keep|--buy-millinear|--buyer-extra-millinear|--buyer-tgas|--launch-deposit-millinear|--sell-plan|--plan-sha256)
      i=$((i + 1))
      [ "$i" -lt "${#args[@]}" ] || refuse "$arg"
      value="${args[$i]}"
      case "$value" in --*) refuse "$arg" ;; esac
      if [ "$arg" = "--network" ] && [ "$value" = "testnet" ]; then features="--features testnet-keychain"; fi
      ;;
    *) refuse "$arg" ;;
  esac
  i=$((i + 1))
done
if [ -z "$features" ]; then
  : "${NEAR_SANDBOX_BIN_PATH:?set NEAR_SANDBOX_BIN_PATH to a near-sandbox binary}"
fi

FC_OPTION_T_FEATURES="$features" scripts/run-with-lock.sh target/build.lock bash -c '
  set -euo pipefail
  scripts/build-contracts.sh
  cargo build --quiet --locked -p fair-chance-sandbox-tests --bin option-t $FC_OPTION_T_FEATURES'

"${CARGO_TARGET_DIR:-target}/debug/option-t" "$@"
