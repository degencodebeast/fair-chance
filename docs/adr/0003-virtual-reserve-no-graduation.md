# Virtual NEAR reserve, no graduation, whole supply in the pool

Status: Accepted; may change after the sandbox skeleton and testnet

## Context

A pool seeded with real NEAR needs someone to seed it, and a small seed makes the early allowance very small. A curve that graduates moves liquidity to an outside pool at a point where creators often sell. Tokens given outside the pool at launch would get around the creator group cap.

## Decision

- At `new`, the coin mints its whole supply into its own pool, against a virtual NEAR reserve P. P is one factory constant for every coin.
- Nobody seeds real NEAR. A NEAR reserve minimum keeps the stored NEAR reserve at P or above, so no sell pays out NEAR that buyers did not put in.
- There is no graduation and no migration. The liquidity stays locked in the coin forever.
- Swap fees go to locked liquidity. Changed by ADR 0010: a creator share of each fee builds up in a separate balance.
- The creator group (the creator account, its sub-accounts and its parent accounts) may hold at most a fixed share of supply at any time. The contract checks this at each buy and at each send into the creator group.

## Consequences

- Every allowance is sized by a pool that exists from block 1.
- Early sells can take a large share of the real NEAR in the pool. ADR 0005 limits this.
- Wallets that the creator funds but did not create as sub-accounts are outside the creator group and its cap. Known limit.
- The trade-off for P: a small P gives snipers cheap supply; a large P makes the demo more expensive.
