# Glossary

**Early supply**: tokens bought or received by the creator group, or bought by any account in blocks 0 to N of the coin. Also called marked tokens.
_Avoid_: sniper supply, insider supply.

**Marked balance / unmarked balance**: the early-supply part and the other part of one account's tokens. A sell or a send moves marked tokens first.

**Creator group**: the creator account, every account below it, and every parent account up to but not including the top-level account. Its tokens are always marked. Wallets the creator funds are not in the group.

**Allowance**: the most tokens an account, or the early supply as one group, may sell into the pool in one window.

**Pro-rata share**: the part of the early-supply allowance one account may use: the shared allowance times its marked balance, divided by all marked supply.

**Window**: the time period for one allowance. Every window sits on one fixed grid that starts at the coin's launch, so all accounts and the real NEAR cap reset at the same moment. Unmarked accounts use 1 h windows and early supply uses 24 h windows.

**Touch**: any send, receive or sell by an account. The first touch in a window records the balance for that window.

**Large account**: an account whose balance, marked and unmarked, is 2% of total supply or more. It cannot send tokens. It stays large until its balance is zero.

**Band account**: an unmarked account whose balance is between one allowance and the large threshold. Public docs and the design call it a mid-size account; the contract and the interface say band (`class` = `band`). Its first allowance in each window always fills. It may sell above that and pay a penalty while the real NEAR cap has room.

**First allowance**: the part of a band account's sells in one window, up to one allowance, that always fills and never counts toward the real NEAR cap.
_Avoid_: small account (a small account's bag fits inside one allowance).

**Early window**: blocks 0 to N of a coin. A buy in the early window is early supply. Marked tokens cannot be sold in the early window. Early-supply shares are first recorded at the first action after block N.

**Receipt**: the event a sell emits, showing what was sold, what was held back and why. A refused send or buy fails and has no receipt.

**Partial fill**: a sell that goes through only up to the allowance. The rest stays in the wallet.

**Real NEAR cap**: the pool-wide limit on NEAR paid out to large, band and marked sells in one window, as a share of the real NEAR in the pool, read live before each sell.

**Virtual reserve**: the NEAR amount P that the pool counts at launch but that nobody put in. A price floor keeps the stored NEAR reserve at P or above.

**Late splitter**: a person who buys after block N through many small accounts, so no account is large. A known limit.

**Account handover**: a holder gives a buyer control of its whole account, so no token moves. A known limit.
