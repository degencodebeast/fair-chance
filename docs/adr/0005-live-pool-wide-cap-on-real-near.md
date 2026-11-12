# Live pool-wide cap on real NEAR paid out

Status: accepted (draft, builder review pending)

## Context

With a virtual NEAR reserve (ADR 0003), one price-sized allowance can pay out a large share of the real NEAR in the pool early on. A per-account cap does not help, because many accounts each at the cap could take all real NEAR in one window. A snapshot of real NEAR at the window start would be zero in the first window and would block every large sell for that window.

## Decision

- One pool-wide cap limits the NEAR paid out to large, band and marked sells in a window to a share c of the real NEAR in the pool.
- The contract reads real NEAR live, from contract state, before each sell. The price-sized allowance keeps the window snapshot.
- For unmarked accounts, the cap counts each account's net NEAR out in the window (sell payouts minus its buys), clamped at zero. For marked tokens, it counts gross payouts.
- Only sells by large, band and marked accounts count toward the cap. Sells by small accounts (a bag inside one allowance) always pass and never count.
- When the cap is full, large, band and marked sells stop at the floor until the next window. A band sell cannot pay the penalty to pass a full cap.
- A floor gives every limited sell a minimum room per window. Each unmarked account has its own floor. Marked accounts share one floor, split pro rata.
- The cap applies at every pool depth. There is no depth above which it turns off.

## Consequences

- Large, band and marked holders together take at most a share c of the real NEAR per window, at every depth. Large holders who sell in the same window cannot add up past it.
- A busy hour of small trading never fills the cap.
- An unmarked round trip adds nothing to the cap total, so it cannot block other sellers.
- A buy raises the cap for the whole pool. The order of sells inside a window matters: the first large seller in a window uses the room first.
- Turning the cap off above some depth was rejected: with no group cap, many large holders selling one allowance each could take most of the real NEAR in one window while the virtual reserve is still large.
- Known limit: band holders and late buyers who split into small wallets skip the cap, because sells by small accounts never count.
- Known limit: every creator-group buy is marked, so a creator round trip counts gross and fills part of the cap. Its marked sells stay inside its share of the shared early-supply allowance, so the effect is small.
