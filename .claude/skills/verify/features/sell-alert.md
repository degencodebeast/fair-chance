# Sell alert from the watcher

The Go watcher reads new coin events through its event source and sends one alert sentence per sell to a chat. Verification uses the fake sender, so no real message is sent.

## Sub-features

- `alert-read` reads `fc_sell` from the block scan adapter from the coin's launch block.
- `alert-sentence` turns the event into one sentence that names the seller, the amount and the NEAR paid.
- `alert-send` passes the sentence to the sender once.

## How to get to it (user POV)

- A follower of a coin gets a Telegram message after a sell. Real Telegram sending is not wired in this release.
- Command: `target/watcher -once -want 1 -timeout 60s` with `FC_RPC_URL`, `FC_COINS` and `FC_SENDER=fake`.

## Driving it with the walking-path runner

Preconditions:

- `verify.sh doctor` passes. The runner builds `target/watcher`.

- **Read and send.** The runner starts the watcher after the page steps. The log shows `walking-path:   alert: <seller> (small holder) sold <amount> tokens for <near> NEAR. The whole sell filled.`
- **Proof.** The alert line in `verify.log` names `alice.sandbox`, and the run ends with `verify: PASS (sandbox)`.

## Gotchas

- `FC_SENDER` accepts only `fake`. Never give the watcher a real bot token in a verify run.
- The holder class and fill text come from stub fields until the rule engine adds them.
