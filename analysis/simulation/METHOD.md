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
gross and pro-rata from that snapshot. Marked sell payouts count gross toward
the cap and are never offset by buys. The cap separately nets each account's
other counted sell payouts against its buys; each buy is counted once, and only
the resulting per-account net is clamped at zero. The shared 0.1% marked floor
uses the 24-hour marked snapshot.

Each account records its class immediately before its first action in a 1-hour
window. Its class can rise on a buy or receive, but cannot fall below that
recorded class during the window. A band wallet stays band after selling below
one allowance, so later sales above its remaining first allowance still pay
the band penalty. A large wallet stays large until its balance reaches zero.

Unmarked sell room nets tokens bought against tokens sold. For cap accounting,
a sell consumes marked tokens first, then tokens bought in the current window,
then tokens held from before it. Current-window purchases count toward the cap
for every class, including small holders. A small holder's sale of older tokens
does not count. For a band wallet, only `allowance - tokens already sold` is
penalty-free; the allowance portion fills and the rest pays the configured
10/20/30% penalty and is cap-limited. Large and marked sales remain
allowance-limited. For every account, cap usage is gross marked NEAR out plus
the maximum of zero and its other counted sell payouts minus buys. Uncounted
sells do not add to the cap record or consume buy credit.
Marked and unmarked token-sold totals use separate counters even when their
window indices coincide. The cap window matches the configured unmarked
window: the 10-minute check also uses a 10-minute cap window, and the 24-hour
check uses a 24-hour cap window.

Each account has one floor in a 1-hour window. The unmarked floor subtracts
every token sold in the window, marked or unmarked, once; buys never restore
floor room. The marked floor subtracts marked tokens sold in its 24-hour
allowance window and unmarked tokens sold in the current 1-hour window. The
available floor for the account is the larger of the unmarked floor and the
marked floor left at its first sell of the window. A marked part held by the cap cannot fill through the unmarked floor (ADR
0005). When a large sell has `marked.floor_left == 0` and its unmarked part fills
exactly the remaining unmarked floor before the cap stops the rest, its limit
reason is `floor`.

`cap_left` is the most NEAR out the counted part of a sell with no band penalty
can take now. It is `(c * real_near_now - cap_used) * (1 - f) / ((1 + c) * (1 - f) + c * s * f)`, rounded down, where `c`, `f`, and `s` are the real cap, swap fee, and creator fee share. Uncounted parts do not use `cap_left`; per-account fills are checked separately. The configured creator fee share is applied to pool actions and leaves the pool.

Each sell is priced as one swap on the total filled token amount. The model
uses that single quote for its fee, creator share, penalty and cap check; it
attributes marked, bought and uncounted portions within the same swap output.
A mixed marked/unmarked sell in blocks 0 to N is refused as a whole with
`FC_EARLY_WINDOW`.

The early buy fee is `1% + 49% × (18 − b) / 18` for launch-relative blocks
`b=0..17`, then 1% from block 18 onward. Allowance is
`1 / sqrt(1 − 0.02) − 1`, evaluated with binary floating point.

The main grid is T=2%, N={5,17,50}, D={off,10%,20%,30%}, penalty={10%,20%,30%},
s={0%,30%}. T=1% and 5% plus 10-minute and 24-hour windows are sensitivity
checks. Test 11 scales marked early supply to 9%, 23%, and 41%, retaining a
0.05 NEAR early buyer at block 3.

`results/cap-cases.json` contains the 1.0.0-rc.7 contract-comparison vectors.
It includes both one-floor sell orders, a marked sell held at the cap with the
unmarked floor tested separately, `cap_left` before and after sells at s=30%,
and a band sell stopped below the full `cap_left` amount. The file records the creator fee share. Pool state uses binary floats; sell
fills and limit comparisons use integer token base units. Buy outputs are
floored from a high-precision decimal quote; gross output, fees and penalties
are quantized to yoctoNEAR. `cap_left` is calculated from binary-float pool and
cap state, then serialized by rounding down.

The simulation is derived from the published rules. Contract code was consulted
only to settle implementation details: class is large while its large flag
remains set, class floors are recorded before an action and raised after it,
a marked sell in blocks 0 to N refuses the whole sell, each sell is one swap,
and the cap check uses real NEAR after the actual net payout, including the
penalty retained in the pool. The rc.7 `cap_left` formula was implemented from
the interface text and checked against the pool's post-sell equation. These
details are implemented independently in Python. After the sell-amount fixes,
the remaining differences reported by the reviewer are numeric only: the
`cap_left_yocto` field can differ by up to 1e9 yoctoNEAR because pool reserves
and cap records remain binary floats. The affected cases are
`band-sell-above-allowance`, `post-sell-cap-s-0`, `post-sell-cap-s-30`,
`band-round-trip-credit`, `small-buy-price-rise-cap`, and
`marked-and-unmarked-old-plus-buy-resell`; the reviewer accepts this tolerance.
rc.6 resolved the earlier band allowance wording difference.

The model stores reserve state as binary floats and uses high-precision decimal
arithmetic when converting swap quotes to base units. It searches limited
fills in integer token base units; NEAR values are rounded at the documented
yoctoNEAR boundaries.

Run
`python3 analysis/simulation/charts.py`
to regenerate the SVG summaries for tests 1, 4, and 10. The configured brake
does not trip in the baseline synthetic path; the unit test separately forces
a price drop across the threshold to verify its behavior.
