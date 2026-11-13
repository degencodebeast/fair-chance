# NEAR launch replay

Status: receipt-verified where a table's status says so; otherwise `UNKNOWN`.

## Fixed selection rule

At `1794496145054` Unix milliseconds (2026-11-12T15:09:05.054Z), request
the Nearly launch list and the JustHoot room list. Select the first ten Nearly
records and first five JustHoot records in each API's descending creation order
that are at least 24 hours old. There is no outcome, price, volume, holder, or
trade-count filter. This yields a 15-coin, two-launchpad cohort with complete
1-hour and 24-hour observation windows.

## Data and boundaries

The runner reads public Nearly and JustHoot endpoints, FastNEAR's public
transaction index, and a public archival RPC. Its
committed `fixtures/` directory contains the exact public responses used for
the replay; a clean checkout therefore produces the same tables without
depending on an evolving launch list. The local `.cache/` is only for an
explicit fresh collection and remains git-ignored. It stops on an unknown
HTTP failure and retries only HTTP 429, with bounded backoff.

The launch point for every measure is the signed launch transaction. Its signer
is the creator; token-account parents and launchpad display fields are not used
for that purpose. Nearly supplies a DEX receipt ID, so a receipt action supplies
a swap owner only when its signer proves it. JustHoot's public trade feed has no
receipt ID, so its `trader` field is never used as an owner. Any sell without a proven owner
makes all measure-2 group shares `UNKNOWN`; it is never folded into
`everyone_else`. Creator and early-buyer amounts in measure 2 are explicitly
named `net_pool_trades`, not token-balance claims, because transfer discovery
is incomplete. The creator group includes the signer, its subaccounts, and
parents below the top-level account; received amounts are `UNKNOWN` until
transfer receipts are indexed, never silently omitted.

Amounts remain integers until CSV rendering. Token decimals and total supply
are read from the token's public archival-RPC `ft_metadata` and
`ft_total_supply` views (the checked coin reports 18 decimals).

## Run

```sh
python3 analysis/near-replay/replay.py \
  --as-of-ms 1794496145054 \
  --only cosmicfrog.nearlytrade.near
```

The committed coin list is the fixed cohort. The runner pages trade feeds and
fails closed if their reported count disagrees. `--only` writes a separate
`results/single/<token>/` set, preserving the committed full-cohort tables.
