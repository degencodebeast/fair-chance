# Coin page

The coin page reads one coin from the chain: stats row, who can sell how fast, receipts, the rules with the code hash, the early banner, the coin account checks, and loading and error states. It needs the page URL: `?coin=<coin>&rpc=<url-encoded RPC URL>`. Optional: `&factory=<factory account>&code_hash=<known coin code hash>` (the known deployment for the account checks), `&receipt=<receipt id>` (marks that receipt "From your alert"), `&fastnear=<base URL>` (event source; a testnet RPC URL uses `https://tx.test.fastnear.com` by default, any other URL uses the block scan).

## Sub-features

- `receipts-load` reads `fc_rules.launch_block`, then the events from there to the final head, and lists one row per `fc_sell` (also `fc_cap_filled` and `fc_became_large` rows, which are not sell receipts).
- `receipts-row` is an `li` with `data-receipt-id` and `data-block`. Its sentence is `.receipt-sentence`: `<seller> sold <amount> of <amount> tokens for <near> NEAR. Fill: <fill>.` The verify run reports it as `Block <n>: <sentence>`.
- `page-stats` shows price, market cap, real NEAR and age from `fc_pool`; a view the coin lacks shows "Not available from this coin".
- `page-account-checks` shows three checks (`#chain-checks li[data-check]`): direct sub-account of the known factory, global contract hash equal to the known hash, no access keys. Any failure or missing known value shows `#chain-warning`.
- `page-error` shows `Values cannot load: <reason>` in `#status` and no number when any read fails.

## How to get to it (user POV)

- Open `web/index.html` (built by Vite) with the query above.
- For manual work: `scripts/walking-path.sh --hold`, then `cd web && npx vite` and open `http://localhost:5173/?coin=<FC_COIN>&rpc=<url-encoded FC_RPC_URL>`.

## Driving it with the walking-path runner

Preconditions:

- `verify.sh doctor` passes and finds Chrome.

- **Command-line read.** The runner runs `node web/scripts/read-receipts.ts "<page URL>"`. The log shows `walking-path:   page row: "<row text>"`.
- **Browser read.** The runner runs `node web/scripts/read-receipt-in-browser.ts "?coin=..&rpc=..&factory=..&code_hash=.." <receipt id> <evidence>/receipts-page.png`. The log shows `walking-path:   browser ("<Chrome version>") row: "Block <n>: <row text>"`, then the stats, the code hash and the three account checks (all `pass`).
- **Proof.** `receipts-page.png` shows the coin page with the stats row, the card "Who can sell how fast", the receipt row and the rules.

## Gotchas

- The sell must be final before the scan; the runner waits for that (`the sell is final at block <n>`).
- The page calls the sandbox RPC from another origin. neard allows any origin today; a CORS error appears as `Values cannot load` plus a console error in the failure message.
- The sandbox keeps blocks because `World::launch` raises `gc_num_epochs_to_keep`. A scan from an old block on a normal node can fail with "the node no longer has the blocks".
- Some coin views are stubs that return zeros until the rule work lands (for example `fc_cap`). The page shows what the coin returns.
- The page loads fonts from Google Fonts; offline it falls back to system fonts.
- Assert on the handles (`#coin`, `#status`, `#stats`, `#receipts li[data-receipt-id]`, `#code-hash`, `#chain-checks`), not on layout.
