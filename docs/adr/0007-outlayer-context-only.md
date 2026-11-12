# OutLayer worker output is context only

Status: Accepted; may change after the sandbox skeleton and testnet

## Context

A worker can look up links between accounts (creator, first funder, sub-accounts) through an indexer. A call waits for the answer through yield/resume, which can take about a minute and can time out. A sell attaches exactly 1 yoctoNEAR, so it cannot pay for a worker call. A mark found during a sell would count only from the next window. A false link would mark an innocent account. The worker's results rest on trust in its operator account, not on a check per call.

## Decision

- The link check runs outside the coin contract. A frontend or a separate small contract asks the worker and shows the answer.
- No coin method calls the worker or reads its answer. Buys and sells never wait.
- The answer is a class plus compact link evidence (funder account, transaction hash, block height), within the resume payload limit.
- The answer never changes a limit. It is shown with the label "answered by an OutLayer worker".

## Consequences

- On-chain rules always apply alone. A timeout or no worker changes nothing in the coin.
- A contract on NEAR cannot see who funded an account, so funder links never mark tokens. Wallets that the creator funds and that buy after block N stay unmarked. Known limit.
- A new indexer or a new worker provider needs no change to the coin.
