# Fair Chance verification map

This folder is the maintained source for verifying what a Fair Chance user can see and do. Read this index before driving the app, then use the matching feature file.

## Baseline preconditions

- Run from a checkout of this repo with `NEAR_SANDBOX_BIN_PATH` set to near-sandbox 2.13.4.
- `.claude/skills/verify/verify.sh doctor` prints `verify: PASS (doctor)`.
- Every run starts its own sandbox on a free port with a new data folder. Never drive a sandbox that this run did not start.
- The sandbox genesis holds `factory.sandbox`, `creator.sandbox` and `alice.sandbox`. The runner launches the coin `walk.factory.sandbox`.

## Driving conventions

- The harness is the walking-path runner (`scripts/walking-path.sh`) plus the browser script (`web/scripts/read-receipt-in-browser.ts`). Use the commands in each feature file as written.
- Prefer the page's stable handles: `#coin`, `#status`, `#receipts li[data-receipt-id]`.
- Read the chain through public RPC methods and contract views only.

## Proof and skip reporting

- Proof is `target/verify-evidence/<run>/verify.log` ending in `verify: PASS (sandbox)`, plus `receipts-page.png` for page features.
- Record the feature file and entry point used with every proof.
- Report an entry point that this run does not drive as not verified. Do not count it as verified through a different path.

## Features

| Feature | File | Proved by the default run |
| --- | --- | --- |
| Launch, buy and limited sell of a coin | [coin-launch-buy-sell.md](coin-launch-buy-sell.md) | yes |
| Coin page (receipts, stats, rules, account checks) | [receipts-page.md](receipts-page.md) | yes (headless Chrome) |
| Sell alert from the watcher | [sell-alert.md](sell-alert.md) | yes (fake sender) |

## Feature entry contract

Each feature file starts with an H1 title and one paragraph. It then has four H2 sections in this order: `Sub-features`, `How to get to it (user POV)`, `Driving it with the walking-path runner`, `Gotchas`.
