//! Sells above the allowance (ADR 0004), through the engine interface: `apply` and the views.
//!
//! A large account's sell above its allowance fills in part, and the rest stays in the wallet.
//! A band account's first allowance in each window always fills; the part above it fills too and
//! pays a penalty of `penalty_bps` (10%) of its NEAR output, which stays in the pool. A limited
//! sell never panics for the limit.
//!
//! Worked numbers come from a separate Python model of the pool (exact integers): 100 NEAR
//! virtual, 1e27 tokens, 1% swap fee rounded up, NEAR out rounded down. The part above the first
//! allowance gets the NEAR the whole sell pays minus the NEAR the first-allowance part alone
//! would pay, both before the swap fee; its NEAR output is that amount minus its own swap fee
//! (rounded up), and the penalty is 10% of it, rounded up.

use std::collections::BTreeMap;

use rule_engine::{
    apply, launch, quote_sell, view_account, AccountView, Action, Applied, At, CoinState, Event,
    FcSell, FixedValues, Holder, Holders, LaunchRequest, Payout,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const ONE_TOKEN: u128 = 1_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const HOUR_S: u64 = 3_600;

const BAND: &str = "band.test";
const LARGE: &str = "large.test";
const SMALL: &str = "small.test";
const OTHER: &str = "other.test";

/// The band account's 1.5 NEAR buy at launch: about 1.46% of supply.
const BAND_TOKENS: u128 = 14_632_704_340_542_937_379_908_360;
/// The allowance in window 1 (token reserve 985_367_295_659_457_062_620_091_640 after the buy).
const ALLOWANCE_WINDOW_1: u128 = 10_003_985_369_474_059_362_529_806;
/// The floor in window 1: 0.1% (10 bps) of that token reserve, rounded down.
const FLOOR_WINDOW_1: u128 = 985_367_295_659_457_062_620_091;

/// Seconds after launch, as a block time. The block is 18 blocks later than the second, after
/// the early window (blocks 0 to 17 after the launch at block 100), so buys here are unmarked.
fn after(seconds: u64) -> At {
    At {
        block: 100 + 18 + seconds,
        ns: LAUNCH_NS + seconds * 1_000_000_000,
    }
}

/// The first second of window 1.
fn window_1() -> At {
    after(HOUR_S)
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

    fn holders(&self, action: &Action) -> Holders {
        action
            .accounts()
            .into_iter()
            .filter_map(|a| self.ledger.get(&a).map(|h| (a.clone(), h.clone())))
            .collect()
    }

    fn act(&mut self, action: Action, at: At) -> Applied {
        let applied = apply(&self.state, self.holders(&action), &action, at).expect("action");
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

    fn send(&mut self, from: &str, to: &str, amount: u128, at: At) {
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
        let holders: Holders = self
            .ledger
            .get(account)
            .map(|h| (account.to_string(), h.clone()))
            .into_iter()
            .collect();
        quote_sell(&self.state, holders, account, amount, at).expect("quote")
    }

    fn account(&self, account: &str, at: At) -> AccountView {
        view_account(&self.state, account, self.ledger.get(account), at)
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

const START_OF_WINDOW_1: At = At {
    block: 100 + HOUR_S,
    ns: LAUNCH_NS + HOUR_S * 1_000_000_000,
};

/// Forty accounts buy 1 NEAR each (under 1% of supply each) as the first actions of window 1,
/// so the real NEAR cap (ADR 0005) has room for the sells this test is about. The window's
/// token reserve is recorded before them, so the window's allowances do not change.
fn deepen_the_pool_at_the_start_of_window_1(coin: &mut Coin) {
    for i in 0..40 {
        coin.buy(&format!("depth-{i}.test"), ONE_NEAR, START_OF_WINDOW_1);
    }
}

/// BAND buys 1.5 NEAR at launch (band: above one allowance, below 2%).
fn coin_with_a_band_holder() -> Coin {
    let mut coin = Coin::launched();
    coin.buy(BAND, 1_500 * ONE_NEAR / 1_000, after(10));
    assert_eq!(coin.balance(BAND), BAND_TOKENS);
    coin
}

#[test]
fn a_band_sell_inside_its_first_allowance_fills_in_full_with_no_penalty() {
    let mut coin = coin_with_a_band_holder();
    assert_eq!(coin.account(BAND, window_1()).class, "band");

    let sold = coin.sell(BAND, ALLOWANCE_WINDOW_1, window_1());

    assert_eq!(sold.class, "band");
    assert_eq!(sold.tokens_sold, ALLOWANCE_WINDOW_1);
    assert_eq!(sold.fill, "full");
    assert_eq!(sold.limit_reason, None);
    assert_eq!(sold.penalty_near, 0);
    assert_eq!(sold.fee_near, 10_201_263_933_915_967_826_401);
    assert_eq!(sold.near_out, 1_009_925_129_457_680_814_813_615);
}

#[test]
fn a_band_sell_above_its_first_allowance_fills_in_full_and_pays_ten_percent_of_the_part_above_to_the_pool(
) {
    let mut coin = coin_with_a_band_holder();
    deepen_the_pool_at_the_start_of_window_1(&mut coin);
    let real_near_before = coin.state.real_near;

    let applied = coin.sell_applied(BAND, BAND_TOKENS, window_1());
    let sold = sell_event(&applied);

    assert_eq!(sold.class, "band");
    assert_eq!(sold.tokens_asked, BAND_TOKENS);
    assert_eq!(sold.tokens_sold, BAND_TOKENS);
    assert_eq!(sold.fill, "full");
    assert_eq!(sold.limit_reason, None);
    assert_eq!(sold.fee_near, 28_606_610_122_305_435_272_571);
    assert_eq!(sold.penalty_near, 88_339_332_811_911_659_378_723);
    assert_eq!(sold.near_out, 2_743_715_069_296_326_432_605_722);
    // The seller gets near_out; the fee and the penalty stay in the pool's real NEAR.
    assert_eq!(
        applied.payout,
        Some(Payout {
            account: BAND.to_string(),
            amount: sold.near_out
        })
    );
    assert_eq!(coin.state.real_near, real_near_before - sold.near_out);
    assert_eq!(coin.balance(BAND), 0);
}

/// LARGE buys 100 NEAR first (about half the supply leaves the pool, so one allowance is about
/// 0.5% of supply), then BAND buys 6 NEAR (about 1.45%: band, and still band after one
/// allowance).
fn coin_with_a_deep_band_holder() -> Coin {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 100 * ONE_NEAR, after(5));
    coin.buy(BAND, 6 * ONE_NEAR, after(10));
    let view = coin.account(BAND, window_1());
    assert_eq!(view.class, "band");
    assert!(view.balance > 2 * view.unmarked.allowance);
    coin
}

#[test]
fn a_band_account_s_first_allowance_counts_over_all_its_sells_in_the_window() {
    let mut coin = coin_with_a_deep_band_holder();
    let allowance = coin.account(BAND, window_1()).unmarked.allowance;

    let first = coin.sell(BAND, allowance / 2, window_1());
    let rest_of_first_allowance = allowance - allowance / 2;
    let view = coin.account(BAND, after(HOUR_S + 1));
    assert_eq!(first.penalty_near, 0);
    assert_eq!(view.penalty_applies_above, Some(rest_of_first_allowance));

    // The second sell's first-allowance part is only what the first sell left.
    let mut inside = coin.clone();
    let inside_sell = inside.sell(BAND, rest_of_first_allowance, after(HOUR_S + 1));
    assert_eq!(inside_sell.penalty_near, 0);
    assert_eq!(inside_sell.fill, "full");
    let above = coin.sell(
        BAND,
        rest_of_first_allowance + ONE_TOKEN * 1_000_000,
        after(HOUR_S + 1),
    );
    assert_eq!(above.class, "band");
    assert_eq!(above.fill, "full");
    assert_eq!(above.limit_reason, None);
    assert!(above.penalty_near > 0);
    assert_eq!(
        coin.account(BAND, after(HOUR_S + 2)).penalty_applies_above,
        Some(0)
    );
}

#[test]
fn a_band_account_gets_a_new_first_allowance_with_no_penalty_in_each_window() {
    let mut coin = coin_with_a_deep_band_holder();
    let allowance_1 = coin.account(BAND, window_1()).unmarked.allowance;
    let sold_1 = coin.sell(BAND, allowance_1 + ONE_TOKEN * 1_000_000, window_1());
    assert!(sold_1.penalty_near > 0);

    let window_2 = after(2 * HOUR_S);
    let view = coin.account(BAND, window_2);
    assert_eq!(view.class, "band");
    assert_eq!(view.penalty_applies_above, Some(view.unmarked.allowance));
    let sold_2 = coin.sell(BAND, view.unmarked.allowance, window_2);
    assert_eq!(sold_2.fill, "full");
    assert_eq!(sold_2.penalty_near, 0);
}

#[test]
fn band_max_full_fill_now_and_penalty_applies_above_match_the_next_real_sell() {
    let coin = coin_with_a_deep_band_holder();
    let view = coin.account(BAND, window_1());
    let allowance = view.unmarked.allowance;
    assert_eq!(view.max_full_fill_now, allowance);
    assert_eq!(view.penalty_applies_above, Some(allowance));

    let at_the_limit = coin.clone().sell(BAND, view.max_full_fill_now, window_1());
    assert_eq!(at_the_limit.fill, "full");
    assert_eq!(at_the_limit.tokens_sold, allowance);
    assert_eq!(at_the_limit.penalty_near, 0);

    let above_the_limit = coin.clone().sell(BAND, allowance + ONE_TOKEN, window_1());
    assert_eq!(above_the_limit.fill, "full");
    assert!(above_the_limit.penalty_near > 0);
}

#[test]
fn penalty_applies_above_is_null_for_small_and_large_accounts() {
    let mut coin = coin_with_a_deep_band_holder();
    coin.buy(SMALL, ONE_NEAR / 10, after(20));
    let small = coin.account(SMALL, window_1());
    assert_eq!(small.class, "small");
    assert_eq!(small.penalty_applies_above, None);
    assert_eq!(small.max_full_fill_now, small.balance);
    let large = coin.account(LARGE, window_1());
    assert_eq!(large.class, "large");
    assert_eq!(large.penalty_applies_above, None);
    assert_eq!(large.max_full_fill_now, large.unmarked.allowance);
}

#[test]
fn floor_left_is_ten_bps_of_the_window_token_reserve_less_the_tokens_used() {
    let mut coin = coin_with_a_band_holder();
    let view = coin.account(BAND, window_1());
    assert_eq!(view.unmarked.floor_left, FLOOR_WINDOW_1);

    let sold = coin.sell(BAND, view.unmarked.floor_left, window_1());
    assert_eq!(sold.fill, "full");
    assert_eq!(sold.tokens_sold, FLOOR_WINDOW_1);
    assert_eq!(sold.penalty_near, 0);
    let after_floor = coin.account(BAND, after(HOUR_S + 1));
    assert_eq!(after_floor.unmarked.floor_left, 0);
    assert_eq!(
        after_floor.unmarked.left,
        ALLOWANCE_WINDOW_1 - FLOOR_WINDOW_1
    );
}

#[test]
fn a_large_account_s_floor_left_fills_in_full_and_is_zero_once_the_allowance_is_used() {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 5 * ONE_NEAR, after(10));
    deepen_the_pool_at_the_start_of_window_1(&mut coin);
    let view = coin.account(LARGE, window_1());
    assert!(view.unmarked.floor_left > 0);
    let sold = coin.sell(LARGE, view.unmarked.floor_left, window_1());
    assert_eq!(sold.fill, "full");
    assert_eq!(sold.tokens_sold, view.unmarked.floor_left);

    let rest = coin.account(LARGE, after(HOUR_S + 1)).unmarked.left;
    coin.sell(LARGE, rest, after(HOUR_S + 1));
    let used_up = coin.account(LARGE, after(HOUR_S + 2));
    assert_eq!(used_up.unmarked.floor_left, 0);
    assert_eq!(used_up.max_full_fill_now, 0);
    let none = coin.sell(LARGE, ONE_TOKEN, after(HOUR_S + 2));
    assert_eq!(none.tokens_sold, 0);
    assert_eq!(none.fill, "none");
}

#[test]
fn a_large_sell_above_its_allowance_fills_the_allowance_left_and_the_rest_stays_in_the_wallet() {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 5 * ONE_NEAR, after(10));
    deepen_the_pool_at_the_start_of_window_1(&mut coin);
    let allowance = coin.account(LARGE, window_1()).unmarked.allowance;
    coin.sell(LARGE, allowance / 3, window_1());
    let view = coin.account(LARGE, after(HOUR_S + 1));
    let balance_before = view.balance;
    assert_eq!(view.unmarked.left, allowance - allowance / 3);

    let applied = coin.sell_applied(LARGE, 2 * allowance, after(HOUR_S + 1));
    let sold = sell_event(&applied);

    assert_eq!(sold.tokens_asked, 2 * allowance);
    assert_eq!(sold.tokens_sold, view.unmarked.left);
    assert_eq!(sold.fill, "partial");
    assert_eq!(sold.limit_reason, Some("allowance"));
    assert_eq!(sold.penalty_near, 0);
    assert_eq!(sold.allowance_left, 0);
    assert_eq!(coin.balance(LARGE), balance_before - view.unmarked.left);
    assert_eq!(
        applied.payout,
        Some(Payout {
            account: LARGE.to_string(),
            amount: sold.near_out
        })
    );
}

fn assert_quote_equals_sell(coin: &mut Coin, account: &str, amount: u128, at: At) {
    let quote = coin.quote(account, amount, at);
    let sold = coin.sell(account, amount, at);
    assert_eq!(quote.tokens_filled, sold.tokens_sold);
    assert_eq!(quote.near_out, sold.near_out);
    assert_eq!(quote.penalty_near, sold.penalty_near);
    assert_eq!(quote.fill, sold.fill);
    assert_eq!(quote.limit_reason, sold.limit_reason);
    assert_eq!(quote.cap_left_after, sold.cap_left);
}

#[test]
fn fc_quote_sell_equals_the_real_band_sell_inside_and_above_the_first_allowance() {
    let mut coin = coin_with_a_deep_band_holder();
    let allowance = coin.account(BAND, window_1()).unmarked.allowance;
    assert_quote_equals_sell(&mut coin, BAND, allowance / 2, window_1());
    assert_quote_equals_sell(&mut coin, BAND, allowance, after(HOUR_S + 1));
    let quote = coin.quote(BAND, ONE_TOKEN * 1_000, after(HOUR_S + 2));
    assert!(quote.penalty_near > 0);
    assert_quote_equals_sell(&mut coin, BAND, ONE_TOKEN * 1_000, after(HOUR_S + 2));
}

#[test]
fn a_sent_bag_tops_up_a_band_account_without_using_its_first_allowance() {
    let mut coin = coin_with_a_band_holder();
    coin.buy(OTHER, ONE_NEAR / 2, after(20));
    let before = coin.account(BAND, window_1()).penalty_applies_above;
    coin.send(OTHER, BAND, ONE_TOKEN * 1_000_000, window_1());
    assert_eq!(coin.account(BAND, window_1()).penalty_applies_above, before);
}
