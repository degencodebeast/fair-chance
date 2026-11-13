# Screens (landing, coin list, alerts, launch form, coin page)

Each screen of the page loads at 1440 and 390 px wide with no console error and shows a main element with a heading. The coin page also draws the price chart and the holders table. Buttons that would send a transaction are turned off with a visible reason.

## Sub-features

- `screen-landing` is the page with no link parameters. Main element: `#landing-headline`.
- `screen-coin-list` is `?view=coins`. Main element: `.coin-card`. Sample coins carry a "Sample data" label.
- `screen-alerts` is `?view=alerts`. Main element: `.chat-bubble`.
- `screen-launch-form` is `?view=launch`. Main element: `#launch-form`.
- `screen-coin-page` is `?coin=<coin>&...`. Main elements: `#trade-box`, `#creator-fees`, `#holders`.
- `screen-turned-off` is the rule that every send-a-transaction button is disabled with a reason element and that every enabled control has a `data-handler`.

## How to get to it (user POV)

- Open the page with the link parameters above. The landing and coin list links in the page header lead to the other screens.

## Driving it with the walking-path runner

Preconditions:

- `verify.sh doctor` passes and finds Chrome.

- **All screen checks alone, no sandbox:** `.claude/skills/verify/verify.sh screens`. It runs the five browser scripts below and ends with `verify: PASS (screens)`.
- **Load check:** `node web/scripts/check-screens-load-in-browser.ts`. Each screen at 1440 and 390: main element and heading shown, no console error (also none up to 2 s after the network is quiet).
- **Layout and labels:** `node web/scripts/check-screens-in-browser.ts` (sample labels, dead controls, width), `check-page-frames-in-browser.ts` (loading and error frames), `check-testnet-link-in-browser.ts` (a testnet link reads only the fixed hosts).
- **Turned-off controls:** `node web/scripts/check-turned-off-screens-in-browser.ts`.
- **Proof:** the full run (`verify.sh`) runs the same scripts, then the walking path. A screen with its main element removed makes the load check fail with `<screen> at <width>: ...`.

## Gotchas

- The chain is a stub on 127.0.0.1 ports 3030 and 3031 (Playwright routes). Every other host is blocked, so fonts do not load and the check ignores that error.
- Self-test of the load check: `FC_LOAD_CHECK_LATE_ERROR=1 node web/scripts/check-screens-load-in-browser.ts` must fail.
- A new screen needs an entry in the `screens` list of the load check and in this file.
