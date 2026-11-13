# SMDD replay: method

The replay plays the first hour of SMDD trades (a Solana coin, launched on pump.fun on
18 November 2024) in order on one Fair Chance pool. It plays them twice: once with the sell limits off
and once with the limits on. It writes one chart (price at the end of each minute) and one table
(how much each group sold). It plays every row of the trade file.

## Input

One trade file (CSV). It is not in this repository. The environment variable
`FC_SMDD_TRADES_FILE` names it. The replay plays every row.

Columns: `seq,seconds_from_launch,slot,wallet,role,side,sol_lamports,token_base_units,signature,venue_sol_reserve_before`.
Roles: `creator`, `same-funder-N` with N from one to five (wallets funded by one funder before
launch), the file's 1% group (wallets whose buys in the file total 1% of supply or more) and
`other`. The replay prices every row with the Fair Chance pool math. `venue_sol_reserve_before`
is the SOL depth of the trade's own venue just before the trade. It sizes buys only; no output
shows it or which venue a trade used.

## How the replay sizes a trade

- A buy takes the same share of the Fair Chance pool's NEAR depth (virtual plus real NEAR, just
  before the buy, in that run) as it took of its own venue's SOL depth just before the trade.
  One function: `near_for_buy` in `limits_off_replay.py`; the in-process runner uses the same
  floor.
- A sell sells the same share of the account's replay balance as the file's sell took of the
  wallet's balance in the file. A full exit stays a full exit, also after a partial fill.
- Blocks: one block every 0.5789 s. This is NEAR mainnet, measured over more than 150 thousand blocks from the
  block heights and timestamps in the committed transaction files of
  `analysis/near-replay/fixtures`. It is an interim value until a testnet measure replaces it
  (one constant in `limits_off_replay.py`).

## Limits off: no Fair Chance rule

`limits_off_pool.py` is the rule engine's pool math in Python, with the same integer rounding:
constant product on virtual plus real NEAR, the 1% swap fee and the NEAR reserve minimum. The
limits-off run has no Fair Chance rule at all: no early fee and no sell limit.

Check: with no rule, the creator should sell about the share of supply the trade file shows
(7.331%). The command stops and writes nothing when the replay misses it by more than 10% of that
value. The tolerance was set after the first run, once that run showed the gap. The gap comes
from the curve shape: the pump.fun curve counts more virtual tokens than the Fair Chance supply.
The swap fee is not a cause, because pump.fun charged the same fee. The tolerance is a margin
over the curve shape, not a value fixed before the run. The command prints the shares and the
gap on each run, and the README section gives the two shares in one line.

## Limits on: all rules together (early fee, allowance, class and cap)

`limits_on_replay.py` plays the same trades against the same coin code, in the near-sdk
in-process runner, with exact blocks and times
(`contracts/coin/examples/replay-trades-in-process.rs`). Every rule applies. Each class,
held-back fill and refusal in the table comes from the rule engine's events. A refused trade
changes nothing. Both runs read the creator fee share s from the coin's `fc_rules`. The
difference between the two runs comes from all the rules together, not from one of them.

With no limit triggered and the early fee on in both, the script's pool and the coin give the
same tokens, NEAR and price path (`tests/test_limits_on_replay.py`). One more test plays the
same trades on a fresh sandbox, after the early window, where a real chain's block does not
change a fee, and checks the same equality there (`SandboxEquivalenceTest`; it starts a
sandbox, so it runs only when the verify gate turns on `FC_REPLAY_ON_SANDBOX`).

## Privacy

- Groups only. No wallet, account name, signature or slot leaves `trade_file.py`. No output
  shows a venue depth or which venue a trade used.
- Minutes from launch only. No clock time. The chart has a point for every minute of the hour:
  a minute with no trade keeps the price before it.
- Group totals only: the creator, "funded directly by the same wallet before launch", "bought 1% of
  supply or more in the first hour" and other, and the chart. No per-wallet row, no top list and
  no single-trade example.
- Small groups: a group other than the creator shows a cell only when it has at least two
  wallets and the share comes from at least two sells (a sell that filled none does not count).
  Otherwise the cell says "Not shown", and a line under the table says why. A run's shares add
  up to 100%, so when a column holds exactly one hidden cell (the creator's too), the smallest
  other group cell in that column (never the creator's) is hidden too; no hidden cell comes back
  by subtraction.
  The same-funder line is not written when one of the group's sell cells is hidden. The same-funder line
  below needs two wallets and two buys. The creator is a named role, so the small-group rule does not apply
  to it.
  The least size is one constant, `MIN_GROUP_SIZE` in `replay_report.py`.
- The same-funder check sees direct funding only, so wallets funded through other wallets can
  sit in "other". The group's sell cells show the run's true value in both runs. No text says
  the group sold. When the group sold nothing in either run, one line gives its wallet count and
  its share of all NEAR paid in for buys in the limits-off run; when it sold, that line is not
  written.
- Sells only, as shares of each run's own totals: tokens sold as a share of all tokens sold in
  that run, and NEAR paid out as a share of all NEAR paid out in that run (one total per run,
  never shown). No NEAR amount, buy total or trade count is shown, except the creator's
  limits-on fill counts when that form is used and the same-funder line above. The chart has words only.
- A share that is not zero never shows as zero. When it rounds to zero, it shows "below" the
  smallest step of its precision. Only a true zero shows "0%".
- The file's 1% group is labelled "bought 1% of supply or more in the first hour". The word
  "large" is a contract class and appears only as a class from the rule engine.
- Formats: the table shows one decimal. The creator's limits-on cells show two decimals, then
  three; then one sentence with the creator's limits-on payout as a share of its limits-off
  payout; then the creator's sells as fill counts only; last, the cells are not shown, with a
  line that says why. The summary and the README repeat those cells in the same form. The
  command takes the first form whose outputs pass the guard.
- `published_figures_guard.py` checks every published output (table, chart text, summary,
  README section) before anything is written, and every caption and the video script. First it
  normalizes the text: unicode forms and digits, dashes, case. It fails on an absolute clock time
  in any form, a long code (an address, a signature or a hash, also when split), a raw count (five or more
  digits), a venue word in
  any spelling, the size of a trade (an amount with a unit or a currency sign), prose about one
  trader with a time point or a share, and anything per wallet (a table row that is not a group,
  an account name, a numbered wallet, a ranking or a single-trade example). In the chart it
  checks all text, comments and attribute values except the drawing attributes. A finding names
  the file, the line and the column, never the text.
- The guard checks this file too. Here it may name the method's venues (the pump.fun curve, the
  pool, the venue depth), because this file describes the method; every other rule holds.
- The guard is also a command:
  `python3 -I analysis/smdd-replay/published_figures_guard.py <file>...`.
- The command refuses an output folder inside a Git repository. Output enters this repository
  only after a person checks it.

## Run

```sh
python3 -I analysis/smdd-replay/replay.py --out <folder outside the repository>
scripts/test-trade-replay.sh
```

The tests use `fixtures/synthetic-trades-fixture.csv`. It is a made-up fixture. It is never
replay output. Each guard rule has a test that fails on a bad made-up text and passes on a good
one.
