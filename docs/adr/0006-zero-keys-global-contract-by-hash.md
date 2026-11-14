# Zero access keys, global contract by hash, one batch, no factory trust

Status: Accepted

## Context

Rules that an owner can change are the pattern that scanners flag as a scam. On NEAR, an account with a full-access key can deploy new code. A global contract used by account ID can change when its owner deploys again. A global contract used by code hash cannot change.

## Decision

- The launchpad is a factory. Anyone may launch. The creator is the account that signs `launch`. Every coin gets the same fixed values.
- `launch` runs one batch transaction on the new coin account: CreateAccount, Transfer (storage), UseGlobalContract by code hash, FunctionCall `new`. There is no AddKey, so the coin account never has a key.
- The coin uses the global contract by code hash only, never by account ID.
- No coin method gives the factory any special right. The only factory call is the one-time `new`.
- The factory account's keys are deleted on the last day, after the final end-to-end test.

Tests:
- Each coin's code hash equals the factory's recorded hash.
- Every coin method other than `new`, called as the factory, gives the same result as for any account.
- A second `new` fails.
- A coin account never exists without its state.

## Consequences

- The README may claim only three lines: the coin account has no access keys; its code is a global contract used by a stated hash; the contract has no owner, pause, setter or upgrade method.
- A claim about the code hash needs a reproducible build linked to the source.
- After the factory keys are deleted, a factory bug needs a new factory, and a coin bug needs a new coin.
