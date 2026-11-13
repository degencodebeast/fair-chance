//! The class floor per window (ADR 0001, ADR 0004): an account's class in a window is never
//! below its class just before its first action in that window. A buy or a receive can raise
//! it; a sell or a send cannot drop it until the next window. Large stays large until the
//! balance is zero. `fc_account` reports the window class, so its class, `penalty_applies_above`
//! and `max_full_fill_now` match the next real sell.

use std::collections::BTreeMap;

use rule_engine::{
    apply, launch, quote_sell, view_account, AccountView, Action, Applied, At, CoinState, Event,
    FcSell, FixedValues, Holder, Holders, LaunchRequest,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const HOUR_S: u64 = 3_600;

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

/// Window 0: forty accounts buy 1 NEAR each, so the real NEAR cap has room in window 1.
fn deepen_the_pool_in_window_0(coin: &mut Coin) {
    for i in 0..40 {
        coin.buy(&format!("depth-{i}.test"), ONE_NEAR, after(2));
    }
}

/// A band bag of about 1.15 NEAR bought in window 0 (the random-run case that found the
/// escape), with a shallow pool: the cap fills on the first sell.
fn coin_with_a_band_bag_in_a_shallow_pool() -> Coin {
    let mut coin = Coin::launched();
    coin.buy(BAND, 1_157_608_309_733_272_483_625_785, after(1));
    assert_eq!(coin.account(BAND, window_1(0)).class, "band");
    coin
}

/// The escape: a band account buys X, sells everything (the cap holds it to its first
/// allowance plus the tokens bought, with no penalty), is now below one allowance, and sells
/// the rest. The rest is above its first allowance, so it is a band sell and the full cap holds
/// it back.
#[test]
fn a_band_account_that_drops_below_one_allowance_by_a_sell_stays_band_for_the_rest_of_the_window() {
    let mut coin = coin_with_a_band_bag_in_a_shallow_pool();
    let at = window_1(0);
    coin.buy(BAND, ONE_NEAR / 100, at);
    let first = coin.sell_all(BAND, window_1(1));
    assert_eq!(first.class, "band");
    assert_eq!(first.limit_reason, Some("cap_full"));
    let view = coin.account(BAND, window_1(2));
    assert!(view.balance <= view.unmarked.allowance);
    assert_eq!(view.class, "band");

    let rest = coin.sell_all(BAND, window_1(2));

    assert_eq!(rest.class, "band");
    assert_eq!(rest.fill, "partial");
    assert_eq!(rest.limit_reason, Some("cap_full"));
    // The buy NEAR offsets once: only the part of it the first sell did not use (less than the
    // buy's two fees) is left for the rest.
    assert!(
        rest.near_out <= ONE_NEAR / 100 * 2 / 100,
        "{}",
        rest.near_out
    );
}

/// The same escape with cap room: the rest pays the band penalty.
#[test]
fn the_band_buy_and_sell_escape_pays_the_penalty_when_the_cap_has_room() {
    let mut coin = Coin::launched();
    deepen_the_pool_in_window_0(&mut coin);
    coin.buy(BAND, 2 * ONE_NEAR, after(3));
    let at = window_1(0);
    assert_eq!(coin.account(BAND, at).class, "band");
    let held = coin.balance(BAND);
    coin.buy(BAND, ONE_NEAR / 100, at);
    let room = coin.account(BAND, at).penalty_applies_above.expect("band");
    // Sell to the edge of the first allowance plus the tokens bought: no penalty.
    let first = coin.sell(BAND, room, window_1(1));
    assert_eq!(first.penalty_near, 0);
    assert!(coin.balance(BAND) < held);
    let view = coin.account(BAND, window_1(2));
    assert!(view.balance <= view.unmarked.allowance);

    let rest = coin.sell_all(BAND, window_1(2));

    assert_eq!(rest.class, "band");
    assert_eq!(rest.fill, "full");
    assert!(rest.penalty_near > 0);
}

#[test]
fn after_the_escape_sequence_fc_account_reports_band_and_a_penalty_the_same_as_the_real_sell() {
    let mut coin = Coin::launched();
    deepen_the_pool_in_window_0(&mut coin);
    coin.buy(BAND, 2 * ONE_NEAR, after(3));
    let at = window_1(0);
    coin.buy(BAND, ONE_NEAR / 100, at);
    let room = coin.account(BAND, at).penalty_applies_above.expect("band");
    coin.sell(BAND, room, window_1(1));

    let view = coin.account(BAND, window_1(2));
    assert_eq!(view.class, "band");
    assert_eq!(view.penalty_applies_above, Some(0));
    assert_eq!(view.max_full_fill_now, 0);
    let quote = coin.quote(BAND, coin.balance(BAND), window_1(2));
    let sell = coin.sell_all(BAND, window_1(2));
    assert_eq!(sell.class, view.class);
    assert!(sell.penalty_near > 0);
    assert_eq!(quote.penalty_near, sell.penalty_near);
    assert_eq!(quote.near_out, sell.near_out);
    assert_eq!(quote.tokens_filled, sell.tokens_sold);
}

/// QUESTIONS.md Q11 item 3: a band bag below two allowances sold in two parts. The second
/// part is above the first allowance, so it pays the penalty.
#[test]
fn a_band_bag_split_into_two_sells_in_one_window_pays_the_penalty_on_the_second() {
    let mut coin = Coin::launched();
    deepen_the_pool_in_window_0(&mut coin);
    coin.buy(BAND, 2 * ONE_NEAR, after(3));
    let at = window_1(0);
    let view = coin.account(BAND, at);
    assert_eq!(view.class, "band");
    assert!(view.balance < 2 * view.unmarked.allowance);

    let first = coin.sell(BAND, view.unmarked.allowance, at);
    assert_eq!(first.penalty_near, 0);
    let second = coin.sell_all(BAND, window_1(1));

    assert_eq!(second.class, "band");
    assert_eq!(second.fill, "full");
    assert!(second.penalty_near > 0);
}

#[test]
fn a_small_account_that_buys_into_band_stays_band_for_the_rest_of_the_window() {
    let mut coin = Coin::launched();
    deepen_the_pool_in_window_0(&mut coin);
    coin.buy(SMALL, ONE_NEAR / 2, after(3));
    let at = window_1(0);
    assert_eq!(coin.account(SMALL, at).class, "small");
    coin.buy(SMALL, ONE_NEAR, at);
    assert_eq!(coin.account(SMALL, at).class, "band");
    // Send most of the bag away: the live balance is below one allowance again.
    coin.send(SMALL, OTHER, coin.balance(SMALL) * 3 / 4, window_1(1));
    let view = coin.account(SMALL, window_1(2));
    assert!(view.balance <= view.unmarked.allowance);
    assert_eq!(view.class, "band");
    assert_eq!(coin.sell_all(SMALL, window_1(2)).class, "band");
}

#[test]
fn the_class_floor_resets_in_the_next_window() {
    let mut coin = Coin::launched();
    deepen_the_pool_in_window_0(&mut coin);
    coin.buy(BAND, 2 * ONE_NEAR, after(3));
    let at = window_1(0);
    let allowance = coin.account(BAND, at).unmarked.allowance;
    coin.sell(BAND, allowance, at);
    assert_eq!(coin.account(BAND, window_1(1)).class, "band");

    let next = after(2 * HOUR_S);
    let view = coin.account(BAND, next);
    assert!(view.balance <= view.unmarked.allowance);
    assert_eq!(view.class, "small");
    assert_eq!(view.penalty_applies_above, None);
    assert_eq!(coin.sell_all(BAND, next).class, "small");
}
