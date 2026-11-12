# Creator fee share, claimable after 24 hours of pool depth

Status: accepted (draft, builder review pending)

## Context

Most launchpads pay creators a fee share. With 100% of swap fees in the pool (ADR 0003), a creator who buys no supply gains nothing from launching here, and the sell limits never touch that creator. Fees paid on every trade also reward many low-effort launches.

## Decision

- A fixed creator share of the normal swap fee, on buys and sells, builds up in a separate creator balance. The rest stays in locked liquidity.
- The 10% mid-size penalty and the extra early-window buy fee (ADR 0008) go 100% to the pool. The creator earns no part of the penalty or of the extra early-window fee.
- The creator balance is not part of the pool's real NEAR. It never changes the pool-wide cap (ADR 0005), and a claim never lowers it.
- Only the creator account can claim. A claim pays out only if the pool's real NEAR was at or above a minimum at every hourly window start for the last 24 hours. One window start below the minimum restarts the 24 hours. Otherwise the claim is refused with a fixed error code. If the payout transfer fails, the contract restores the balance.
- An unclaimed balance stays outside the pool. A later rule may move it to the pool.
- The share and the minimum are not final. They are fixed facts of every coin, not launch settings.
- There is no holders' vault and no buyback. Fees and penalties that stay in the pool raise the price every holder can sell at.

## Consequences

- The pool grows more slowly than with 100% of fees in it.
- Wash trading costs the creator less, because part of the fee comes back.
- A coin that never reaches the minimum never pays its creator. A coin with a short dip waits one more day.
- Someone can fake the depth with borrowed NEAR in many small wallets. It costs a full day of holding with price risk, plus the fees. The minimum must be set so that this costs more than the creator balance. The paper test attacks this case.
