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
subtracts floor tokens already sold by that account in the window.

For an unmarked wallet, a window records token buys and sells. Its first-band
room is `allowance + buys - sells`, floored at zero. The initial allowance is
always allowed and never reaches the cap; excess is a band sale, pays the
configured 10/20/30% penalty, and is cap-limited. Large and marked sales remain
allowance-limited and cap-counted. For every account, cap usage is its limited
NEAR out net of its own buys, clamped at zero. Small sells and the band
account's first allowance do not count. The cap window matches the configured
unmarked window: the 10-minute check also uses a 10-minute cap window, and the
24-hour check uses a 24-hour cap window. At a full cap, each account can use
only its remaining floor.

The early buy fee is `1% + 49% × (18 − b) / 18` for launch-relative blocks
`b=0..17`, then 1% from block 18 onward. Allowance is
`1 / sqrt(1 − 0.02) − 1`, evaluated with binary floating point.

The main grid is T=2%, N={5,17,50}, D={off,10%,20%,30%}, penalty={10%,20%,30%},
s={0%,30%}. T=1% and 5% plus 10-minute and 24-hour windows are sensitivity
checks. Test 11 scales marked early supply to 9%, 23%, and 41%, retaining a
0.05 NEAR early buyer at block 3.

`results/cap-cases.json` contains rc.5 contract-comparison vectors. Each vector
records its pool, holder balances, marked balances, large flags, rule settings,
ordered actions, fills, cap use, and limit reasons. It includes buys for all
launch-relative blocks 0 through 17, a 2%-holder threshold crossing, a band
sale above allowance, marked allowance limiting, and full-cap floor behavior.
The model uses binary floating point with 100 bisection iterations; values are
rounded to the nearest base unit only when serialized. Run
`python3 analysis/simulation/charts.py`
to regenerate the SVG summaries for tests 1, 4, and 10. The configured brake
does not trip in the baseline synthetic path; the unit test separately forces
a price drop across the threshold to verify its behavior.
