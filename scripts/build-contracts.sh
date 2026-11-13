#!/usr/bin/env bash
# Builds the coin and factory wasm into target/near/<crate>/<crate>.wasm.
# The sandbox tests refuse a wasm older than its sources. cargo-near keeps the old file time
# when nothing changed, so after a successful build this script touches each wasm.
set -euo pipefail
cd "$(dirname "$0")/.."

want_cargo_near="0.21.1"
have_cargo_near="$(cargo near --version 2>/dev/null | awk '{print $2}')"
if [ "$have_cargo_near" != "$want_cargo_near" ]; then
  echo "build-contracts: need cargo-near $want_cargo_near, found '${have_cargo_near:-none}'" >&2
  exit 1
fi

for crate in coin factory; do
  cargo near build non-reproducible-wasm --locked --no-abi --manifest-path "contracts/$crate/Cargo.toml" >/dev/null 2>&1 \
    || cargo near build non-reproducible-wasm --locked --no-abi --manifest-path "contracts/$crate/Cargo.toml"
  touch "target/near/fair_chance_$crate/fair_chance_$crate.wasm"
done
echo "build-contracts: target/near/fair_chance_coin/fair_chance_coin.wasm target/near/fair_chance_factory/fair_chance_factory.wasm"
