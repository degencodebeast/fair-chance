# Receipts page

The receipts page shows a coin's sell receipts, read from the chain by a block scan from the coin's launch block. It needs only the page URL: `?coin=<coin>&rpc=<url-encoded RPC URL>`.

## Sub-features

- `receipts-load` reads `fc_rules.launch_block`, scans blocks to the final head and lists one row per `fc_sell`.
- `receipts-row` shows `Block <n>: <seller> sold <amount> of <amount> tokens for <near> NEAR. Fill: <fill>.`
- `receipts-error` shows `Values cannot load: <reason>` in `#status` when the chain read fails.

## How to get to it (user POV)

- Open `web/index.html` (built by Vite) with `?coin=<coin>&rpc=<url-encoded RPC URL>`.
- For manual work: `scripts/walking-path.sh --hold`, then `cd web && npx vite` and open `http://localhost:5173/?coin=<FC_COIN>&rpc=<url-encoded FC_RPC_URL>`.

## Driving it with the walking-path runner

Preconditions:

- `verify.sh doctor` passes and finds Chrome.

- **Command-line read.** The runner runs `node web/scripts/read-receipts.ts "<page URL>"`. The log shows `walking-path:   page row: "<row text>"`.
- **Browser read.** The runner runs `node web/scripts/read-receipt-in-browser.ts "?coin=..&rpc=.." <receipt id> <evidence>/receipts-page.png`. The log shows `walking-path:   browser ("<Chrome version>") row: "Block <n>: <row text>"`.
- **Proof.** `receipts-page.png` shows the coin name in `#coin`, `1 sell receipts from block <n>, read from the chain.` in `#status` and the row.

## Gotchas

- The sell must be final before the scan; the runner waits for that (`the sell is final at block <n>`).
- The page calls the sandbox RPC from another origin. neard allows any origin today; a CORS error appears as `Values cannot load` plus a console error in the failure message.
- The sandbox keeps blocks because `World::launch` raises `gc_num_epochs_to_keep`. A scan from an old block on a normal node can fail with "the node no longer has the blocks".
- The page has no design styling yet. Assert on the handles, not on layout.
