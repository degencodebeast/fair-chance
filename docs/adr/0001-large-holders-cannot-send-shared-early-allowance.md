# Large holders cannot send, and early supply shares one allowance

Status: accepted (draft, builder review pending)

## Context

A per-account sell limit fails if a holder can split its bag across fresh accounts, because each fresh account gets its own allowance. Fresh accounts on NEAR cost little. A creator can also split at launch by buying from many accounts in the first blocks.

## Decision

- Every unmarked account has the same allowance per window, sized by locked pool depth.
- Early supply is marked. Early supply is tokens bought by the creator account, by its sub-accounts (suffix test), or by any account in the first N blocks.
- All early supply shares one allowance per window. Each account may use a pro-rata share: the shared allowance times its marked balance, divided by all marked supply.
- A sell or a send moves marked tokens first. The mark follows the tokens.
- The first touch (send, receive or sell) of an account in a window records its balance. Each marked token counts at most once per window.
- A large account cannot send tokens. An account stays large until its balance is zero.
- Accounts below the large threshold send freely.
- `ft_transfer_call` is refused unless the receiver is our pool. Outside liquidity is refused.
- Force-unregister is refused while an account holds tokens or has used allowance.

Claim: no large holder or insider can sell into our pool faster than its depth allows, and no transfer by a large holder, and no transfer of early supply, gives a fresh allowance.

## Consequences

- Splitting at launch gains nothing. Sending from a large account is impossible.
- Known limits: account handover; late splitters (accounts that buy after block N and stay small); a holder between one allowance and the large threshold can send parts to fresh accounts; claims on marked tokens held by a wrapper contract can trade outside our pool; an account that was once large cannot send while it holds tokens.
- A buyer can become large by accident with one big buy. The frontend must warn before that buy.
