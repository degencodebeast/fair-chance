# Snipers are early supply, plus a falling buy fee in the first N blocks

Status: accepted (draft, builder review pending)

## Context

Other launchpads punish snipers only at the buy, with a high fee that falls over the first seconds or minutes. The fee is paid once, so a sniper can still sell at any speed afterwards. A separate sniper class would need its own rules and its own definition.

## Decision

- There is no sniper class. Any account that buys in the first N blocks holds early supply under ADR 0001, with no exemption.
- A buy fee runs for exactly the first N blocks and falls to the normal fee at block N. Its start value is a code parameter. It goes to locked liquidity. No account is exempt, the creator included.
- A view method returns N, the blocks left in the early window, total early supply, the shared allowance, the rate per account, the current buy fee and the block it ends.
- Each early buy emits a NEP-297 event that marks it.
- For Fair Chance coins, the analysis label is "bought in the first N blocks".

## Consequences

- A large sniper share slows every early exit pro rata, the creator's included. It also slows honest early buyers by the same factor.
- The falling fee is prior art. The README must not present it as new.
- Bundlers who buy after block N are unmarked. This is the late-splitter known limit.
