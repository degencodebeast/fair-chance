# Live pool-wide cap on real NEAR paid out

Status: accepted (draft, builder review pending)

## Context

With a virtual NEAR reserve (ADR 0003), one price-sized allowance can pay out a large share of the real NEAR in the pool early on. A per-account cap does not help, because many accounts each at the cap could take all real NEAR in one window. A snapshot of real NEAR at the window start would be zero in the first window and would block every large sell for that window.

## Decision

- One pool-wide cap limits the NEAR paid out to large, band and marked sells in a window to a share c of the real NEAR in the pool.
- The contract reads real NEAR live, from contract state, before each sell. The price-sized allowance keeps the window snapshot.
- For unmarked accounts, the cap counts each account's net NEAR out in the window (sell payouts minus its buys), clamped at zero. For marked tokens, it counts gross payouts.
- A floor gives every limited sell a minimum room per window.
- Sells that fit inside one allowance always pass, and they count toward the cap.

## Consequences

- A round trip adds nothing to the cap total, so it cannot block other sellers.
- A buy raises the cap for the whole pool. The order of sells inside a window matters.
- Known limit: band holders and late buyers who split into small wallets skip the cap. So the cap protects early real NEAR only against holders who do not split.
