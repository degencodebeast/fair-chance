#!/usr/bin/env bash
# Builds the coin wasm with the cargo-near reproducible build (Docker), from the committed
# source at HEAD, then runs the sandbox tests that check the factory's recorded hash and the
# coin's contract_source_metadata against that wasm.
# Output in target/near-reproducible/fair_chance_coin/: fair_chance_coin.wasm,
# built-commit.txt and cargo-near-sha256-base58.txt (the checksum cargo-near printed).
# Needs Docker. The build ignores uncommitted changes, so the script refuses a dirty tree.
set -euo pipefail
cd "$(dirname "$0")/.."

want_cargo_near="0.21.1"
have_cargo_near="$(cargo near --version 2>/dev/null | awk '{print $2}')"
if [ "$have_cargo_near" != "$want_cargo_near" ]; then
  echo "build-coin-reproducible: need cargo-near $want_cargo_near, found '${have_cargo_near:-none}'" >&2
  exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
  echo "build-coin-reproducible: commit or stash every change first; the build reads HEAD only" >&2
  exit 1
fi

out="target/near-reproducible/fair_chance_coin"
rm -rf "$out"
mkdir -p "$out"
commit="$(git rev-parse HEAD)"
log="$out/cargo-near-build.log"
cargo near build reproducible-wasm --manifest-path contracts/coin/Cargo.toml --out-dir "$out" 2>&1 | tee "$log"

# cargo-near prints "Binary checksum (base58): <hash>"; the hash is the code hash on chain.
hash="$(sed 's/\x1b\[[0-9;]*m//g' "$log" | awk '/sha256/ && /[Bb]ase58/ {print $NF}' | tail -1)"
if [ -z "$hash" ]; then
  echo "build-coin-reproducible: no base58 sha256 checksum in the cargo-near output" >&2
  exit 1
fi
echo "$commit" > "$out/built-commit.txt"
echo "$hash" > "$out/cargo-near-sha256-base58.txt"
echo "build-coin-reproducible: commit $commit code hash $hash"

cargo test --locked -p fair-chance-sandbox-tests --test reproducible_coin_build_on_sandbox -- --ignored
