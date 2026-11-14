# Evidence

This page lists what each test proves and what the tests leave open. It is reference, not a promise. Numbers from a replay or a simulation are simulation, not demand.

Run every test with `cargo test --workspace --locked`. Sandbox tests need `NEAR_SANDBOX_BIN_PATH` and the wasm from `scripts/build-contracts.sh`. The merge gate is `.claude/skills/verify/verify.sh`.

Last full run: 13 November 2026 (UTC), Rust 1.97.1, near-sandbox 2.13.4: `cargo test --workspace --locked --no-fail-fast` gave 263 passed, 0 failed, 2 ignored. The 2 ignored tests build the coin in Docker and run with `scripts/build-coin-reproducible.sh`.

## Rule tests (`rule-engine/tests/`)

| File | What it shows |
| --- | --- |
| `pool_buy_and_sell.rs` | Constant-product buy and sell, the NEAR reserve minimum, slippage refusal. |
| `unmarked_allowance.rs` | One allowance per account per 1 hour window, counted net in tokens, with no carry-over. |
| `holder_class_and_sends.rs` | Large accounts cannot send. A send that would make a receiver large is refused. Sends to the coin and to yourself are refused. Unregister is refused while tokens are held. |
| `window_class_floor.rs` | A band account that sells below one allowance stays band until the window ends. A large account that sells to zero is band until the window ends. An account that was large and holds a bag inside one allowance is still limited as large: at a full cap it fills only its floor. The floor resets in the next window. |
| `partial_fill_and_band_penalty.rs` | A large sell above the allowance fills in part and the rest stays in the wallet. A band sell inside one allowance pays no penalty. A band sell above it pays 10% on the part above. |
| `real_near_cap.rs` | The cap rule, case by case: a large sell fills up to 5% of the real NEAR left, checked after the sell. A seller's own buy gains it no cap room. Marked sells count gross. A small sell of tokens bought in the window counts toward the cap. Floors fill at a full cap. Wallet-splitting and buy-first routes are measured, with their bounds. |
| `real_near_cap_properties.rs` | Random sequences of buys, sells, sends, creator claims and window moves across many accounts. After every step: the cap holds for the parts it limits, no sell pays out NEAR that buyers did not put in, the supply never changes, no account takes two floors in one window, and the quote equals the real sell. The case counts are 10,000, 1,000 and 1,000, set in the file. Failing cases are kept in `rule-engine/proptest-regressions/`. |
| `marked_tokens_and_early_window.rs` | A buy in blocks 0 to 17 is early supply. The creator group and its sub-accounts share one limit and a 4% holding cap. Early supply shares one allowance per 24 hours and cannot sell in the early blocks. |
| `creator_fee_claim.rs` | The creator share of the swap fee is paid only after the pool holds its minimum real NEAR for 24 hours. A failed payout restores the balance. |
| `launch_and_rules.rs` | `new` mints the whole supply once, into the pool. The parent account alone can call `new`. `fc_rules` returns every fixed value. |
| `walking_path.rs` | Launch, buy, sell and receipt in the rule engine alone. |
| `sell_floor_cap_and_limit_reason.rs` | One floor per account per 1 hour window: a buy gives no floor room back, and marked and unmarked floors do not stack, in either order. The marked part of a small sell stays within the cap. A large sell that fills only its unmarked floor reports `floor`. A small sell with a marked part and tokens bought in the window may fill less of the bought part when the cap is tight, and a second sell does not avoid it, because a sell takes its marked part first. |

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

## How we checked it

Each row names one way we checked the rules, what it shows, what it leaves open, and the command that runs it.

| Method | What it shows | What it does not show | Command |
| --- | --- | --- | --- |
| Rule tests | Each rule in `rule-engine/` does what its ADR says, one case at a time. | Cases nobody wrote. Behavior on a real chain. | `cargo test --locked -p fair-chance-rule-engine` |
| Property tests | Random sequences of buys, sells and sends by several accounts hold the cap, floor and allowance rules. The case counts are fixed in the test file (10,000 for the main property, 1,000 for each of two more). A failing case is saved in `proptest-regressions/` and runs again every time. | Every sequence. A rule nobody wrote as a property. | `cargo test --locked -p fair-chance-rule-engine --test real_near_cap_properties` |
| Sandbox tests | The compiled coin and factory run on a local NEAR sandbox chain: launch, buy, sell, receipts, refusals, no access keys, no owner, pause, setter or upgrade method. | Testnet or mainnet behavior. Wallet behavior. | `scripts/build-contracts.sh` then `cargo test --locked -p fair-chance-sandbox-tests` |
| The merge gate | One command runs the tools check, the tests, the sandbox walking path, the page in headless Chrome and the private-string scan. A change merges only when it passes. | A hosted page or a real wallet. | `.claude/skills/verify/verify.sh` |
| TLA+ model | A small model of one sell, with its rules written as invariants, was checked by TLC over every reachable state within its bounds. The model run found a gap between the model and the docs. We fixed the model and ran it again. | Sizes beyond the bounds. The real code. Prices, fees and sends. The bounds and the full list of limits are in [docs/model/README.md](model/README.md). | See [docs/model/README.md](model/README.md) |
| Reproducible build | The coin wasm built in a fixed container from the committed source has one SHA-256 hash. That hash is the code hash the chain stores for the global contract. A reader can build it again and compare. | That the source has no bug. | `scripts/build-coin-reproducible.sh` (needs Docker). Compare with the hash the chain returns, as in the README "Check it yourself". |
| Separate Python model of the cap rules | A model of the cap rules in Python, written apart from the Rust code, compared with it case by case. | Not in this release: it is not merged yet. See "Not in this release" in the README. | None |
| Testnet runs | Three recorded runs of coins on NEAR testnet, kept as `evidence/testnet-run.json`, `evidence/testnet-cabal-run.json` and `evidence/testnet-cabal2-run.json`: partial fills, 10 of 10 early sells refused with `FC_EARLY_WINDOW` in both cabal runs, and a later buyer that sold in full. Each README transaction link names its claim, and the link check reads the receipt and compares. | A forecast. Real demand. Amounts in the cabal files are rounded as the run log printed them. | `python3 -I scripts/check_readme_links.py --testnet` (reads the public testnet RPC, no key) |
| Adversarial review | Two models from different families tried to break each change to the cap rules. A finding counted only with a failing test or a step sequence that reproduces it. The test was committed before the fix. | That no finding is left. Review by people outside the project. | The tests committed with each fix are in `rule-engine/tests/`. |

Every command above runs from the repository root.

## Known limits

Each limit has an owner decision in an ADR. The tests above show the rule works. They do not show these routes closed.

| Limit | Why it stays | Source |
| --- | --- | --- |
| A fresh account gets a fresh allowance. | Small sells of tokens held from before the window never count toward the cap. Each fresh small wallet carries at most one allowance per window, and the gain per wallet is measured: up to 0.31 of one allowance's NEAR at a full cap. | [ADR 0005](adr/0005-live-pool-wide-cap-on-real-near.md) |
| Floors above the cap. | Each account has its own floor, so many accounts can each fill their floor at a full cap. One floor is 10 bps of the window's token reserve, once per account per hour. | [ADR 0005](adr/0005-live-pool-wide-cap-on-real-near.md) |
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

## Terminal checks of the BAG coin

Recorded read-only calls on the public testnet RPC, 14 November 2026, for `bag.factory.fairchance.testnet`. The commands are in the README section "Check it from a terminal". A reply from the chain is shown as it came, with `...` where it is cut.

1. Access keys: `{"jsonrpc":"2.0","result":{"block_hash":"81zskvnA1j4dWWEEHWfs4RWxqysYDL33DsTf5cFEkiCE","block_height":277644204,"keys":[]},"id":1}`. The coin has no access keys.
2. The chain's code hash: `{"jsonrpc":"2.0","result":{"amount":"7265684998033816547910958","block_hash":"zg7fLcpH5TYisgxCyD8btZMJa7dFRTxdRh2qciafyMR","block_height":277644215,"code_hash":"11111111111111111111111111111111","global_contract_hash":"6Ld5DFTJ1BYc6uzb6YYvdK3yAMHdfBsvrwNEhsKE9er3","locked":"0","storage_paid_at":0,"storage_usage":6486},"id":1}`. The coin uses a global contract by hash, so `code_hash` holds the empty value and `global_contract_hash` holds the hash.
3. The coin's rules (`result.result` decoded): `{"launch_block":277565791,"p_bps":200,"window_unmarked_s":3600,"window_early_s":86400,"early_blocks":17,"large_threshold_bps":200,"creator_cap_bps":400,"real_cap_c_bps":500,"floor_bps":10,"penalty_bps":1000,"virtual_near":"100000000000000000000000000","swap_fee_bps":100,"early_fee_start_bps":5000,"brake_drop_bps":null,"creator_fee_share_bps":3000,"claim_min_real_near":"10000000000000000000000000","claim_depth_hold_s":86400,"unclaimed_to_pool_s":null,"code_hash":"6Ld5DFTJ1BYc6uzb6YYvdK3yAMHdfBsvrwNEhsKE9er3", ...}`.
4. The source metadata (decoded): `{"version":"1.0.0-rc.7","link":"https://github.com/degencodebeast/fair-chance/tree/b42fddecd94f75904636f912c4ce0dccb7e437d9","standards":[...],"build_info":{"build_environment":"sourcescan/cargo-near:0.22.0-rust-1.97.1@sha256:7467038bdddc86484b73b416eeadce926ff59013e128e53dec5a19e1cb4b2234","contract_path":"contracts/coin","source_code_snapshot":"git+https://github.com/degencodebeast/fair-chance?rev=b42fddecd94f75904636f912c4ce0dccb7e437d9", ...}}`. It names the commit and the pinned build image.

## Replay and simulation

[analysis/FINDINGS.md](../analysis/FINDINGS.md) holds the NEAR launch replay and the 60-day synthetic simulation, with method and rerun commands. These are simulation, not demand. A Solana-proxy number is a proxy for how sellers behave. It does not show that NEAR users want these rules.

## SMDD replay

[analysis/smdd-replay/METHOD.md](../analysis/smdd-replay/METHOD.md) holds the method. The replay plays the first hour of SMDD trades, a Solana coin, in order on one Fair Chance pool, once with the sell limits off and once with them on, in the same coin code. Results are in [analysis/smdd-replay/results/](../analysis/smdd-replay/results/) and in the README "Measured results".

- What it shows: how much of the tokens sold and of the NEAR paid out each group (creator, wallets funded by one funder, large buyers, others) takes, with and without the rules.
- What it does not show: demand, or how NEAR traders would act. The trade file is not in this repository, so a reader checks the replay code and the results and cannot rerun the trade file. The two checks that need the file skip when it is absent.
- Test command: `python3 -m unittest discover -s analysis/smdd-replay/tests` (141 tests, 2 skipped without the trade file, in the last run).
