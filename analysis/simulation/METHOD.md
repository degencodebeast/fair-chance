# Sell-rule simulation

Every output row is labeled `simulation`. It is deterministic (PRNG seed 7)
and models a 1B-token constant-product pool with a 100 NEAR virtual reserve,
a 1% swap fee, no graduation, and no initial real NEAR.

The starting population is synthetic because the replay cannot yet establish
transfer-complete historical balances: creator (4 NEAR early buy), five early
buyers (3 NEAR each), three later large buyers (35 NEAR), eight later band
buyers (4 NEAR), and twelve small random traders. The model applies the
ticket's allowance rate, early 24-hour window, live 5% real-NEAR cap, 0.1%
floor, partial fills, and configured band penalty/drop brake. The otherwise
unspecified early fee is a stated simulation assumption: 10% at block zero,
falling linearly to 1% at block N. It makes no claim about real demand or user
behavior.

The pool-wide cap uses the configured unmarked-account window as its one
shared cap clock; marked supply keeps its separate 24-hour allowance clock.

Run `python3 analysis/simulation/simulate.py`. The full grid is 108 base
settings (N × T × brake × penalty), plus the required 10-minute and 24-hour
window checks at N=17, T=2%, brake off, penalty 10%.
