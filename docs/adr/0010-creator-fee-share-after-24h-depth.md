# Creator fee share, claimable after 24 hours of pool depth

Status: Accepted; may change after the sandbox skeleton and testnet

## Context

Most launchpads pay creators a fee share. With 100% of swap fees in the pool (ADR 0003), a creator who buys no supply gains nothing from launching here, and the sell limits never touch that creator. Fees paid on every trade also reward many low-effort launches.

## Decision

- A fixed creator share of the normal swap fee, on buys and sells, builds up in a separate creator balance. The rest stays in locked liquidity.
- The 10% mid-size penalty and the extra early-window buy fee (ADR 0008) go 100% to the pool. The creator earns no part of the penalty or of the extra early-window fee.
- The creator balance is not part of the pool's real NEAR. It never changes the pool-wide cap (ADR 0005), and a claim never lowers it.
- Only the creator account can claim. A claim pays out only if the lowest real NEAR in every 1 h window of the last 24 hours was at or above a minimum. The contract keeps the lowest real NEAR of each window, updated at every action. A new window starts that value at the real NEAR carried from the last action, and a window with no actions keeps that carried value. The lowest value is read after each action. A claim tests the last 24 full windows and the current window up to the claim. The claim does not change real NEAR. Window 0 starts its lowest value at 0 real NEAR. The contract keeps `depth_ok_since`: set by the first window whose lowest value is at or above the minimum, and cleared by any window below it. A claim needs at least 24 h since `depth_ok_since`. One window with a lowest value below the minimum restarts the 24 hours. Otherwise the claim is refused with a fixed error code. If the payout transfer fails, the contract restores the balance.
- An unclaimed balance stays outside the pool. An optional fixed value D moves it to the pool.
- The share and the minimum are not final. They are fixed facts of every coin, not launch settings.
- There is no holders' vault and no buyback. Fees and penalties that stay in the pool raise the price every holder can sell at.

## Consequences

- The pool grows more slowly than with 100% of fees in it.
- Wash trading costs the creator less, because part of the fee comes back.
- A coin that never reaches the minimum never pays its creator. A coin with a short dip waits one more day.
- Someone can fake the depth with borrowed NEAR in many small wallets. Because the claim tests the lowest real NEAR of every window, the NEAR must stay in the pool for the full 24 hours, with price risk, plus the fees. The fees cost only about 1.4% to 1.7% of the minimum, so the main cost is holding the NEAR for 24 h with price risk. The spec sizes the minimum. The paper test attacks this case.
- Known limit: if the creator deletes its account, whoever creates that name again can claim the unclaimed balance. If the optional value D is set, it moves an unclaimed balance to the pool, which limits this loss.
