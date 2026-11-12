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
gross and pro-rata from that snapshot.

For an unmarked wallet, a window records token buys and sells. Its first-band
room is `allowance + buys - sells`, floored at zero. The initial allowance is
always allowed and never reaches the cap; excess is a band sale, pays the
configured 10/20/30% penalty, and is cap-limited. Large and marked sales remain
allowance-limited and cap-counted. The shared cap charges marked gross NEAR
out plus unmarked NEAR out net of unmarked buys, clamped at zero. At a full cap
only the price-floor amount can fill.

The main grid is T=2%, N={5,17,50}, D={off,10%,20%,30%}, penalty={10%,20%,30%},
s={0%,30%}. T=1% and 5% plus 10-minute and 24-hour windows are sensitivity
checks. Test 11 scales marked early supply to 9%, 23%, and 41%, retaining a
0.05 NEAR early buyer at block 3.
