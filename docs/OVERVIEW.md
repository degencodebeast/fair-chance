# Fair Chance: overview

Fair Chance is a memecoin launchpad on NEAR. Every coin it launches has sell limits written into the coin contract. The creator, early buyers and large holders can sell into the pool only as fast as the pool's depth allows, and a large holder cannot split its bag by sending it to other wallets. The known limits below list the wallet routes the rule does not cover.

Read this page first. The decisions and their reasons are in `docs/adr/`. The words are defined in `GLOSSARY.md`. Exact parameter values are in the spec, not here.

## Who it is for

- Retail memecoin traders who buy after launch and hold.
- Honest creators who want buyers to trust their coin.

## The problem

When a new memecoin launches, the creator and a few large wallets often sell into the pool in the first minutes. Later buyers take the loss. Detection tools can warn a buyer, but they cannot stop a sell. Launchpads that act at all usually charge snipers a high fee at the buy, which is paid once. After that, the sniper can sell at any speed.

## What Fair Chance does

1. **One contract per coin, holding both the token and its pool.** The coin sees every buy, sell and send. It trades only in its own pool. Outside pools and deposits are refused (ADR 0002).
2. **A virtual NEAR reserve from block 1.** The whole supply goes into the pool at launch. Nobody seeds real NEAR. There is no graduation, and the liquidity stays locked. Swap fees stay in the pool, except a creator share. The creator can claim that share only after the pool has held a minimum of real NEAR for 24 hours. The share and the minimum are not final (ADR 0003, ADR 0010).
3. **An allowance per window, sized by pool depth.** Every account may sell up to one allowance per window. A deeper pool means a larger allowance. If your bag fits inside one allowance, no limit ever applies to you (ADR 0001).
4. **Early supply shares one allowance.** Tokens bought or received by the creator group (the creator, its sub-accounts and its parent accounts), or bought by anyone in blocks 0 to N, are marked. All marked tokens share one allowance per window, split by balance. Marked tokens cannot be sold in blocks 0 to N. Early-supply windows run on a 24 h grid from launch. Shares are first recorded at the first action after block N. Splitting across wallets at launch gains nothing (ADR 0001, ADR 0008).
5. **Large holders cannot send.** A holder above the large threshold cannot move tokens to fresh wallets to get fresh allowances (ADR 0001).
6. **Selling above the allowance.** Large and marked accounts get a partial fill, and the rest stays in the wallet. Mid-size (band) accounts may sell more and pay a penalty, which stays in the pool (ADR 0004).
7. **A pool-wide cap on real NEAR paid out.** Early on, most of the pool is virtual. A cap, read live before each sell, limits how much real NEAR large, band and marked sells can take in one window (ADR 0005).
8. **A falling buy fee in the first N blocks.** It cuts how much cheap early supply snipers take. It goes to the pool, and no account is exempt (ADR 0008).
9. **Every sell needs a wallet confirmation.** The sell method needs 1 yoctoNEAR attached, so a leaked app login key cannot sell (ADR 0009).
10. **Nobody can change a coin after launch.** The coin account has no access keys. Its code is a global contract used by code hash. It has no owner, pause, setter or upgrade method (ADR 0006).

**The rule slows a dump. It does not stop it.** A limited sell gets a partial fill, and the rest stays in the wallet. The holder can sell again in a later window, with a new signature. Nothing waits in a queue on chain. So an alert can say how much went through now, and how much can follow, and when: "A large holder tried to sell 3% of supply. The rule filled 1%. It can sell up to 1% more after 14:00."

## What a trader can know

- The creator group, the buyers in the first N blocks and every large holder sell only as fast as the pool allows.
- When one of them tries to sell more, holders hear about it, with how much can follow and when.
- Nobody can change the rules of a coin after launch.

Fair Chance does not promise that the price goes up. Insiders who hide behind fresh wallets that buy after the first N blocks are a known limit. The optional off-chain link check can point to such wallets only as context.

## The enforce and explain split

The contract enforces the rules. Everything else explains them: the coin page, events, the watcher with its Telegram bot, and any off-chain link checks. An explain part can never move funds or change a limit. Wallet-link checks by an OutLayer worker, if built, are labeled context only (ADR 0007).

## What it does not stop (known limits)

- Late splitters: people who buy after block N through many small wallets at market price.
- Wallets the creator funds but did not create as sub-accounts. The contract cannot see who funded an account.
- A mid-size (band) holder sending parts of its bag to fresh wallets.
- A bag split into many small wallets (each inside one allowance) sells without limit and never counts toward the cap, at any value of T.
- Many mid-size wallets. Each wallet's first allowance in a window passes without counting toward the pool-wide cap, so a bag split across many mid-size wallets sells one allowance per wallet per window.
- Account handover: a holder gives away the whole account, keys included.
- Two allowances back to back at a window boundary.
- Small early buyers. A buy in blocks 0 to N counts as early supply, whatever its size, so it sells at the one shared pace (about 10 to 69 days to exit at p = 2%, by paper estimate). The early-window banner and the buy preview say this before the buy, and count blocks left, with seconds only as an estimate: "Buys in the next 5 blocks (about 3 seconds) count as early supply".
- Selling by small holders. The rule slows big exits. It does not make the price go up.

Public claims say "most wallets", never "most selling". They do not use "safe", "cannot be rugged", "audited", "first" or "only".

## Parts of the build

| Part | What it is | Who |
| --- | --- | --- |
| Factory contract | Launches coins with one batch transaction, the same fixed values for every coin | Builder |
| Coin contract | Token, pool and every rule above | Builder |
| Frontend | Launch form, coin list, coin page with limits and receipts | Builder |
| Watcher and Telegram bot | Reads coin events, sends plain-language alerts, answers `/watch` and `/check` | Builder |
| Events and view methods | An NEP-297 event for every sell, including limited sells; hard refusals return a fixed error code; a view method for the early window and allowances | Builder (names frozen as 1.0.0-rc.2; 1.0.0 after the paper test) |
| NEAR launch replay | Measures early and insider selling on real NEAR launches | Kalu, issue #1 |
| Rule simulation | Tests the parameters and attacks over 60 days | Kalu, issue #2 |

## Ground rules for this repo

- Testnet only. No mainnet deploy, no mainnet swap.
- No keys or secrets in code, output or commits. Keys go in environment variables.
- Analysis output is labeled "simulation" or "replay". It shows how the rules behave, not that users want them.
- Do not change a rule to make a test pass. Report which test shows the problem.
