# Glossary

**Early supply**: tokens bought by the creator account, by the creator's sub-accounts, or by any account in the first N blocks of the coin. Also called marked tokens.
_Avoid_: sniper supply, insider supply.

**Marked balance / unmarked balance**: the early-supply part and the other part of one account's tokens. A sell or a send moves marked tokens first.

**Creator group**: the creator account and its sub-accounts, found by a suffix test. Wallets the creator funds are not in the group.

**Allowance**: the most tokens an account, or the early supply as one group, may sell into the pool in one window.

**Pro-rata share**: the part of the early-supply allowance one account may use: the shared allowance times its marked balance, divided by all marked supply.

**Window**: the time period for one allowance. Unmarked accounts and early supply use different window lengths.

**Touch**: any send, receive or sell by an account. The first touch in a window records the balance for that window.

**Large account**: an account at or above the large threshold. It cannot send tokens. It stays large until its balance is zero.

**Band account**: an unmarked account whose balance is between one allowance and the large threshold. It may sell above the allowance and pay a penalty.
_Avoid_: small account (a small account's bag fits inside one allowance).

**Partial fill**: a sell that goes through only up to the allowance. The rest stays in the wallet.

**Real NEAR cap**: the pool-wide limit on NEAR paid out to large, band and marked sells in one window, as a share of the real NEAR in the pool, read live before each sell.

**Virtual reserve**: the NEAR amount P that the pool counts at launch but that nobody put in. A price floor keeps the stored NEAR reserve at P or above.

**Late splitter**: a person who buys after block N through many small accounts, so no account is large. A known limit.

**Account handover**: a holder gives a buyer control of its whole account, so no token moves. A known limit.
