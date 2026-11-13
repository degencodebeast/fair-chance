# Evidence

This page lists what each test proves and what the tests leave open. It is reference, not a promise. Numbers from a replay or a simulation are simulation, not demand.

Run every test with `cargo test --workspace --locked`. Sandbox tests need `NEAR_SANDBOX_BIN_PATH` and the wasm from `scripts/build-contracts.sh`. The merge gate is `.claude/skills/verify/verify.sh`.

<!-- waits on: the final test count and the date of the run -->

## Rule tests (`rule-engine/tests/`)

| File | What it shows |
| --- | --- |
| `pool_buy_and_sell.rs` | Constant-product buy and sell, the NEAR reserve minimum, slippage refusal. |
| `unmarked_allowance.rs` | One allowance per account per 1 hour window, counted net in tokens, with no carry-over. |
| `holder_class_and_sends.rs` | Large accounts cannot send. A send that would make a receiver large is refused. Sends to the coin and to yourself are refused. Unregister is refused while tokens are held. |
| `window_class_floor.rs` | A band account that sells below one allowance stays band until the window ends. A large account that sells to zero is band until the window ends. An account that was large and holds a bag inside one allowance is still limited as large: at a full cap it fills only its floor. The floor resets in the next window. |
| `partial_fill_and_band_penalty.rs` | A large sell above the allowance fills in part and the rest stays in the wallet. A band sell inside one allowance pays no penalty. A band sell above it pays 10% on the part above. |
| `real_near_cap.rs` | <!-- waits on: the marked-token cap rule --> |
| `real_near_cap_properties.rs` | <!-- waits on: the marked-token cap rule --> |
| `launch_and_rules.rs` | `new` mints the whole supply once, into the pool. The parent account alone can call `new`. `fc_rules` returns every fixed value. |
| `walking_path.rs` | Launch, buy, sell and receipt in the rule engine alone. |
| `sell_floor_cap_and_limit_reason.rs` | One floor per account per 1 hour window: a buy gives no floor room back, and marked and unmarked floors do not stack, in either order. The marked part of a small sell stays within the cap. A large sell that fills only its unmarked floor reports `floor`. A small sell with a marked part and tokens bought in the window may fill less of the bought part when the cap is tight, and a second sell does not avoid it, because a sell takes its marked part first. |

<!-- waits on: the marked-token cap rule (tests for early supply and the cap, and the property file's final case count) -->

## Sandbox tests (`sandbox-tests/tests/`)

| File | What it shows |
| --- | --- |
| `factory_launch_on_sandbox.rs` | The coin runs the recorded global code by hash and has no access keys. A second `new` is refused, even from the factory. A failed launch leaves no coin and returns the deposit. Two coins have equal fixed rules and separate state. The factory gets no special right on any coin method. |
| `launch_buy_sell_on_sandbox.rs` | A sell logs `fc_sell` with the interface field names. A sell without 1 yoctoNEAR is refused with `FC_ONE_YOCTO`. `ft_transfer_call` is refused. |
| `first_buy_storage_on_sandbox.rs` | A buy from an account with no storage record is refused with the NEP-145 text and returns the NEAR. `storage_deposit` and `buy` work in one transaction. |
| `band_buy_and_sell_back_on_sandbox.rs` | A band account that buys and sells back in one window follows the cap rules on a real chain. |
| `coin_method_list.rs` | The compiled coin exports interface and standard methods, and nothing else. No burn, owner, pause, setter or upgrade method exists. |
| `wasm_freshness.rs` | The tests refuse a wasm older than its sources. |

## Watcher and page

- `watcher/` has Go tests for alert text, event reading, commands and the check card. A recorded-RPC fixture replaces the network.
- `web/` has Vitest tests for the page model and receipts. `web/scripts/` loads the page in headless Chrome with every outside host stubbed, so no request leaves the machine.
- The gate runs a walking path: launch, buy, sell, page receipt in Chrome, watcher alert at a fake sender.

## Known limits

Each limit has an owner decision in an ADR. The tests above show the rule works. They do not show these routes closed.

| Limit | Why it stays | Source |
| --- | --- | --- |
| A fresh account gets a fresh allowance. | <!-- waits on: the marked-token cap rule --> | |
| Floors above the cap. | <!-- waits on: the marked-token cap rule --> | |
| Creator-funded wallets. | The contract cannot see who funded an account. | ADR 0007 in `docs/adr/` |
| Late splitters after block N. | Accounts that buy at market price stay unmarked. | [ADR 0008](adr/0008-snipers-are-early-supply-with-early-fee.md) |
| Bots at the reset. | Every allowance and the cap reset together. | [INTERFACE](INTERFACE.md) |
| Two allowances at a window boundary. | Windows sit on a fixed grid. | [ADR 0004](adr/0004-partial-fill-band-penalty-per-class-windows.md) |
| Account handover. | No token moves, so no rule sees it. | [ADR 0001](adr/0001-large-holders-cannot-send-shared-early-allowance.md) |
| A band holder sends parts to fresh accounts. | Band accounts can send. | [ADR 0001](adr/0001-large-holders-cannot-send-shared-early-allowance.md) |
| A band account buys, sends the bought tokens to a fresh small account that sells them uncounted, and keeps the buy as an offset for its own counted sells. | Wallet splitting. Each fresh small wallet carries at most one allowance per window. Measured at a full cap: up to 0.31 of one allowance's NEAR per fresh wallet. | [ADR 0005](adr/0005-live-pool-wide-cap-on-real-near.md) |
| A buy raises the cap by c x X, and a band account with marked tokens fills its marked sell into that room, then sells the bought tokens inside its first allowance. | At most c x X and at most one unmarked floor per window. Measured: up to 0.29 of one floor (2,340 swept shapes). | [ADR 0005](adr/0005-live-pool-wide-cap-on-real-near.md) |
| A small sell that has a marked part and tokens bought in the window may fill less of the bought part when the cap is tight, though that part adds nothing to the cap used. A second sell takes its marked part first, so it does not help while marked tokens are left. | The cap holds. The fix needs a recheck of the marked part after the payout, and is not in this release. | [ADR 0005](adr/0005-live-pool-wide-cap-on-real-near.md) |
| Aggregators and routers cannot buy for users. | The caller holds the tokens. | [OVERVIEW](OVERVIEW.md) |
| Plain unregister differs from NEP-145. | It refuses with `FC_UNREGISTER_WINDOW_OPEN` while the window is open and with `FC_UNREGISTER_WITH_BALANCE` while tokens are held. | [INTERFACE](INTERFACE.md) |
| Borrowed NEAR can fake pool depth for the creator fee. | The fake costs a 24 hour hold with price risk plus about 1.4% to 1.7% of the minimum in fees. | [ADR 0010](adr/0010-creator-fee-share-after-24h-depth.md) |
| A deleted creator account can be created again, and the new owner claims the creator balance. | The optional value D is off. | [ADR 0010](adr/0010-creator-fee-share-after-24h-depth.md) |

<!-- waits on: the marked-token cap rule (limits that depend on the real NEAR cap, the floor and small bags) -->

## Replay and simulation

[analysis/FINDINGS.md](../analysis/FINDINGS.md) holds the NEAR launch replay and the 60-day synthetic simulation, with method and rerun commands. These are simulation, not demand. A Solana-proxy number is a proxy for how sellers behave. It does not show that NEAR users want these rules.

<!-- waits on: the SMDD replay section -->
