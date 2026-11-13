# Large holders cannot send, and early supply shares one allowance

Status: Accepted; may change after the sandbox skeleton and testnet

## Context

A per-account sell limit fails if a holder can split its bag across fresh accounts, because each fresh account gets its own allowance. Fresh accounts on NEAR cost little. A creator can also split at launch by buying from many accounts in the first blocks.

## Decision

- Every unmarked account has the same allowance per window, sized by locked pool depth.
- Early supply is marked. Early supply is tokens bought or received by the creator group, or bought by any account in blocks 0 to N. The creator group is the creator account, every account whose name ends with `.` plus the creator's name, and every parent account of the creator up to but not including the top-level account. A plain string suffix test is wrong: `malice.near` is not in the group of `alice.near`.
- All early supply shares one allowance per window. Each account may use a pro-rata share: the shared allowance times its marked balance just before its first touch in the window (buy, send, receive or sell; in window 0, its first touch after block N), divided by all marked supply at window start.
- A send moves marked tokens first. A sell fills the marked part first, up to the account's pro-rata share, then the unmarked part, up to the account's own allowance. The mark follows the tokens.
- The first touch (buy, send, receive or sell) of an account in a window records its balance. Each marked token counts at most once per window.
- Class uses the live balance at each action. A large account cannot send tokens. An account stays large until its balance is zero.
- Accounts below the large threshold send freely, except that a send that would take a receiver that is not large to the large threshold or more is refused. Only a buy can make an account large.
- `ft_transfer_call` is refused for every receiver. A plain `ft_transfer` to the coin account is refused. Sells use only the sell method. A send to yourself is refused, as in NEP-141. Outside liquidity is refused.
- Unregister (NEP-145 `storage_unregister`), with or without force, is refused while an account holds tokens. At zero tokens it is refused while the account has used allowance or a real NEAR cap record in the current 1 h window, so a new record cannot give a fresh allowance or a clean cap net in that window. It is allowed again when the window ends, so the storage deposit is never locked.

Claim: no large holder, creator-group account or early-supply account can sell into our pool faster than its depth allows, and no transfer by a large holder, and no transfer of early supply, gives a fresh allowance.

## Consequences

- Splitting at launch gains nothing. Sending from a large account is impossible.
- Known limits: account handover; late splitters (accounts that buy after block N and stay small); a holder between one allowance and the large threshold can send parts to fresh accounts; claims on marked tokens held by a wrapper contract can trade outside our pool; an account that was once large cannot send while it holds tokens.
- A buyer can become large by accident with one big buy. The frontend must warn before that buy.
