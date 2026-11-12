# Partial fill for large and marked sells, penalty for band sells, one window per class

Status: Accepted; may change after the sandbox skeleton and testnet

## Context

A sell above the allowance can be refused, partly filled, queued or allowed with a penalty. A refusal or a long wait traps ordinary traders, and many ordinary traders sell more than one allowance in one window. A queue cannot run later, because every sell needs a new signature (ADR 0009).

## Decision

- The contract records the token reserve at the first action in each window, before that action takes effect, and counts what each account sold in that window. It uses no last price and no average.
- Unmarked accounts use a short window. Early supply uses a long window. Both are code parameters.
- Unused allowance does not carry over.
- Large accounts and marked accounts get a partial fill up to the allowance. The rest stays in the wallet.
- Band accounts (unmarked, between one allowance and the large threshold) may sell above the allowance. The part above pays a flat penalty on its NEAR output. The penalty goes to locked liquidity.
- A band account's first allowance in each window always fills and never counts toward the real NEAR cap (ADR 0005). Only the part above counts, and it fills zero when the cap is full. Large and marked sells stop at the floor when the cap is full.
- Unmarked sells count net, in tokens: tokens sold minus tokens bought in the window, at least zero. A buy then a sell uses no allowance; a sell first must fit the allowance when it happens. Marked sells count gross.
- A drop brake exists as a parameter, off by default.

## Consequences

- An ordinary trader waits at most one short window, or pays the penalty.
- Early supply never gets the penalty path.
- Known limit: two allowances back to back at a window boundary.
- Any public claim says "most wallets", never "most selling", because most sell volume is above one allowance.
