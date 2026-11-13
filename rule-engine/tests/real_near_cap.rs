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

/// Seconds after launch, as a block time.
fn after(seconds: u64) -> At {
    At {
        block: 100 + seconds,
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
        view_account(&self.state, self.ledger.get(account), at)
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
