//! Holder class (ADR 0001) and sends (NEP-141 with the Fair Chance refusals), through the engine
//! interface: `apply` and the views.
//!
//! Class: `small` when the balance fits in one allowance, `band` above that, `large` from the
//! buy that takes the balance to T = 2% of supply (2e25 base units of 1e27) until the balance
//! is zero. Only a buy makes an account large.
//!
//! Token amounts for a buy are worked out outside this code on the launch pool (100 NEAR
//! virtual, 1e27 tokens, 1% fee): a buy of n NEAR gives about 1e27 x 0.99n / (100 + 0.99n).

use std::collections::BTreeMap;

use rule_engine::{
    apply, launch, quote_buy, view_account, view_pool, AccountView, Action, Applied, At, CoinState,
    Event, FcSell, FixedValues, Holder, Holders, LaunchRequest, Refusal,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const HOUR_S: u64 = 3_600;
const COIN: &str = "coin.factory.test";
/// T = 2% of the 1e27 supply: the smallest large balance.
const LARGE_THRESHOLD: u128 = 20_000_000_000_000_000_000_000_000;

const BUYER: &str = "buyer.test";
const LARGE: &str = "large.test";
const SMALL: &str = "small.test";
const OTHER: &str = "other.test";

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
                coin: COIN,
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

    fn try_act(&mut self, action: Action, at: At) -> Result<Applied, Refusal> {
        let applied = apply(&self.state, self.holders(&action), &action, at)?;
        self.state = applied.state.clone();
        self.ledger.extend(applied.holders.clone());
        Ok(applied)
    }

    fn act(&mut self, action: Action, at: At) -> Applied {
        self.try_act(action, at).expect("action")
    }

    fn buy(&mut self, account: &str, near_in: u128, at: At) -> Applied {
        self.act(
            Action::Buy {
                account: account.into(),
                near_in,
                min_tokens_out: 0,
            },
            at,
        )
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
        applied
            .events
            .iter()
            .find_map(|e| match e {
                Event::FcSell(s) => Some(s.clone()),
                _ => None,
            })
            .expect("fc_sell")
    }

    fn send(&mut self, from: &str, to: &str, amount: u128, at: At) -> Result<Applied, Refusal> {
        self.try_act(
            Action::Send {
                from: from.into(),
                to: to.into(),
                amount,
                memo: None,
            },
            at,
        )
    }

    /// NEP-145 registration: an empty record for the account.
    fn register(&mut self, account: &str) {
        self.ledger
            .entry(account.to_string())
            .or_insert_with(|| Holder::registered(0));
    }

    fn account(&self, account: &str, at: At) -> AccountView {
        view_account(&self.state, self.ledger.get(account), at)
    }

    fn balance(&self, account: &str) -> u128 {
        self.ledger.get(account).map_or(0, |h| h.balance)
    }

    fn large_holders(&self, at: At) -> u32 {
        view_pool(&self.state, at).large_holders
    }
}

fn became_large(applied: &Applied) -> Vec<rule_engine::FcBecameLarge> {
    applied
        .events
        .iter()
        .filter_map(|e| match e {
            Event::FcBecameLarge(b) => Some(b.clone()),
            _ => None,
        })
        .collect()
}

fn class_after(applied: &Applied) -> &'static str {
    applied
        .events
        .iter()
        .find_map(|e| match e {
            Event::FcBuy(b) => Some(b.class_after),
            _ => None,
        })
        .expect("fc_buy")
}

#[test]
fn paper_case_e_a_three_percent_buy_is_large_at_once_and_cannot_send() {
    let mut coin = Coin::launched();
    let at = after(10);
    let quote = quote_buy(
        &coin.state,
        Holders::new(),
        BUYER,
        3_200 * ONE_NEAR / 1_000,
        at,
    )
    .expect("quote");

    let bought = coin.buy(BUYER, 3_200 * ONE_NEAR / 1_000, at);

    let balance = coin.balance(BUYER);
    assert!(balance * 100 >= 3 * 1_000_000_000 * 10u128.pow(18));
    assert_eq!(class_after(&bought), "large");
    assert_eq!(quote.class_after, "large");
    assert!(quote.becomes_large);
    let events = became_large(&bought);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].account, BUYER);
    assert_eq!(events[0].balance, balance);
    assert_eq!(events[0].large_threshold, LARGE_THRESHOLD);
    let view = coin.account(BUYER, after(20));
    assert_eq!(view.class, "large");
    assert!(view.is_large);
    assert!(!view.can_send);
    assert_eq!(view.max_receive, None);

    coin.register(OTHER);
    let refused = coin.send(BUYER, OTHER, 1, after(20)).unwrap_err();
    assert_eq!(refused.to_string(), "FC_LARGE_CANNOT_SEND");
}

fn refusal(result: Result<Applied, Refusal>) -> String {
    match result {
        Ok(_) => panic!("the send was not refused"),
        Err(refusal) => refusal.to_string(),
    }
}

/// SMALL holds about 0.1% (small), OTHER about 1.5% (band: above one allowance of about 1.02%).
fn coin_with_a_small_and_a_band_holder() -> Coin {
    let mut coin = Coin::launched();
    coin.buy(SMALL, ONE_NEAR / 10, after(10));
    coin.buy(OTHER, 1_500 * ONE_NEAR / 1_000, after(11));
    coin
}

#[test]
fn class_is_small_inside_one_allowance_and_band_above_it_and_a_send_keeps_band_until_the_window_ends(
) {
    let mut coin = coin_with_a_small_and_a_band_holder();
    let at = after(20);
    assert_eq!(coin.account(SMALL, at).class, "small");
    assert_eq!(coin.account(OTHER, at).class, "band");
    assert!(coin.account(OTHER, at).can_send);

    // The band account sends most of its bag: the receiver rises to band at once, and the
    // sender stays band (its class floor) until the window ends.
    let three_quarters = coin.balance(OTHER) * 3 / 4;
    coin.send(OTHER, SMALL, three_quarters, at).expect("send");
    assert_eq!(coin.account(OTHER, at).class, "band");
    assert_eq!(coin.account(SMALL, at).class, "band");
    let next_window = after(HOUR_S);
    assert_eq!(coin.account(OTHER, next_window).class, "small");
    assert_eq!(coin.account(SMALL, next_window).class, "band");
}

#[test]
fn a_send_moves_tokens_and_emits_ft_transfer_with_the_memo_and_fc_send() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    let amount = coin.balance(SMALL) / 2;
    let small_before = coin.balance(SMALL);
    let other_before = coin.balance(OTHER);

    let sent = coin
        .try_act(
            Action::Send {
                from: SMALL.into(),
                to: OTHER.into(),
                amount,
                memo: Some("thanks".into()),
            },
            after(20),
        )
        .expect("send");

    assert_eq!(coin.balance(SMALL), small_before - amount);
    assert_eq!(coin.balance(OTHER), other_before + amount);
    assert_eq!(sent.payout, None);
    assert_eq!(sent.state, coin.state);
    let lines: Vec<serde_json::Value> = sent
        .events
        .iter()
        .map(|e| {
            let line = e.log_line();
            serde_json::from_str(line.strip_prefix("EVENT_JSON:").expect("prefix")).expect("json")
        })
        .collect();
    assert_eq!(
        lines,
        vec![
            serde_json::json!({
                "standard": "nep141", "version": "1.0.0", "event": "ft_transfer",
                "data": [{
                    "old_owner_id": SMALL, "new_owner_id": OTHER,
                    "amount": amount.to_string(), "memo": "thanks"
                }]
            }),
            serde_json::json!({
                "standard": "fair_chance", "version": rule_engine::INTERFACE_VERSION,
                "event": "fc_send",
                "data": [{
                    "from": SMALL, "to": OTHER, "amount": amount.to_string(),
                    "marked_amount": "0", "to_marked_amount": "0"
                }]
            }),
        ]
    );
}

#[test]
fn a_send_uses_no_allowance_of_the_sender_or_the_receiver() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    let at = after(20);
    coin.send(OTHER, SMALL, coin.balance(OTHER) / 2, at)
        .expect("send");
    assert_eq!(coin.account(OTHER, at).unmarked.used, 0);
    assert_eq!(coin.account(SMALL, at).unmarked.used, 0);
}

#[test]
fn a_send_from_a_large_account_is_refused_with_fc_large_cannot_send() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    coin.buy(LARGE, 5 * ONE_NEAR, after(12));
    assert_eq!(
        refusal(coin.send(LARGE, SMALL, 1, after(20))),
        "FC_LARGE_CANNOT_SEND"
    );
}

#[test]
fn paper_case_f_a_send_that_would_take_the_receiver_to_two_percent_is_refused_with_the_largest_amount(
) {
    let mut coin = coin_with_a_small_and_a_band_holder();
    coin.buy(BUYER, ONE_NEAR, after(12));
    let at = after(20);
    let receiver = coin.balance(BUYER);
    let largest = LARGE_THRESHOLD - 1 - receiver;
    assert_eq!(coin.account(BUYER, at).max_receive, Some(largest));
    assert!(coin.balance(OTHER) > largest);

    assert_eq!(
        refusal(coin.send(OTHER, BUYER, largest + 1, at)),
        format!("FC_RECEIVER_WOULD_BE_LARGE: {largest}")
    );

    // The largest amount goes through and leaves the receiver one unit below T, not large.
    coin.send(OTHER, BUYER, largest, at).expect("largest send");
    assert_eq!(coin.balance(BUYER), LARGE_THRESHOLD - 1);
    assert_eq!(coin.account(BUYER, at).class, "band");
    assert_eq!(coin.account(BUYER, at).max_receive, Some(0));
    assert_eq!(coin.large_holders(at), 0);

    // Only a buy makes it large.
    let bought = coin.buy(BUYER, ONE_NEAR / 1_000, after(30));
    assert_eq!(became_large(&bought).len(), 1);
    assert_eq!(coin.account(BUYER, after(30)).class, "large");
    assert_eq!(coin.large_holders(after(30)), 1);
}

#[test]
fn a_send_to_an_account_that_is_already_large_is_allowed() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    coin.buy(LARGE, 5 * ONE_NEAR, after(12));
    let at = after(20);
    let before = coin.balance(LARGE);
    let amount = coin.balance(OTHER);

    coin.send(OTHER, LARGE, amount, at)
        .expect("send to a large account");

    assert_eq!(coin.balance(LARGE), before + amount);
    assert_eq!(coin.account(LARGE, at).class, "large");
    assert_eq!(coin.large_holders(at), 1);
}

#[test]
fn a_send_to_the_coin_is_refused_with_fc_send_to_coin() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    assert_eq!(
        refusal(coin.send(SMALL, COIN, 1, after(20))),
        "FC_SEND_TO_COIN"
    );
}

#[test]
fn a_send_to_yourself_is_refused_with_the_nep141_error() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    assert_eq!(
        refusal(coin.send(SMALL, SMALL, 1, after(20))),
        "Sender and receiver should be different"
    );
}

#[test]
fn a_send_of_zero_tokens_is_refused_with_the_nep141_error() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    assert_eq!(
        refusal(coin.send(SMALL, OTHER, 0, after(20))),
        "The amount should be a positive number"
    );
}

#[test]
fn a_send_above_the_balance_is_refused_with_the_nep141_error() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    let balance = coin.balance(SMALL);
    assert_eq!(
        refusal(coin.send(SMALL, OTHER, balance + 1, after(20))),
        "The account doesn't have enough balance"
    );
}

#[test]
fn a_send_to_an_account_with_no_storage_record_is_refused() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    assert_eq!(
        refusal(coin.send(SMALL, "fresh.test", 1, after(20))),
        "The account is not registered: fresh.test"
    );
    coin.register("fresh.test");
    coin.send(SMALL, "fresh.test", 1, after(20))
        .expect("send after registration");
    assert_eq!(coin.balance("fresh.test"), 1);
}

#[test]
fn becomes_large_is_true_exactly_for_the_buy_that_crosses_t() {
    let mut coin = Coin::launched();
    let steps = [
        (ONE_NEAR, false),      // about 0.98%: small
        (ONE_NEAR, false),      // about 1.94%: band, still below T
        (ONE_NEAR / 10, true),  // crosses T
        (ONE_NEAR / 10, false), // already large
    ];
    for (i, (near_in, crosses)) in steps.into_iter().enumerate() {
        let at = after(10 + i as u64);
        let holders: Holders = coin
            .ledger
            .get(BUYER)
            .map(|h| (BUYER.to_string(), h.clone()))
            .into_iter()
            .collect();
        let quote = quote_buy(&coin.state, holders, BUYER, near_in, at).expect("quote");
        let bought = coin.buy(BUYER, near_in, at);
        assert_eq!(quote.becomes_large, crosses, "step {i}");
        assert_eq!(
            became_large(&bought).len(),
            usize::from(crosses),
            "step {i}"
        );
        assert_eq!(quote.class_after, class_after(&bought), "step {i}");
        assert_eq!(
            quote.allowance_after,
            coin.account(BUYER, at).unmarked.allowance,
            "step {i}"
        );
        assert_eq!(coin.large_holders(at), u32::from(i >= 2), "step {i}");
    }
}

#[test]
fn a_large_account_that_sells_below_two_percent_stays_large_and_stays_held_to_its_allowance() {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 2_500 * ONE_NEAR / 1_000, after(10));
    deepen_the_pool_at_the_start_of_window_1(&mut coin);
    let window_1 = HOUR_S + 10;
    let allowance = coin.account(LARGE, after(window_1)).unmarked.allowance;

    let first = coin.sell(LARGE, allowance, after(window_1));
    assert_eq!(first.class, "large");
    assert_eq!(first.fill, "full");
    assert!(coin.balance(LARGE) < LARGE_THRESHOLD);

    // Below 2% now, but still large: the next sell in the window fills nothing, and no send.
    let view = coin.account(LARGE, after(window_1 + 1));
    assert_eq!(view.class, "large");
    assert!(!view.can_send);
    assert_eq!(view.max_full_fill_now, 0);
    let second = coin.sell(LARGE, coin.balance(LARGE), after(window_1 + 1));
    assert_eq!(second.class, "large");
    assert_eq!(second.tokens_sold, 0);
    assert_eq!(second.fill, "none");
    coin.register(OTHER);
    assert_eq!(
        refusal(coin.send(LARGE, OTHER, 1, after(window_1 + 2))),
        "FC_LARGE_CANNOT_SEND"
    );
    assert_eq!(coin.large_holders(after(window_1 + 2)), 1);
}

#[test]
fn fc_pool_large_holders_follows_each_class_change_and_a_large_account_is_small_at_zero_balance() {
    let mut coin = Coin::launched();
    assert_eq!(coin.large_holders(after(1)), 0);
    coin.buy(LARGE, 2_500 * ONE_NEAR / 1_000, after(10));
    assert_eq!(coin.large_holders(after(10)), 1);
    coin.buy(BUYER, 3 * ONE_NEAR, after(11));
    assert_eq!(coin.large_holders(after(11)), 2);
    deepen_the_pool_at_the_start_of_window_1(&mut coin);

    // LARGE sells its allowance in each window until its balance is zero.
    let mut window = 1;
    while coin.balance(LARGE) > 0 {
        let at = after(window * HOUR_S + 10);
        assert_eq!(coin.large_holders(at), 2, "window {window}");
        coin.sell(LARGE, coin.balance(LARGE), at);
        window += 1;
        assert!(window < 10, "LARGE never reached zero");
    }
    let at = after(window * HOUR_S);
    assert_eq!(coin.large_holders(at), 1);
    let view = coin.account(LARGE, at);
    assert_eq!(view.class, "small");
    assert!(!view.is_large);
    assert!(view.can_send);

    // From zero, a small buy is small and can send again.
    coin.buy(LARGE, ONE_NEAR / 10, at);
    coin.register(OTHER);
    coin.send(LARGE, OTHER, 1, at).expect("send after zero");
    assert_eq!(coin.account(LARGE, at).class, "small");
    assert_eq!(coin.large_holders(at), 1);
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

fn unregister(
    coin: &Coin,
    account: &str,
    force: bool,
    at: At,
) -> Result<rule_engine::Unregistered, String> {
    rule_engine::unregister(&coin.state, coin.ledger.get(account), force, at)
        .map_err(|r| r.to_string())
}

#[test]
fn an_unregister_while_the_account_holds_tokens_is_refused_with_fc_unregister_with_balance() {
    let coin = coin_with_a_small_and_a_band_holder();
    for force in [false, true] {
        assert_eq!(
            unregister(&coin, SMALL, force, after(20)),
            Err("FC_UNREGISTER_WITH_BALANCE".to_string()),
            "force {force}"
        );
    }
}

#[test]
fn an_unregister_with_tokens_and_an_open_window_is_refused_with_fc_unregister_with_balance() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    let at = after(HOUR_S + 10);
    coin.sell(SMALL, coin.balance(SMALL) / 2, at);
    assert!(coin.account(SMALL, at).unmarked.used > 0);
    for force in [false, true] {
        assert_eq!(
            unregister(&coin, SMALL, force, at),
            Err("FC_UNREGISTER_WITH_BALANCE".to_string()),
            "force {force}"
        );
    }
}

#[test]
fn an_account_that_sold_to_zero_cannot_unregister_until_the_window_ends() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    let at = after(HOUR_S + 10);
    coin.sell(SMALL, coin.balance(SMALL), at);
    assert_eq!(coin.balance(SMALL), 0);
    for force in [false, true] {
        assert_eq!(
            unregister(&coin, SMALL, force, at),
            Err("FC_UNREGISTER_WINDOW_OPEN".to_string()),
            "force {force}"
        );
        assert_eq!(
            unregister(&coin, SMALL, force, after(2 * HOUR_S - 1)),
            Err("FC_UNREGISTER_WINDOW_OPEN".to_string()),
            "force {force}, last second of the window"
        );
        assert_eq!(
            unregister(&coin, SMALL, force, after(2 * HOUR_S)),
            Ok(rule_engine::Unregistered::Removed { refund: 0 }),
            "force {force}, next window"
        );
    }
}

/// A buy and a sell back of the same tokens use no allowance (net), but the account still has
/// a cap record in the window, so the window stays open.
#[test]
fn an_account_with_no_used_allowance_but_a_cap_record_in_the_window_cannot_unregister() {
    let mut coin = Coin::launched();
    coin.register(OTHER);
    let at = after(20);
    coin.buy(OTHER, ONE_NEAR / 10, at);
    coin.sell(OTHER, coin.balance(OTHER), at);
    assert_eq!(coin.balance(OTHER), 0);
    assert_eq!(coin.account(OTHER, at).unmarked.used, 0);
    for force in [false, true] {
        assert_eq!(
            unregister(&coin, OTHER, force, at),
            Err("FC_UNREGISTER_WINDOW_OPEN".to_string()),
            "force {force}"
        );
        assert!(unregister(&coin, OTHER, force, after(HOUR_S)).is_ok());
    }
}

#[test]
fn an_unregister_with_no_tokens_and_no_used_allowance_refunds_the_storage_deposit() {
    let mut coin = Coin::launched();
    coin.ledger.insert(
        OTHER.into(),
        Holder::registered(1_250_000_000_000_000_000_000),
    );
    for force in [false, true] {
        assert_eq!(
            unregister(&coin, OTHER, force, after(20)),
            Ok(rule_engine::Unregistered::Removed {
                refund: 1_250_000_000_000_000_000_000
            })
        );
    }
    assert_eq!(
        unregister(&coin, "fresh.test", true, after(20)),
        Ok(rule_engine::Unregistered::NotRegistered)
    );
}

#[test]
fn max_receive_of_an_account_with_no_record_is_zero_because_a_send_to_it_is_refused() {
    let mut coin = coin_with_a_small_and_a_band_holder();
    let at = after(20);
    assert_eq!(coin.account("fresh.test", at).max_receive, Some(0));
    assert!(coin.send(SMALL, "fresh.test", 1, at).is_err());
    coin.register("fresh.test");
    assert_eq!(
        coin.account("fresh.test", at).max_receive,
        Some(LARGE_THRESHOLD - 1)
    );
}
