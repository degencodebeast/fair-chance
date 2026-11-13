//! The unmarked allowance (ADR 0004) through the engine interface: one allowance per account per
//! 1 h window on the fixed grid from launch, sized by the token reserve at the first action of
//! the window (before that action), so that one allowance moves the price down by at most
//! p = 2%. `used` is net tokens on window totals, max(0, sold - bought), with no carry-over.
//!
//! Expected allowances are worked out outside this code: the largest q with
//! (y / (y + q))^2 >= 0.98 for token reserve y, that is q = floor(y / sqrt(0.98)) - y.

use std::collections::BTreeMap;

use rule_engine::{
    apply, launch, quote_sell, view_account, AccountView, Action, Applied, At, CoinState, Event,
    FcSell, FixedValues, Holder, Holders, LaunchRequest,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const HOUR_NS: u64 = 3_600_000_000_000;

/// Allowance at the full launch reserve (1e27 base units): 1e27 / sqrt(0.98) - 1e27, rounded
/// down (Python `Decimal`, 60 digits).
const ALLOWANCE_AT_LAUNCH: u128 = 10_152_544_552_210_749_144_063_374;

/// Seconds after launch, as a block time.
fn after(seconds: u64) -> At {
    At {
        block: 100 + seconds,
        ns: LAUNCH_NS + seconds * 1_000_000_000,
    }
}

/// One coin and its whole ledger, driven only through `apply` and the views.
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

    fn buy(&mut self, account: &str, near_in: u128, at: At) -> u128 {
        let applied = self.act(
            Action::Buy {
                account: account.into(),
                near_in,
                min_tokens_out: 0,
            },
            at,
        );
        match applied.events.iter().find_map(|e| match e {
            Event::FcBuy(b) => Some(b.tokens_out),
            _ => None,
        }) {
            Some(tokens) => tokens,
            None => panic!("no fc_buy"),
        }
    }

    fn sell(&mut self, account: &str, amount: u128, at: At) -> FcSell {
        let applied = self.act(
            Action::Sell {
                account: account.into(),
                amount,
                min_near_out: 0,
                attached: 1,
            },
            at,
        );
        sell_event(&applied)
    }

    fn account(&self, account: &str, at: At) -> AccountView {
        view_account(&self.state, self.ledger.get(account), at)
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

#[test]
fn a_fresh_coin_gives_every_account_one_allowance_of_two_percent_price_drop() {
    let coin = Coin::launched();
    let view = coin.account("alice.test", after(10));
    assert_eq!(view.unmarked.allowance, ALLOWANCE_AT_LAUNCH);
    assert_eq!(view.unmarked.used, 0);
    assert_eq!(view.unmarked.left, ALLOWANCE_AT_LAUNCH);
    assert_eq!(view.unmarked.window_end_ns, LAUNCH_NS + HOUR_NS);
}

const LARGE: &str = "large.test";
const SMALL: &str = "small.test";

/// A coin where LARGE holds about 4.7% of supply (large: 2% or more), bought in window 0.
const START_OF_WINDOW_1: At = At {
    block: 100 + HOUR_NS / 1_000_000_000,
    ns: LAUNCH_NS + HOUR_NS,
};

/// Forty accounts buy 1 NEAR each (under 1% of supply each) as the first actions of window 1,
/// so the real NEAR cap (ADR 0005) has room for the sells this test is about. The window's
/// token reserve is recorded before them, so the window's allowances do not change.
fn deepen_the_pool_at_the_start_of_window_1(coin: &mut Coin) {
    for i in 0..40 {
        coin.buy(&format!("depth-{i}.test"), ONE_NEAR, START_OF_WINDOW_1);
    }
}

fn coin_with_a_large_holder() -> Coin {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 5 * ONE_NEAR, after(10));
    coin
}

#[test]
fn paper_case_d_a_buy_then_a_sell_of_the_bought_tokens_uses_no_allowance() {
    let mut coin = Coin::launched();
    let bought = coin.buy(SMALL, ONE_NEAR / 10, after(10));
    let allowance = coin.account(SMALL, after(20)).unmarked.allowance;

    let sold = coin.sell(SMALL, bought, after(20));

    assert_eq!(sold.tokens_sold, bought);
    assert_eq!(sold.allowance_used, 0);
    assert_eq!(sold.allowance_left, allowance);
    let view = coin.account(SMALL, after(30));
    assert_eq!(view.unmarked.used, 0);
    assert_eq!(view.unmarked.left, allowance);
}

#[test]
fn paper_case_d_a_sell_first_must_fit_the_allowance_when_it_happens() {
    let mut coin = coin_with_a_large_holder();
    deepen_the_pool_at_the_start_of_window_1(&mut coin);
    let window_1 = HOUR_NS / 1_000_000_000 + 10;
    let allowance = coin.account(LARGE, after(window_1)).unmarked.allowance;

    let sold = coin.sell(LARGE, 2 * allowance, after(window_1));

    assert_eq!(sold.tokens_asked, 2 * allowance);
    assert_eq!(sold.tokens_sold, allowance);
    assert_eq!(sold.fill, "partial");
    assert_eq!(sold.limit_reason, Some("allowance"));
    assert_eq!(sold.allowance_used, allowance);
    assert_eq!(sold.allowance_left, 0);
}

#[test]
fn paper_case_d_sell_then_buy_and_buy_then_sell_both_allow_the_allowance_plus_the_tokens_bought() {
    let mut coin = coin_with_a_large_holder();
    deepen_the_pool_at_the_start_of_window_1(&mut coin);
    let hour_s = HOUR_NS / 1_000_000_000;

    // Window 1: sell first (up to the allowance), then buy, then sell again.
    let start = hour_s + 10;
    let allowance_1 = coin.account(LARGE, after(start)).unmarked.allowance;
    let first = coin.sell(LARGE, allowance_1, after(start));
    let bought_1 = coin.buy(LARGE, ONE_NEAR / 2, after(start + 10));
    let second = coin.sell(LARGE, 2 * allowance_1, after(start + 20));
    assert_eq!(first.tokens_sold, allowance_1);
    assert_eq!(first.fill, "full");
    assert_eq!(second.tokens_sold, bought_1);
    assert_eq!(second.fill, "partial");
    assert_eq!(
        first.tokens_sold + second.tokens_sold,
        allowance_1 + bought_1
    );
    assert_eq!(second.allowance_used, allowance_1);

    // Window 2: buy first, then one sell of the allowance plus the tokens bought fills in full.
    let start = 2 * hour_s + 10;
    let bought_2 = coin.buy(LARGE, ONE_NEAR / 2, after(start));
    let allowance_2 = coin.account(LARGE, after(start + 10)).unmarked.allowance;
    let sold = coin.sell(LARGE, allowance_2 + bought_2, after(start + 10));
    assert_eq!(sold.tokens_sold, allowance_2 + bought_2);
    assert_eq!(sold.fill, "full");
    assert_eq!(sold.limit_reason, None);
    assert_eq!(sold.allowance_used, allowance_2);
    assert_eq!(sold.allowance_left, 0);
    let more = coin.sell(LARGE, 1, after(start + 20));
    assert_eq!(more.tokens_sold, 0);
    assert_eq!(more.fill, "none");
}

const OTHER: &str = "other.test";

/// The exact block time `ns` nanoseconds after launch.
fn at_ns(ns: u64) -> At {
    At {
        block: 100 + ns / 1_000_000_000,
        ns: LAUNCH_NS + ns,
    }
}

#[test]
fn paper_case_b_the_first_action_of_a_window_sets_every_account_s_token_reserve_snapshot() {
    let mut coin = coin_with_a_large_holder();
    let window_1 = HOUR_NS + 10_000_000_000;
    let before_first_action = coin.account(SMALL, at_ns(window_1)).unmarked.allowance;

    // The first action of window 1 is a large buy by another account: it takes tokens out of
    // the pool, but the window's allowances use the reserve before it.
    coin.buy(OTHER, 20 * ONE_NEAR, at_ns(window_1));
    coin.buy(OTHER, 20 * ONE_NEAR, at_ns(window_1 + 1_000_000_000));
    assert_eq!(
        coin.account(SMALL, at_ns(window_1 + 2_000_000_000))
            .unmarked
            .allowance,
        before_first_action
    );
    assert_eq!(
        coin.account(LARGE, at_ns(window_1 + 2_000_000_000))
            .unmarked
            .allowance,
        before_first_action
    );
    let sold = coin.sell(
        LARGE,
        2 * before_first_action,
        at_ns(window_1 + 2_000_000_000),
    );
    assert_eq!(sold.tokens_sold, before_first_action);

    // Window 2 has its own snapshot: the smaller reserve after the buys gives a smaller allowance.
    let window_2 = coin.account(SMALL, at_ns(2 * HOUR_NS)).unmarked.allowance;
    assert!(window_2 < before_first_action);
}

#[test]
fn paper_case_b_every_allowance_resets_at_the_same_grid_time_with_no_carry_over() {
    // LARGE bought in window 0, so it uses its allowance in window 1; SMALL buys in window 1
    // and sells nothing.
    let mut coin = coin_with_a_large_holder();
    deepen_the_pool_at_the_start_of_window_1(&mut coin);
    coin.buy(SMALL, ONE_NEAR / 10, at_ns(HOUR_NS + 20_000_000_000));
    let last_ns_of_window_1 = 2 * HOUR_NS - 1;
    let allowance_1 = coin
        .account(LARGE, at_ns(last_ns_of_window_1))
        .unmarked
        .allowance;
    coin.sell(LARGE, allowance_1, at_ns(last_ns_of_window_1));

    let used_up = coin.account(LARGE, at_ns(last_ns_of_window_1));
    assert_eq!(used_up.unmarked.used, allowance_1);
    assert_eq!(used_up.unmarked.left, 0);
    assert_eq!(used_up.unmarked.window_end_ns, LAUNCH_NS + 2 * HOUR_NS);
    assert_eq!(
        coin.account(SMALL, at_ns(last_ns_of_window_1))
            .unmarked
            .window_end_ns,
        LAUNCH_NS + 2 * HOUR_NS
    );

    // At launch + 2 h exactly, both accounts start window 2: nothing used, and the small
    // account's unused allowance from window 1 does not add to window 2.
    let large_2 = coin.account(LARGE, at_ns(2 * HOUR_NS));
    let small_2 = coin.account(SMALL, at_ns(2 * HOUR_NS));
    assert_eq!(large_2.unmarked.used, 0);
    assert_eq!(large_2.unmarked.left, large_2.unmarked.allowance);
    assert_eq!(small_2.unmarked.left, small_2.unmarked.allowance);
    assert_eq!(small_2.unmarked.allowance, large_2.unmarked.allowance);
    assert_eq!(large_2.unmarked.window_end_ns, LAUNCH_NS + 3 * HOUR_NS);
    assert_eq!(small_2.unmarked.window_end_ns, LAUNCH_NS + 3 * HOUR_NS);
    let sold = coin.sell(LARGE, 2 * large_2.unmarked.allowance, at_ns(2 * HOUR_NS));
    assert_eq!(sold.tokens_sold, large_2.unmarked.allowance);
}

impl Coin {
    fn quote(&self, account: &str, amount: u128, at: At) -> rule_engine::SellQuote {
        let holders: Holders = self
            .ledger
            .get(account)
            .map(|h| (account.to_string(), h.clone()))
            .into_iter()
            .collect();
        quote_sell(&self.state, holders, account, amount, at).expect("quote")
    }
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
fn fc_quote_sell_equals_the_real_sell_for_full_partial_and_empty_fills() {
    let mut coin = coin_with_a_large_holder();
    let small = coin.buy(SMALL, ONE_NEAR / 10, after(20));
    assert_quote_equals_sell(&mut coin, SMALL, small / 2, after(30));

    // First action of window 1 is the quoted sell itself: the quote uses the same snapshot.
    let window_1 = HOUR_NS + 5_000_000_000;
    let allowance = coin.account(LARGE, at_ns(window_1)).unmarked.allowance;
    assert_quote_equals_sell(&mut coin, LARGE, allowance / 2, at_ns(window_1));
    assert_quote_equals_sell(&mut coin, LARGE, allowance, at_ns(window_1 + 1));
    assert_quote_equals_sell(&mut coin, LARGE, allowance, at_ns(window_1 + 2));
}

#[test]
fn fc_account_max_full_fill_now_of_a_large_account_is_a_sell_that_fills_in_full() {
    let mut coin = coin_with_a_large_holder();
    let at = after(HOUR_NS / 1_000_000_000 + 10);
    let view = coin.account(LARGE, at);
    assert!(view.max_full_fill_now < view.balance);
    let sold = coin.sell(LARGE, view.max_full_fill_now, at);
    assert_eq!(sold.fill, "full");
    assert_eq!(sold.tokens_sold, view.max_full_fill_now);
}

#[test]
fn fc_quote_buy_allowance_after_equals_fc_account_unmarked_allowance_after_the_buy() {
    let quoted = Coin::launched();
    let at = after(10);
    let quote = rule_engine::quote_buy(&quoted.state, Holders::new(), SMALL, ONE_NEAR / 10, at)
        .expect("quote");
    let mut bought = Coin::launched();
    bought.buy(SMALL, ONE_NEAR / 10, at);
    assert_eq!(quote.allowance_after, ALLOWANCE_AT_LAUNCH);
    assert_eq!(
        quote.allowance_after,
        bought.account(SMALL, at).unmarked.allowance
    );
}
