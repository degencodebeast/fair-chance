# Hosted page check

A read-only check of a coin page that is already hosted. It holds no key, no wallet and no sandbox. It opens the given URL in headless Chrome and reads what a visitor sees.

## Sub-features

- `hosted-load` the page finishes loading: `#status` says how many sell receipts it read, not `Values cannot load`.
- `hosted-banner` the early banner (`#early-banner`) shows.
- `hosted-receipts` the receipt list (`#receipts`) renders; the report counts the rows.
- `hosted-stats` four stats with a value.
- `hosted-account-checks` all three `#chain-checks li` have `data-status="pass"` and `#chain-warning` is absent.
- `hosted-console` no console error and no uncaught error from the page's own host.

## How to get to it (user POV)

- A visitor opens the hosted coin link, for example `https://<host>/?coin=<coin>&network=testnet`.

## Driving it with the walking-path runner

Preconditions:

- `verify.sh doctor` passes and finds Chrome.

- **Command:** `.claude/skills/verify/verify.sh hosted "<url>"`, or `FC_HOSTED_URL=<url> .claude/skills/verify/verify.sh hosted`. The URL must be https (http only on 127.0.0.1). Only the host is printed.
- **Proof:** `verify.log` ends in `verify: PASS (hosted) ...`.
- **Self-test:** the default run builds the page, serves it on 127.0.0.1 with a fake chain, runs `verify.sh hosted` on it (must pass) and again with a chain that fails every read (must fail): `node web/scripts/check-hosted-page-on-local-build.ts`.

## Gotchas

- Never write the URL into a file. The command takes it from the command line or the environment only.
- A 403 or 429 answer stops the check at once with "Stop and report; do not retry". Do not retry it.
- The check needs the hosted page to know its factory and code hash (for a `network=testnet` link they come from `known-deployments.ts`). While those are null the account checks show "not checked" and this check fails by design.
