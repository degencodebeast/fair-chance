# Launch, buy and limited sell of a coin

A creator launches a coin through the factory. A trader buys the coin with NEAR and sells part of it back. The sell emits one `fc_sell` event with the interface field names.

## Sub-features

- `launch` creates `<ticker>.<factory>` with the coin code by hash and emits `fc_launch` (standard `fair_chance_factory`).
- `buy` takes attached NEAR and credits tokens.
- `sell` takes tokens and 1 yoctoNEAR, pays NEAR and emits `fc_sell` (standard `fair_chance`).
- `version` reports the interface version in `contract_source_metadata`.

## How to get to it (user POV)

- A creator calls the factory method `launch(name, ticker, icon, description, opening_buy)` with a deposit.
- A trader calls the coin methods `buy(min_tokens_out)` and `sell(amount, min_near_out)`.
- No page has launch, buy or sell buttons yet.

## Driving it with the walking-path runner

Preconditions:

- `verify.sh doctor` passes.

- **Launch.** Run `.claude/skills/verify/verify.sh`. The log shows `walking-path:   coin walk.factory.sandbox launch_block <n> version "<interface version>"`.
- **Buy.** The log shows `walking-path: one account buys`, and the next step reads `ft_balance_of` for `alice.sandbox`.
- **Limited sell.** The log shows `walking-path:   fc_sell in receipt <id> with the rc.4 field names: tokens_sold "<n>" near_out "<n>"`.
- **Proof.** `verify.log` in the run's evidence folder ends with `verify: PASS (sandbox)`.

## Gotchas

- The runner refuses a wasm older than its sources. `scripts/build-contracts.sh` rebuilds and touches it; do not copy wasm files by hand.
- The sell is "limited" only by name today: the limit fields are stubs until the rule engine adds them. The run proves the path, not the limits.
- A buy with 0 NEAR passes today; that is a known gap, not a verify failure.
