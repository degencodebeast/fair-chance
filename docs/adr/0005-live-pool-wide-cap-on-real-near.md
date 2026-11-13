# Live pool-wide cap on real NEAR paid out

Status: Accepted; may change after the sandbox skeleton and testnet

## Context

With a virtual NEAR reserve (ADR 0003), one price-sized allowance can pay out a large share of the real NEAR in the pool early on. A per-account cap does not help, because many accounts each at the cap could take all real NEAR in one window. A snapshot of real NEAR at the window start would be zero in the first window and would block every large sell for that window.

## Decision

- One pool-wide cap limits the NEAR paid out to large, band and marked sells in a window to a share c of the real NEAR in the pool.
- The contract reads real NEAR live, from contract state, at each sell, and checks the cap after the sell: the cap used in the window, plus this sell, must be at most c times the real NEAR left after this sell. The real NEAR left subtracts the payout and the creator share of the fee that leaves the pool. The check applies to every large, band and marked sell, also when its own net adds zero. The price-sized allowance keeps the window snapshot.
- The cap counts each account's net NEAR out in the window: its counted sell payouts minus its buys, each buy counted as the full NEAR paid, fee included. Each account keeps one signed running value per window. That value is clamped at zero when it is read, before it is added to the cap total, so one account's buys never offset another account's sells, and each buy offsets once. It is not clamped at each step, and not at a buy. Marked sells count the same way, net of the same account's buys in the window, for the cap only; the marked allowance still counts gross (ADR 0004).
- Which tokens of a sell count: the marked part first (it always counts), then the tokens the account bought in the current window (they always count, for every class, small too), then the tokens it held from before the window, by class. This order is for the cap count only; the penalty-free part, the allowances and the class rules do not follow it (ADR 0004).
- Of the tokens held from before the window, only sells by large and band accounts count toward the cap. Small accounts (a bag inside one allowance) never count them. A small sell of tokens bought in the window counts: it fills as far as it passes the cap check or adds nothing to the cap used, so at a full cap the part above the NEAR the account paid waits for the next window.
- A band account's first allowance in each window always fills and never counts toward the cap. Only the part above counts, and it fills zero when the cap is full; a band sell cannot pay the penalty to pass a full cap. Large and marked sells stop at the floor when the cap is full, until the next window.
- A floor gives every limited sell a minimum room per window. Each unmarked account has its own floor. Marked accounts share one floor, split pro rata. Marked tokens an account sold in the window count as used against its unmarked floor (floor left = floor - unmarked used - marked sold in the 1 h window, at least 0), so each account gets one floor per window.
- The floor is 10 bps (floor_bps) of the window's recorded token reserve, the same snapshot that sizes the allowance, rounded down, in tokens. The coin records the snapshot before the first action of each window runs, buy or sell. Each unmarked account gets one floor per window. The shared marked floor uses the same base, split pro rata. Each account gets its own floor, so many accounts can pass a full cap (known limit: floors above the cap).
- The cap applies at every pool depth. There is no depth above which it turns off.

## Consequences

- Large and marked sells, and band sells above one allowance, together take at most a share c of the real NEAR per window. Band first allowances are outside this limit. Above it, each large account's floor and the one shared marked floor still fill.
- One sell can take at most about c ÷ (1 + c) of real NEAR. A seller's own buy gains it no cap room, because the bought NEAR leaves the pool again on the sell.
- Small sells of tokens held from before the window never add to the cap used. They lower real NEAR, so they lower the cap for limited sellers. Known limit: heavy small selling, or a group of small wallets, can leave no cap room for large, band and marked sellers in that window.
- A round trip by one account, marked or unmarked, adds nothing to the cap total, so it cannot block other sellers.
- A buy raises the cap for the whole pool. The order of sells inside a window matters: the first large seller in a window uses the room first.
- Known limit: A wallet that buys first raises the cap for everyone. One large wallet then exits at the large-holder pace. Many small wallets can exit at once. The large seller then takes more NEAR sooner, and the small wallets pay about that amount in fees and price fall. A seller that wants to exit early may still gain from this. If the large seller owns the small wallets, the price fall comes back to it, so its real cost is the fees, about 2% of the NEAR the wallets put in.
- Known limit: When the cap is full, a large holder that bought in the window and sells the same amount back is held at the floor when the cap is over full by more than c x its buy; otherwise it gets back the part its own buy made room for. The buy opens no room for tokens the account held before. A band account that buys and sells back in the same window counts the bought part toward the cap, net of its buy.
- Turning the cap off above some depth was rejected: with no group cap, many large holders selling one allowance each could take most of the real NEAR in one window while the virtual reserve is still large.
- Known limit: band holders and late buyers who split into small wallets skip the cap, because small sells of tokens held from before the window never count.
- Known limit: the cap nets buys and sells per account. A creator that buys in one creator-group account and sells from another counts the sell in full.
