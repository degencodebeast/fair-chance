//! The pool-wide cap on real NEAR (ADR 0005), through the engine interface: `apply` and the
//! views, at a creator fee share of 0.
//!
//! Large sells, band sells above the first allowance and marked sells together take at most
//! c = 5% of the real NEAR left after each sell, per 1 h window. The check runs after the sell:
//! the cap used in the window, with this sell, must be at most c x the real NEAR left. The cap
//! counts each account's net NEAR out (sell payouts minus its buys, each buy at the full NEAR
//! paid), clamped at zero when read. Small sells never count. Floors stay outside the cap.

use std::collections::BTreeMap;

use rule_engine::{
    apply, launch, quote_sell, view_account, view_cap, view_pool, AccountView, Action, Applied, At,
    CapView, CoinState, Event, FcSell, FixedValues, Holder, Holders, LaunchRequest,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const HOUR_S: u64 = 3_600;
/// c = 500 bps.
const C_BPS: u128 = 500;
const BPS: u128 = 10_000;

const LARGE: &str = "large.test";
const SECOND_LARGE: &str = "second-large.test";
const BAND: &str = "band.test";
const SMALL: &str = "small.test";
const OTHER: &str = "other.test";

/// Seconds after launch, as a block time. The block is 18 blocks later than the second, after
/// the early window (blocks 0 to 17 after the launch at block 100), so buys here are unmarked.
fn after(seconds: u64) -> At {
    At {
        block: 100 + 18 + seconds,
        ns: LAUNCH_NS + seconds * 1_000_000_000,
    }
}

/// A moment `seconds` into window 1.
fn window_1(seconds: u64) -> At {
    after(HOUR_S + seconds)
}

/// One coin and its whole ledger, driven only through `apply` and the views.
#[derive(Clone)]
struct Coin {
    state: CoinState,
    ledger: BTreeMap<String, Holder>,
}

impl Coin {
    fn launched() -> Self {
        let state = launch(
            FixedValues::fixed(),
            LaunchRequest {
                coin: "coin.factory.test",
                caller: "factory.test",
                creator: "creator.test",
                code_hash: "hash",
                name: "Test Coin",
                symbol: "TEST",
                icon: None,
                description: None,
            },
            At {
                block: 100,
                ns: LAUNCH_NS,
            },
        )
        .expect("launch")
        .state;
        Self {
            state,
            ledger: BTreeMap::new(),
        }
    }

    fn holders(&self, accounts: &[String]) -> Holders {
        accounts
            .iter()
            .filter_map(|a| self.ledger.get(a).map(|h| (a.clone(), h.clone())))
            .collect()
    }

    fn act(&mut self, action: Action, at: At) -> Applied {
        let applied =
            apply(&self.state, self.holders(&action.accounts()), &action, at).expect("action");
        self.state = applied.state.clone();
        self.ledger.extend(applied.holders.clone());
        applied
    }

    fn buy(&mut self, account: &str, near_in: u128, at: At) {
        self.act(
            Action::Buy {
                account: account.into(),
                near_in,
                min_tokens_out: 0,
            },
            at,
        );
    }

    fn sell_applied(&mut self, account: &str, amount: u128, at: At) -> Applied {
        self.act(
            Action::Sell {
                account: account.into(),
                amount,
                min_near_out: 0,
                attached: 1,
            },
            at,
        )
    }

    fn sell(&mut self, account: &str, amount: u128, at: At) -> FcSell {
        sell_event(&self.sell_applied(account, amount, at))
    }

    fn sell_all(&mut self, account: &str, at: At) -> FcSell {
        self.sell(account, self.balance(account), at)
    }

    fn send(&mut self, from: &str, to: &str, amount: u128, at: At) {
        self.ledger.entry(to.to_string()).or_default();
        self.act(
            Action::Send {
                from: from.into(),
                to: to.into(),
                amount,
                memo: None,
            },
            at,
        );
    }

    fn quote(&self, account: &str, amount: u128, at: At) -> rule_engine::SellQuote {
        quote_sell(
            &self.state,
            self.holders(&[account.to_string()]),
            account,
            amount,
            at,
        )
        .expect("quote")
    }

    fn account(&self, account: &str, at: At) -> AccountView {
        view_account(&self.state, account, self.ledger.get(account), at)
    }

    fn cap(&self, at: At) -> CapView {
        view_cap(&self.state, at)
    }

    fn real_near(&self, at: At) -> u128 {
        view_pool(&self.state, at).real_near
    }

    fn balance(&self, account: &str) -> u128 {
        self.ledger.get(account).map_or(0, |h| h.balance)
    }
}

fn sell_event(applied: &Applied) -> FcSell {
    applied
        .events
        .iter()
        .find_map(|e| match e {
            Event::FcSell(s) => Some(s.clone()),
            _ => None,
        })
        .expect("fc_sell")
}

fn cap_filled_events(applied: &Applied) -> Vec<serde_json::Value> {
    applied
        .events
        .iter()
        .map(|e| e.log_line())
        .filter_map(|line| {
            let event: serde_json::Value =
                serde_json::from_str(line.strip_prefix("EVENT_JSON:")?).ok()?;
            (event["event"] == "fc_cap_filled").then(|| event["data"][0].clone())
        })
        .collect()
}

/// c x real NEAR, the way ADR 0005 states the check: cap used <= c x real NEAR left.
fn within_cap(cap_used: u128, real_near_left: u128) -> bool {
    cap_used * BPS <= C_BPS * real_near_left
}

/// NEAR out (after the 1% swap fee) for `tokens` sold into the pool as it stands, by the
/// constant-product formula: NEAR out rounded down, fee rounded up.
fn payout_for(coin: &Coin, tokens: u128, at: At) -> u128 {
    let pool = view_pool(&coin.state, at);
    let before_fee = (ethnum::U256::from(pool.near_reserve) * ethnum::U256::from(tokens)
        / ethnum::U256::from(pool.token_reserve + tokens))
    .as_u128()
    .min(pool.real_near);
    before_fee - before_fee.div_ceil(100)
}

/// Test 1: R = 10 real NEAR, no buy in the window, one large sell. The fill stops where the cap
/// used is at most c x the real NEAR left, and one more token unit would go above it. Expected
/// fill: about 10 x 0.05 / 1.05 = 0.476 NEAR.
#[test]
fn test_1_a_large_sell_with_no_buy_in_the_window_fills_to_c_of_the_real_near_left() {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 10 * ONE_NEAR, after(10));
    let at = window_1(10);
    assert_eq!(coin.real_near(at), 10 * ONE_NEAR);
    let before = coin.clone();

    let applied = coin.sell_applied(LARGE, coin.balance(LARGE), at);
    let sell = sell_event(&applied);

    assert_eq!(sell.fill, "partial");
    assert_eq!(sell.limit_reason, Some("cap_full"));
    // 10 NEAR x 500 / 10_500, rounded down.
    assert!(sell.near_out <= 476_190_476_190_476_190_476_190);
    assert!(sell.near_out > 476_100_000_000_000_000_000_000);
    let left = coin.real_near(at);
    assert_eq!(left, 10 * ONE_NEAR - sell.near_out);
    assert_eq!(coin.cap(at).cap_used, sell.near_out);
    assert!(within_cap(sell.near_out, left));
    let one_more = payout_for(&before, sell.tokens_sold + 1, at);
    assert!(!within_cap(one_more, 10 * ONE_NEAR - one_more));

    assert_eq!(sell.cap_used, sell.near_out);
    assert_eq!(sell.cap_left, 0);
    let cap = coin.cap(at);
    assert!(cap.is_full);
    assert_eq!(cap.cap_left, 0);
    assert_eq!(cap.fills_this_window, 1);
    assert_eq!(cap.cap_total, left * C_BPS / BPS);
    let filled = cap_filled_events(&applied);
    assert_eq!(filled.len(), 1);
    assert_eq!(filled[0]["by_account"], LARGE);
    assert_eq!(filled[0]["fill_index"], 1);
    assert_eq!(filled[0]["cap_used"], sell.near_out.to_string());
    assert_eq!(filled[0]["real_near"], left.to_string());
}

/// Test 2 (also paper case a): a seller that already holds tokens buys 100 NEAR of tokens in
/// the window, then sells all of them. Its own buy gains it no cap room: its net NEAR out is at
/// most the Test 1 fill plus the NEAR value of one token unit.
#[test]
fn test_2_a_seller_s_own_buy_in_the_window_gains_it_no_cap_room() {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 10 * ONE_NEAR, after(10));
    let mut test_1 = coin.clone();
    let test_1_fill = test_1.sell_all(LARGE, window_1(10)).near_out;

    coin.buy(LARGE, 100 * ONE_NEAR, window_1(10));
    let pool = view_pool(&coin.state, window_1(20));
    let one_token_unit = pool.near_reserve.div_ceil(pool.token_reserve);
    let sell = coin.sell_all(LARGE, window_1(20));

    // Here the allowance (with room back for the tokens bought) stops the sell first; the cap
    // would stop it at the same net.
    assert!(sell.limit_reason.is_some());
    let net_out = sell.near_out.saturating_sub(100 * ONE_NEAR);
    assert!(
        net_out <= test_1_fill + one_token_unit,
        "net {net_out} > fill {test_1_fill} + {one_token_unit}"
    );
    assert_eq!(coin.cap(window_1(20)).cap_used, net_out);
}

/// Window 0: LARGE buys 10 NEAR, SECOND_LARGE 3 NEAR, BAND 1.5 NEAR and SMALL 0.5 NEAR.
/// Window 1: LARGE sells its whole balance, and the cap stops it: the cap is full.
fn coin_with_a_full_cap_in_window_1() -> Coin {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 10 * ONE_NEAR, after(10));
    coin.buy(SECOND_LARGE, 3 * ONE_NEAR, after(11));
    coin.buy(BAND, 1_500 * ONE_NEAR / 1_000, after(12));
    coin.buy(SMALL, ONE_NEAR / 2, after(13));
    let at = window_1(1);
    assert_eq!(coin.account(SECOND_LARGE, at).class, "large");
    assert_eq!(coin.account(BAND, at).class, "band");
    assert_eq!(coin.account(SMALL, at).class, "small");
    let sell = coin.sell_all(LARGE, at);
    assert_eq!(sell.limit_reason, Some("cap_full"));
    assert!(coin.cap(at).is_full);
    coin
}

/// The full cap of [`coin_with_a_full_cap_in_window_1`], then SMALL sells its whole bag. A
/// small sell never counts but lowers the real NEAR, so the cap used is now above c x the real
/// NEAR.
fn coin_with_a_cap_above_full_in_window_1() -> Coin {
    let mut coin = coin_with_a_full_cap_in_window_1();
    coin.sell_all(SMALL, window_1(2));
    let cap = coin.cap(window_1(2));
    assert!(!within_cap(cap.cap_used, cap.real_near_now));
    coin
}

#[test]
fn a_small_sell_fills_in_full_when_the_cap_is_full_and_never_counts() {
    let mut coin = coin_with_a_full_cap_in_window_1();
    let used = coin.cap(window_1(2)).cap_used;
    let sell = coin.sell_all(SMALL, window_1(2));
    assert_eq!(sell.class, "small");
    assert_eq!(sell.fill, "full");
    assert_eq!(sell.limit_reason, None);
    assert_eq!(sell.cap_used, used);
    assert_eq!(coin.cap(window_1(2)).cap_used, used);
}

#[test]
fn when_the_cap_is_full_a_large_sell_fills_its_floor_and_a_band_sell_fills_zero_above_its_first_allowance(
) {
    let mut coin = coin_with_a_cap_above_full_in_window_1();
    let at = window_1(3);

    let floor_left = coin.account(SECOND_LARGE, at).unmarked.floor_left;
    assert!(floor_left > 0);
    assert_eq!(coin.account(SECOND_LARGE, at).max_full_fill_now, floor_left);
    let large = coin.sell_all(SECOND_LARGE, at);
    assert_eq!(large.class, "large");
    assert_eq!(large.tokens_sold, floor_left);
    assert_eq!(large.fill, "partial");
    assert_eq!(large.limit_reason, Some("floor"));
    assert_eq!(coin.account(SECOND_LARGE, at).unmarked.floor_left, 0);
    let after_floor = coin.sell_all(SECOND_LARGE, at);
    assert_eq!(after_floor.tokens_sold, 0);
    assert_eq!(after_floor.fill, "none");
    assert_eq!(after_floor.limit_reason, Some("cap_full"));

    let first_allowance = coin.account(BAND, at).penalty_applies_above.expect("band");
    let band_balance = coin.balance(BAND);
    assert!(band_balance > first_allowance);
    let band = coin.sell_all(BAND, at);
    assert_eq!(band.class, "band");
    assert_eq!(band.tokens_sold, first_allowance);
    assert_eq!(band.fill, "partial");
    assert_eq!(band.limit_reason, Some("cap_full"));
    assert_eq!(band.penalty_near, 0);
}

/// The buy raises the cap by c x X, and the sell back takes that room out again. Here X (0.3
/// NEAR) is below the amount by which SMALL's sell put the cap used above c x real NEAR (about
/// 0.49 NEAR of real NEAR), so even the floor sell leaves the cap above full.
#[test]
fn cap_full_a_large_account_that_buys_x_and_sells_x_in_the_window_is_held_at_the_floor() {
    let mut coin = coin_with_a_cap_above_full_in_window_1();
    let at = window_1(3);
    let held_before = coin.balance(SECOND_LARGE);
    coin.buy(SECOND_LARGE, 3 * ONE_NEAR / 10, at);
    let bought = coin.balance(SECOND_LARGE) - held_before;
    let floor_left = coin.account(SECOND_LARGE, at).unmarked.floor_left;
    assert!(floor_left < bought);

    let sell = coin.sell(SECOND_LARGE, bought, at);

    assert_eq!(sell.tokens_sold, floor_left);
    assert_eq!(sell.limit_reason, Some("floor"));
}

#[test]
fn a_limited_sell_whose_own_net_adds_zero_is_still_checked_against_the_cap() {
    let coin = coin_with_a_cap_above_full_in_window_1();
    let at = window_1(3);
    let mut full = coin.clone();
    let held = full.balance(SECOND_LARGE);
    full.buy(SECOND_LARGE, ONE_NEAR, at);
    // A sell of nine tenths of the tokens it just bought for 1 NEAR: it pays out less than
    // 1 NEAR, so its own net adds zero.
    let tokens = (full.balance(SECOND_LARGE) - held) * 9 / 10;
    let used_before = full.cap(at).cap_used;
    let sell = full.sell(SECOND_LARGE, tokens, at);
    assert_eq!(sell.limit_reason, Some("cap_full"));
    assert!(sell.tokens_sold < tokens);
    assert!(sell.near_out < ONE_NEAR);
    assert_eq!(full.cap(at).cap_used, used_before);

    // With the cap's room back (a new window), the same buy and sell fill in full.
    let mut next_window = coin.clone();
    let at_2 = after(2 * HOUR_S);
    next_window.buy(SECOND_LARGE, ONE_NEAR, at_2);
    assert_eq!(next_window.sell(SECOND_LARGE, tokens, at_2).fill, "full");
}

#[test]
fn one_account_s_buys_never_offset_another_account_s_counted_sells() {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 50 * ONE_NEAR, after(10));
    let at = window_1(1);
    let half_allowance = coin.account(LARGE, at).unmarked.allowance / 2;
    let sell = coin.sell(LARGE, half_allowance, at);
    assert_eq!(sell.fill, "full");
    assert_eq!(coin.cap(at).cap_used, sell.near_out);

    coin.buy(OTHER, 20 * ONE_NEAR, at);
    let cap = coin.cap(at);
    assert_eq!(cap.cap_used, sell.near_out);
    assert!(cap.cap_total > sell.near_out);
}

/// Window 0: LARGE buys 100 NEAR (a deep pool), then BAND buys 6 NEAR (band, more than two
/// allowances).
fn coin_with_a_deep_band_holder() -> Coin {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 100 * ONE_NEAR, after(5));
    coin.buy(BAND, 6 * ONE_NEAR, after(10));
    let view = coin.account(BAND, window_1(0));
    assert_eq!(view.class, "band");
    assert!(view.balance > 2 * view.unmarked.allowance);
    coin
}

/// One net per account per window: counted sells minus buys. An uncounted sell takes no buy NEAR,
/// and the tokens bought in the window count first when they are sold.
#[test]
fn a_buy_after_an_uncounted_sell_offsets_the_resale_of_the_bought_tokens() {
    let mut coin = coin_with_a_deep_band_holder();
    let at = window_1(1);
    let allowance = coin.account(BAND, at).unmarked.allowance;

    // The whole first allowance: it never counts.
    let first = coin.sell(BAND, allowance, at);
    assert_eq!(first.penalty_near, 0);
    assert_eq!(coin.cap(at).cap_used, 0);
    let held = coin.balance(BAND);
    let near_in = first.near_out / 2;
    coin.buy(BAND, near_in, at);
    let bought = coin.balance(BAND) - held;
    assert_eq!(coin.cap(at).cap_used, 0);
    // The next sell is the bought tokens and more, all above the first allowance: it counts in
    // full, and the buy NEAR offsets it once.
    let counted = coin.sell(BAND, bought + allowance / 4, at);
    assert_eq!(counted.fill, "full");
    assert!(counted.penalty_near > 0);
    assert_eq!(coin.cap(at).cap_used, counted.near_out - near_in);
}

#[test]
fn a_buy_after_a_counted_sell_offsets_it() {
    let mut coin = coin_with_a_deep_band_holder();
    let at = window_1(1);
    let allowance = coin.account(LARGE, at).unmarked.allowance;
    let sell = coin.sell(LARGE, allowance / 2, at);
    assert_eq!(sell.fill, "full");
    let used = coin.cap(at).cap_used;
    assert_eq!(used, sell.near_out);

    coin.buy(LARGE, used / 3, at);
    assert_eq!(coin.cap(at).cap_used, used - used / 3);
    coin.buy(LARGE, used, at);
    assert_eq!(coin.cap(at).cap_used, 0);
}

#[test]
fn a_buy_after_a_band_sell_offsets_only_its_counted_part() {
    let mut coin = coin_with_a_deep_band_holder();
    let at = window_1(1);
    let allowance = coin.account(BAND, at).unmarked.allowance;
    let sell = coin.sell(BAND, allowance + allowance / 4, at);
    let used = coin.cap(at).cap_used;
    let uncounted = sell.near_out - used;
    assert!(used > 0 && uncounted > used);

    // The buy offsets the counted part; the uncounted part takes none of it.
    coin.buy(BAND, used / 2, at);
    assert_eq!(coin.cap(at).cap_used, used - used / 2);
    coin.buy(BAND, used, at);
    assert_eq!(coin.cap(at).cap_used, 0);
}

/// Paper case 5 (wash trading): ten first-allowance round trips by a band account, then a sell
/// above the first allowance from the same account. The round trips add nothing to the cap.
/// Each buy offsets once: the tokens it bought count when they are sold, so the later sell keeps
/// as offset only the last buy (its tokens are in that sell) and the fees the round trips left
/// in the pool.
#[test]
fn paper_case_5_first_allowance_round_trips_leave_only_their_fees_as_offset_for_a_later_sell() {
    let mut coin = coin_with_a_deep_band_holder();
    let at = window_1(1);
    let allowance = coin.account(BAND, at).unmarked.allowance;
    let round_trip = allowance / 50;
    let mut last_buy = 0;
    let mut fees = 0;
    for _ in 0..10 {
        let sold = coin.sell(BAND, round_trip, at);
        last_buy = sold.near_out;
        coin.buy(BAND, last_buy, at);
        // The sell fee and the buy fee (1%, rounded up).
        fees += sold.fee_near + last_buy.div_ceil(100);
        assert_eq!(coin.cap(at).cap_used, 0);
    }

    // The first allowance left with no room back for buys: the allowance less the ten sells.
    let first_allowance_left = allowance - 10 * round_trip;
    let before = coin.clone();
    let sell = coin.sell(BAND, allowance + allowance / 4, at);
    let uncounted = payout_for(&before, first_allowance_left, at);
    let counted_with_no_offset = sell.near_out - uncounted;
    assert!(sell.penalty_near > 0);
    let used = coin.cap(at).cap_used;
    assert!(used <= counted_with_no_offset, "{used}");
    assert!(
        used + last_buy + fees >= counted_with_no_offset,
        "{used} {last_buy} {fees} {counted_with_no_offset}"
    );
}

#[test]
fn a_round_trip_by_one_account_adds_nothing_to_the_cap_total() {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 50 * ONE_NEAR, after(10));
    coin.buy(SECOND_LARGE, 3 * ONE_NEAR, after(11));
    let at = window_1(1);
    let half_allowance = coin.account(LARGE, at).unmarked.allowance / 2;
    let first = coin.sell(LARGE, half_allowance, at);
    let used = coin.cap(at).cap_used;
    assert_eq!(used, first.near_out);

    let held = coin.balance(SECOND_LARGE);
    coin.buy(SECOND_LARGE, 5 * ONE_NEAR, at);
    let round_trip = coin.sell(SECOND_LARGE, coin.balance(SECOND_LARGE) - held, at);
    assert_eq!(round_trip.fill, "full");
    assert_eq!(coin.cap(at).cap_used, used);
    assert_eq!(round_trip.cap_used, used);
}

#[test]
fn fc_quote_sell_cap_left_after_equals_the_real_sell_and_the_cap_resets_in_the_next_window() {
    let mut coin = coin_with_a_full_cap_in_window_1();
    let at = window_1(2);
    let quote = coin.quote(SECOND_LARGE, coin.balance(SECOND_LARGE), at);
    let sell = coin.sell_all(SECOND_LARGE, at);
    assert_eq!(quote.tokens_filled, sell.tokens_sold);
    assert_eq!(quote.near_out, sell.near_out);
    assert_eq!(quote.limit_reason, sell.limit_reason);
    assert_eq!(quote.cap_left_after, sell.cap_left);

    let next = after(2 * HOUR_S);
    let cap = coin.cap(next);
    assert_eq!(cap.cap_used, 0);
    assert_eq!(cap.fills_this_window, 0);
    assert!(!cap.is_full);
    assert_eq!(cap.cap_left, cap.real_near_now * C_BPS / (BPS + C_BPS));
    assert_eq!(cap.window_start_ns, LAUNCH_NS + 2 * HOUR_S * 1_000_000_000);
}

/// QUESTIONS.md Q12 (b): for a band account, `penalty_applies_above` and `max_full_fill_now`
/// give the same value, also when the account's balance is below its first allowance left.
#[test]
fn band_penalty_applies_above_equals_max_full_fill_now_when_the_balance_is_below_the_room() {
    let mut coin = coin_with_a_deep_band_holder();
    let at = window_1(1);
    let allowance = coin.account(BAND, at).unmarked.allowance;
    // Send away all but a little more than one allowance.
    let keep = allowance + allowance / 10;
    coin.send(BAND, OTHER, coin.balance(BAND) - keep, at);
    // A buy gives room back; sending the bought tokens away keeps the balance.
    coin.buy(BAND, ONE_NEAR / 2, at);
    coin.send(BAND, OTHER, coin.balance(BAND) - keep, at);

    let view = coin.account(BAND, at);
    assert_eq!(view.class, "band");
    assert_eq!(view.balance, keep);
    assert_eq!(view.max_full_fill_now, keep);
    assert_eq!(view.penalty_applies_above, Some(view.max_full_fill_now));
    let sell = coin.sell_all(BAND, at);
    assert_eq!(sell.fill, "full");
    assert_eq!(sell.penalty_near, 0);
}

/// The allowance at the launch token reserve (1e27), from a separate integer model: the largest
/// q with (y / (y + q))^2 >= 0.98.
const ALLOWANCE_AT_LAUNCH: u128 = 10_152_544_552_210_749_144_063_374;
/// The floor at the launch token reserve: 10 bps of 1e27.
const FLOOR_AT_LAUNCH: u128 = 1_000_000_000_000_000_000_000_000;

#[test]
fn the_floor_and_the_allowance_read_the_same_window_snapshot_in_window_0_and_later_windows() {
    let mut coin = Coin::launched();
    // Window 0: the snapshot is the launch reserve, before and after the window's buys.
    for (i, at) in [after(1), after(20)].into_iter().enumerate() {
        let view = coin.account(OTHER, at);
        assert_eq!(view.unmarked.allowance, ALLOWANCE_AT_LAUNCH, "step {i}");
        assert_eq!(view.unmarked.floor_left, FLOOR_AT_LAUNCH, "step {i}");
        coin.buy(LARGE, 10 * ONE_NEAR, at);
    }
    // Window 1: both come from the token reserve at the window's first action.
    let reserve = view_pool(&coin.state, window_1(0)).token_reserve;
    let before = coin.account(OTHER, window_1(0));
    assert_eq!(before.unmarked.floor_left, reserve / 1_000);
    coin.buy(SECOND_LARGE, 5 * ONE_NEAR, window_1(0));
    coin.sell_all(LARGE, window_1(1));
    let later = coin.account(OTHER, window_1(2));
    assert_eq!(later.unmarked.floor_left, reserve / 1_000);
    assert_eq!(later.unmarked.allowance, before.unmarked.allowance);
    assert!(view_pool(&coin.state, window_1(2)).token_reserve != reserve);
}

#[test]
fn a_sell_that_is_the_first_action_of_a_window_does_not_change_its_own_floor() {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 10 * ONE_NEAR, after(10));
    let at = window_1(0);
    let floor = coin.account(LARGE, at).unmarked.floor_left;
    let other_floor = coin.account(OTHER, at).unmarked.floor_left;
    assert_eq!(floor, other_floor);

    let sell = coin.sell_all(LARGE, at);
    assert!(sell.tokens_sold > 0);
    let view = coin.account(LARGE, at);
    // floor_left is the floor less the tokens sold: the floor itself kept its value.
    assert_eq!(view.unmarked.floor_left, floor.saturating_sub(sell.tokens_sold));
    assert_eq!(coin.account(OTHER, at).unmarked.floor_left, floor);
}

#[test]
fn a_buy_that_is_the_first_action_of_a_window_does_not_change_that_window_s_floor() {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 10 * ONE_NEAR, after(10));
    let at = window_1(0);
    let floor = coin.account(OTHER, at).unmarked.floor_left;
    let buyer_floor = coin.account(SECOND_LARGE, at).unmarked.floor_left;

    coin.buy(SECOND_LARGE, 20 * ONE_NEAR, at);

    assert_eq!(coin.account(OTHER, at).unmarked.floor_left, floor);
    assert_eq!(
        coin.account(SECOND_LARGE, at).unmarked.floor_left,
        buyer_floor
    );
    assert_eq!(buyer_floor, floor);
}

/// QUESTIONS.md answer to Q13 item 1: at an exactly full cap, a large account buys X and sells
/// back X plus Y of the tokens it held before. Its buy opens no room for the earlier tokens:
/// the Y part fills only up to its floor plus the room the fees the pool kept add (c x those
/// fees), never the full Y.
#[test]
fn at_an_exactly_full_cap_a_buy_opens_no_room_for_tokens_the_account_held_before() {
    let mut coin = coin_with_a_full_cap_in_window_1();
    let at = window_1(2);
    let cap = coin.cap(at);
    assert!(cap.is_full);
    assert!(within_cap(cap.cap_used, cap.real_near_now));

    let held = coin.balance(SECOND_LARGE);
    let x = ONE_NEAR;
    coin.buy(SECOND_LARGE, x, at);
    let bought = coin.balance(SECOND_LARGE) - held;
    let floor_left = coin.account(SECOND_LARGE, at).unmarked.floor_left;
    let y = 5 * floor_left;
    let before = coin.clone();

    let sell = coin.sell(SECOND_LARGE, bought + y, at);

    assert!(sell.tokens_sold >= bought, "the bought part comes back");
    let y_filled = sell.tokens_sold - bought;
    assert!(y_filled < y, "the full Y filled");
    let y_near = sell.near_out - payout_for(&before, bought, at);
    let floor_near = payout_for(&before, bought + floor_left, at) - payout_for(&before, bought, at);
    // Fees the pool kept: the 1% buy fee on X, rounded up, and the sell's fee.
    let kept_fees = x.div_ceil(100) + sell.fee_near;
    let fee_room = (kept_fees * C_BPS).div_ceil(BPS);
    assert!(
        y_near <= floor_near + fee_room + 1,
        "Y part {y_near} > floor {floor_near} + fee room {fee_room}"
    );
}

/// Tokens bought in the current cap window count toward the cap when they are sold, for every
/// account, small accounts too. The account's net (counted sells minus buys) offsets them.
#[test]
fn a_small_account_with_no_earlier_counted_sell_buys_and_sells_back_at_an_over_full_cap() {
    let mut coin = coin_with_a_cap_above_full_in_window_1();
    // SMALL sold its whole bag uncounted at window_1(2); now it buys X and sells X back.
    let at = window_1(3);
    let used = coin.cap(at).cap_used;
    coin.buy(SMALL, ONE_NEAR / 10, at);
    let bought = coin.balance(SMALL);
    let sell = coin.sell(SMALL, bought, window_1(4));
    assert_eq!(sell.class, "small");
    assert_eq!(sell.fill, "full");
    assert_eq!(sell.limit_reason, None);
    // The resale pays less than X, so the net stays below zero and adds nothing.
    assert!(sell.near_out < ONE_NEAR / 10);
    assert_eq!(coin.cap(window_1(4)).cap_used, used);
}

#[test]
fn a_small_account_that_sells_bought_tokens_for_more_than_it_paid_at_a_full_cap_waits_for_the_next_window(
) {
    let mut coin = coin_with_a_full_cap_in_window_1();
    let near_in = ONE_NEAR / 10;
    coin.buy(OTHER, near_in, window_1(2));
    let bought = coin.balance(OTHER);
    // A large buy raises the price; SECOND_LARGE then fills the cap the buy opened.
    coin.buy("pump.test", 4 * ONE_NEAR, window_1(3));
    coin.sell_all(SECOND_LARGE, window_1(4));
    assert!(coin.cap(window_1(4)).is_full);
    let worth = payout_for(&coin, bought, window_1(5));
    assert!(worth > near_in, "{worth}");
    assert_eq!(coin.account(OTHER, window_1(5)).class, "small");

    let sell = coin.sell(OTHER, bought, window_1(5));
    assert_eq!(sell.fill, "partial");
    assert_eq!(sell.limit_reason, Some("cap_full"));
    // The part above its buy NEAR is held back, up to the room the pool's kept fees add.
    assert!(sell.near_out >= near_in * 99 / 100, "{}", sell.near_out);
    assert!(sell.near_out < worth);
    let excess = sell.near_out.saturating_sub(near_in);
    assert!(excess <= near_in / 100, "{excess}");
    // The rest fills in the next window.
    let next = coin.sell_all(OTHER, after(2 * HOUR_S + 1));
    assert_eq!(next.fill, "full");
}

/// A band account buys while its first allowance is unused, the price then rises, and it sells
/// the bought tokens: the bought part counts toward the cap, net of the buy. The first allowance
/// held from before the window never counts (ADR 0004), so without the bought part the cap used
/// would stay 0.
#[test]
fn a_band_account_that_buys_then_sells_the_bought_tokens_counts_the_bought_part_toward_the_cap() {
    let mut coin = coin_with_a_deep_band_holder();
    let at = window_1(1);
    let held = coin.balance(BAND);
    let buy_near = ONE_NEAR / 10;
    coin.buy(BAND, buy_near, at);
    let bought = coin.balance(BAND) - held;
    // Another account's buy lifts the price, so the bought tokens sell for more than they cost.
    coin.buy(OTHER, 50 * ONE_NEAR, at);
    assert_eq!(coin.cap(at).cap_used, 0);

    let sell = coin.sell(BAND, bought, at);

    assert_eq!(sell.fill, "full");
    assert_eq!(sell.penalty_near, 0);
    assert!(sell.near_out > buy_near);
    assert_eq!(coin.cap(at).cap_used, sell.near_out - buy_near);
}

/// At a full cap a band account's first allowance still fills, and only the part above it is
/// checked: a sell inside the first allowance fills in full and adds nothing to the cap used,
/// and a sell above it stops at the first allowance.
#[test]
fn at_a_full_cap_a_band_first_allowance_fills_and_only_the_part_above_is_checked() {
    let mut coin = coin_with_a_cap_above_full_in_window_1();
    let at = window_1(3);
    let first_allowance = coin.account(BAND, at).penalty_applies_above.expect("band");
    assert!(coin.balance(BAND) > first_allowance);
    let used = coin.cap(at).cap_used;

    let inside = coin.clone().sell(BAND, first_allowance / 2, at);
    assert_eq!(inside.fill, "full");
    assert_eq!(inside.limit_reason, None);
    assert_eq!(inside.penalty_near, 0);

    let above = coin.sell(BAND, first_allowance + 1, at);
    assert_eq!(above.tokens_sold, first_allowance);
    assert_eq!(above.fill, "partial");
    assert_eq!(above.limit_reason, Some("cap_full"));
    assert_eq!(above.penalty_near, 0);
    assert_eq!(coin.cap(at).cap_used, used);
}

/// Window 1 ends with a full cap well past full: LARGE and eight small wallets sold everything.
/// Then SMALL (bought in window 1) gains from a later buy by OTHER. SMALL is small with no
/// marked tokens, and the cap holds its profit: `fc_account.max_full_fill_now` is below its
/// balance and the quote for the whole balance stops at the cap.
#[test]
fn at_a_full_cap_a_small_account_that_bought_in_the_window_cannot_sell_its_whole_balance_now() {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 10 * ONE_NEAR, after(10));
    for i in 0..8 {
        coin.buy(&format!("wallet-{i}.test"), ONE_NEAR / 2, after(11 + i));
    }
    let at = window_1(1);
    coin.sell_all(LARGE, at);
    for i in 0..8 {
        coin.sell_all(&format!("wallet-{i}.test"), at);
    }
    coin.buy(SMALL, ONE_NEAR / 2, at);
    coin.buy(OTHER, 2 * ONE_NEAR, at);
    assert!(coin.cap(at).is_full);

    let view = coin.account(SMALL, at);
    assert_eq!(view.class, "small");
    assert_eq!(view.marked_balance, 0);
    assert!(view.max_full_fill_now < view.balance);
    let quote = coin.quote(SMALL, view.balance, at);
    assert_eq!(quote.limit_reason, Some("cap_full"));
}

/// A small account with no floor buys twice in the window and sells once. Its bought tokens count
/// above the total NEAR it paid for both buys, and fill only as far as the cap room allows.
#[test]
fn a_small_account_that_buys_twice_and_sells_once_counts_the_part_above_all_the_near_it_paid() {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 10 * ONE_NEAR, after(10));
    for i in 0..8 {
        coin.buy(&format!("wallet-{i}.test"), ONE_NEAR / 2, after(11 + i));
    }
    let at = window_1(1);
    coin.sell_all(LARGE, at);
    for i in 0..8 {
        coin.sell_all(&format!("wallet-{i}.test"), at);
    }
    let (first_buy, second_buy) = (ONE_NEAR / 4, ONE_NEAR / 4);
    coin.buy(SMALL, first_buy, at);
    coin.buy(SMALL, second_buy, at);
    coin.buy(OTHER, 2 * ONE_NEAR, at);
    let used_before = coin.cap(at).cap_used;
    assert!(coin.cap(at).is_full);
    let view = coin.account(SMALL, at);
    assert_eq!(view.class, "small");
    assert!(view.max_full_fill_now < view.balance);

    let sell = coin.sell_all(SMALL, at);

    // The sell pays the NEAR of both buys and not more: the part above it counts and is held.
    assert_eq!(sell.limit_reason, Some("cap_full"));
    assert_eq!(sell.tokens_sold, view.max_full_fill_now);
    assert_eq!(sell.near_out, first_buy + second_buy);
    assert_eq!(coin.cap(at).cap_used, used_before);
}
