# NEAR launch replay

Status: receipt-verified where a table's status says so; otherwise `UNKNOWN`.

## Fixed selection rule

At `1794496145054` Unix milliseconds (2026-11-12T15:09:05.054Z), request
`https://nearly.trade/api/launches?sort=new&limit=60&offset=0`. Select the
first ten records in its descending `created_at_ms` order that are at least
24 hours old. There is no outcome, price, volume, holder, or trade-count
filter. This permits complete 1-hour and 24-hour observation windows.

Issue #1 says the JustHoot link is "to follow". No public endpoint was
provided, so its portion of the intended 10–20 launch universe is `UNKNOWN`;
this checkpoint is deliberately a ten-coin Nearly-only cohort, not an
unannounced substitute for a cross-platform sample.

## Data and boundaries

The script reads only Nearly's public, unauthenticated launch and trade
endpoints and caches the raw JSON under `.cache/` (git-ignored). It stops on
an unknown HTTP failure and retries only HTTP 429, with a bounded backoff.

Its first observed pool trade is a **proxy**, not the launch receipt. Its
measure-1 figures are observed pool buys, not account balances at block N.
Its measure-2 figures are observed per-account pool sells less buys, floored
at zero; token transfers are not yet reconciled. This is necessary because
the sample already shows a launch-side `lock_8.nearlytrade.near` account whose
relationship to the stated creator is not proved by the public trade feed.
All affected cells say `UNVERIFIED_PROXY`; the final replay must instead use
the receipt method in `DATA_SOURCES.md`.

Amounts remain integers until CSV rendering. Token decimals and total supply
are read from the token's public archival-RPC `ft_metadata` and
`ft_total_supply` views (the checked coin reports 18 decimals).

## Run

```sh
python3 analysis/near-replay/replay.py \
  --as-of-ms 1794496145054 \
  --only cosmicfrog.nearlytrade.near
```

The committed coin list is the fixed cohort. The runner renders all five
required CSV tables and fails closed when the launchpad feed truncates trades.
