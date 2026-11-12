# Every sell needs exactly 1 yoctoNEAR attached

Status: accepted (draft, builder review pending)

## Context

On NEAR, a function-call access key (for example an old app login) can call contract methods but cannot attach a deposit. NEP-141 and NEP-145 use a 1 yoctoNEAR deposit on sensitive methods for this reason.

## Decision

The sell method requires exactly 1 yoctoNEAR attached, like `ft_transfer`.

## Consequences

- A leaked function-call key cannot sell a holder's tokens.
- Every sell opens a wallet confirmation. Nothing sells in the background, and the explain agent cannot sell.
- A sell cannot be queued to run later. Each step needs a new signature. A frontend button may repeat partial fills.
- If a full-access key leaks, the thief can also add and delete keys. The holder must act first.
