# Sniper scene: two wallets, same buy (local sandbox)

Local NEAR sandbox only (near-sandbox 2.13.4, fresh chain, throwaway keys). Not testnet.

Command: `scripts/sniper-scene.sh`, run on commit 3c3f9fb (the rule code is the same as on the integration branch).
Result: exit 0, every check passed. The scene prints these lines under "One early wallet vs one late wallet".

- Two accounts make the same buy on a second coin with the same opening buy. The early buy lands in block 10 after launch. The late buy lands in block 23 after launch, after the early window (blocks 0 to 17).
- The early buy gets about 17.7% fewer tokens for the same NEAR paid, because the early buy fee is still falling in block 10.
- Both then sell their whole bag in the same 1 hour window as their buy.
- Early buy: sold 16.3% of its bag. Limit reason `allowance`, read from the chain.
- Late buy: sold 100.0% of its bag. No limit, read from the chain.

The condition: the late sell is in the same 1 hour window as its buy, so it fills in full. A late buy that sells in a later hour is limited by its own allowance.
