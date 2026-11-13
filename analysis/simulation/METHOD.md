# Sell-rule simulation

This is a deterministic synthetic exercise, not a prediction. It models a
1B-token constant-product pool with a 100 NEAR virtual reserve, no graduation,
and a 60-day seed-7 activity path. The checkpoint is `--checkpoint`: tests 1
and 4 at N=17, T=2%, brake off, for creator normal-fee shares s=0% and 30%.

Every buy and sell has a 1% normal fee. Creator s receives that portion of the
normal fee in a separate balance; it is outside real NEAR and the cap. The 10%
band penalty and the early-buy fee above 1% remain entirely in the pool.

Early buys are marked. A signed creator-group model marks group buys and
receipts, rejects a group balance over 4% of supply, and refuses marked sells
through block N. On the first action after N in each fixed 24-hour window the
model snapshots marked balances and total early supply. Marked allowance is
gross and pro-rata from that snapshot. Marked cap usage is net of the same
account's buys. Its shared 0.1% floor uses the same 24-hour snapshot and
subtracts the account's net tokens sold (tokens sold minus tokens bought,
floored at zero).

Each account records its class immediately before its first action in a 1-hour
window. Its class can rise on a buy or receive, but cannot fall below that
recorded class during the window. A band wallet stays band after selling below
one allowance, so later sales above its remaining first allowance still pay
the band penalty. A large wallet stays large until its balance reaches zero.

Unmarked sell room nets tokens bought against tokens sold. For a band wallet,
only `allowance - tokens already sold` is uncounted; bought-back tokens are
counted as above-allowance sales. The initial allowance portion always fills
and never reaches the cap; the above-allowance part pays the configured
10/20/30% penalty and is cap-limited. Large and marked sales remain
allowance-limited and cap-counted. For every account, cap usage is its limited
NEAR out net of its own buys, clamped at zero. Small sells and the band
account's first allowance do not count. The account cap record is updated in
action order: buys first consume outstanding uncounted small/band proceeds,
then reduce the signed net; uncounted sells consume unused buy credit before
they become outstanding proceeds; counted sells increase the signed net.
Marked and unmarked token-sold totals use separate counters even when their
window indices coincide. The cap window matches the configured
unmarked window: the 10-minute check also uses a 10-minute cap window, and the
24-hour check uses a 24-hour cap window. A limited fill is the larger of the
cap-permitted amount and the account's remaining floor, capped by the request.
Floor remaining subtracts net tokens sold in its window.

The early buy fee is `1% + 49% × (18 − b) / 18` for launch-relative blocks
`b=0..17`, then 1% from block 18 onward. Allowance is
`1 / sqrt(1 − 0.02) − 1`, evaluated with binary floating point.

The main grid is T=2%, N={5,17,50}, D={off,10%,20%,30%}, penalty={10%,20%,30%},
s={0%,30%}. T=1% and 5% plus 10-minute and 24-hour windows are sensitivity
checks. Test 11 scales marked early supply to 9%, 23%, and 41%, retaining a
0.05 NEAR early buyer at block 3.

`results/cap-cases.json` contains 1.0.0-rc.5 contract-comparison vectors. Each
vector records its pool, holder balances, marked balances, large flags, rule
settings in the `fc_rules` units, ordered actions, fills, cap use, and engine
limit reasons. It includes buys for all launch-relative blocks 0 through 17,
a 2%-holder threshold crossing, a band sale after a same-window buy, a band
class-floor case, marked allowance limiting, full-cap floor behavior, and
post-sell-cap and same-account round-trip cases for creator fee shares 0% and
30%.

The simulation is derived from the published rules. Contract code was consulted
only to settle these specific implementation details: class is large while its
large flag remains set, otherwise band only when its balance is strictly above
the recorded allowance; class floors are recorded before an action and raised
after it; a buy offsets an account's uncounted sell proceeds before counted
proceeds; and the cap check uses real NEAR after the actual net payout, including
the penalty retained in the pool. The model implements those details separately
in Python. It does not copy contract code or integer rounding: pool math uses
binary floats and 100 bisection iterations. One rule-text/code disagreement
remains: the band vector follows the code by counting tokens sold from a
same-window buy toward the cap, while ADR 0005 currently says the first
allowance never counts. The docs owner has identified this as a wording issue;
the vector preserves the implemented behavior for contract comparison.

The model uses binary floating point with 100 bisection iterations; values are
rounded to the nearest base unit only when serialized. Run
`python3 analysis/simulation/charts.py`
to regenerate the SVG summaries for tests 1, 4, and 10. The configured brake
does not trip in the baseline synthetic path; the unit test separately forces
a price drop across the threshold to verify its behavior.
