# Token and pool in one contract

Status: Accepted; may change after the sandbox skeleton and testnet

## Context

A sell limit must see every sell and every send. If the token and the pool are separate contracts, the token cannot tell a sell from a send, and the token cannot refuse deposits into other pools. Existing NEAR launch curves keep the token, the DEX and the pool in separate contracts, and they have no sell limit.

## Decision

Each coin is one contract that holds both the NEP-141 token ledger and its own constant-product pool. We copy no code from other launchpads. We credit prior curve designs in the README.

## Consequences

- The contract sees every buy, sell and send, and can apply the rules in ADR 0001.
- The coin trades only in its own pool. `ft_transfer_call` is refused for every receiver, so deposits into outside pools and NEAR Intents are refused.
- We write and test the pool math ourselves.
