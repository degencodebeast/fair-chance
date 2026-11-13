#!/usr/bin/env bash
# Builds the coin and factory wasm into target/near/<crate>/<crate>.wasm.
set -euo pipefail
cd "$(dirname "$0")/.."
for crate in coin factory; do
  cargo near build non-reproducible-wasm --locked --no-abi --manifest-path "contracts/$crate/Cargo.toml" >/dev/null 2>&1 \
    || cargo near build non-reproducible-wasm --locked --no-abi --manifest-path "contracts/$crate/Cargo.toml"
done
echo "build-contracts: target/near/fair_chance_coin/fair_chance_coin.wasm target/near/fair_chance_factory/fair_chance_factory.wasm"
