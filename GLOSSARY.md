# Glossary

**Early supply**: tokens bought or received by the creator group, or bought by any account in blocks 0 to N of the coin. Also called marked tokens.
_Avoid_: sniper supply, insider supply.

**Marked balance / unmarked balance**: the early-supply part and the other part of one account's tokens. A send moves marked tokens first. A sell fills the marked part first, up to the account's pro-rata share, then the unmarked part, up to the account's own allowance.

**Creator group**: the creator account, every account whose name ends with `.` plus the creator's name, and every parent account up to but not including the top-level account. Its tokens are always marked. Wallets the creator funds are not in the group.

**Allowance**: the most tokens an account, or the early supply as one group, may sell into the pool in one window.

**Pro-rata share**: the part of the early-supply allowance one account may use: the shared allowance times its marked balance just before its first touch in the window (buy, send, receive or sell; in window 0, its first touch after block N), divided by all marked supply at window start.

**Window**: the time period for one allowance. Every window sits on one fixed grid that starts at the coin's launch, so all accounts and the real NEAR cap reset at the same moment. Unmarked accounts use 1 h windows and early supply uses 24 h windows.

**Touch**: any buy, send, receive or sell by an account. The first touch in a window records the balance for that window.

**Large account**: an account whose live balance, marked and unmarked, is 2% of total supply or more. Class uses the live balance at each action, with the class floor per window (see Band account). It cannot send tokens. It stays large until its balance is zero. Only a buy can make an account large: a send that would take a receiver that is not large to 2% or more is refused.

**Band account**: an account that is not large and whose balance, marked and unmarked, is between one allowance and the large threshold. An account that was large stays large. The band rules apply only to the unmarked part; marked tokens always follow the early-supply share. Public docs and the design call it a mid-size account; the contract and the interface say band (`class` = `band`). Its first allowance in each window always fills. It may sell above that and pay a penalty while the real NEAR cap has room. In each 1 h window, an account's class is never below its class just before its first action (buy, sell, send or receive) in that window. A buy or a receive can raise it; a sell or a send cannot drop it until the window ends. So a band account that sells or sends below one allowance stays band for the rest of the window, and an account that was large and sells to zero is band for the rest of the window.

**First allowance**: the part of a band account's sells in one window, up to one allowance, that always fills. Of the tokens it held from before the window, that part never counts toward the real NEAR cap; tokens bought in the window always count.
_Avoid_: small account (a small account's bag fits inside one allowance).

**Early window**: blocks 0 to N of a coin. A buy in the early window is early supply. Marked tokens cannot be sold in the early window. Early-supply shares are first recorded at the first action after block N.

**Receipt**: the event a sell emits, showing what was sold, what was held back and why. A refused send or buy fails and has no receipt.

**Partial fill**: a sell that goes through only up to the allowance. The rest stays in the wallet.

**Real NEAR cap**: the pool-wide limit on NEAR paid out to large, band and marked sells in one window, except a band account's first allowance, as a share c of the real NEAR in the pool. Each sell is checked against c times the real NEAR left after that sell.

**Virtual reserve**: the NEAR amount P that the pool counts at launch but that nobody put in. A NEAR reserve minimum keeps the stored NEAR reserve at P or above.

**NEAR reserve minimum**: the rule that the stored NEAR reserve never falls below P, so no sell pays out NEAR that buyers did not put in.

**Floor**: the minimum sell room per window that every limited sell gets (`floor_bps`, `floor_left`). Each unmarked account has its own floor. Marked accounts share one floor, split pro rata.

**Late splitter**: a person who buys after block N through many small accounts, so no account is large. A known limit.

**Account handover**: a holder gives a buyer control of its whole account, so no token moves. A known limit.
