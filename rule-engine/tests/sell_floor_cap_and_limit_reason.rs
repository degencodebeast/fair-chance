//! Sell rules found by a state model check of the sell path: one floor per account per window,
//! the cap check on every sell, and the limit reason a sell reports.
//! Each test drives the engine through `apply` and the views only, at the fixed values.

use std::collections::BTreeMap;

use rule_engine::{
    apply, launch, view_account, view_cap, view_pool, AccountView, Action, Applied, At, CapView,
    CoinState, Event, FcSell, FixedValues, Holder, Holders, LaunchRequest,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const HOUR_S: u64 = 3_600;
const C_BPS: u128 = 500;
const BPS: u128 = 10_000;

const LARGE: &str = "large.test";
const SMALL: &str = "small.test";
const EARLY: &str = "early.test";
const SECOND_EARLY: &str = "second-early.test";
const SECOND_LARGE: &str = "second-large.test";

/// A block in the early window (blocks 0 to 17 after launch at block 100).
fn early_block(b: u64) -> At {
    At {
        block: 100 + b,
        ns: LAUNCH_NS + b * 1_000_000_000,
    }
}

/// Seconds after launch, after the early window.
fn after(seconds: u64) -> At {
    At {
        block: 100 + 18 + seconds,
        ns: LAUNCH_NS + seconds * 1_000_000_000,
    }
}

fn window_1(seconds: u64) -> At {
    after(HOUR_S + seconds)
}

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
}

fn within_cap(cap_used: u128, real_near_left: u128) -> bool {
    cap_used * BPS <= C_BPS * real_near_left
}

/// Window 0: LARGE 10 NEAR, SECOND_LARGE 3 NEAR, SMALL 0.5 NEAR. Window 1: LARGE sells all
/// (the cap stops it), then SMALL sells all, so the cap used is above c x real NEAR.
fn coin_with_a_cap_above_full_in_window_1() -> Coin {
    let mut coin = Coin::launched();
    coin.buy(LARGE, 10 * ONE_NEAR, after(10));
    coin.buy(SECOND_LARGE, 3 * ONE_NEAR, after(11));
    coin.buy(SMALL, ONE_NEAR / 2, after(13));
    let at = window_1(1);
    assert_eq!(
        coin.sell(LARGE, coin.balance(LARGE), at).limit_reason,
        Some("cap_full")
    );
    coin.sell(SMALL, coin.balance(SMALL), window_1(2));
    let cap = coin.cap(window_1(2));
    assert!(!within_cap(cap.cap_used, cap.real_near_now));
    coin
}

/// At a cap above full, a large account sells to its floor,
/// buys, and sells at its floor again in the same 1 h window.
#[test]
fn a_buy_gives_no_floor_room_back_in_the_same_window() {
    let mut coin = coin_with_a_cap_above_full_in_window_1();
    let at = window_1(3);
    let floor = coin.account(SECOND_LARGE, at).unmarked.floor_left;
    let first = coin.sell(SECOND_LARGE, coin.balance(SECOND_LARGE), at);
    let held = coin.balance(SECOND_LARGE);
    coin.buy(SECOND_LARGE, 3 * ONE_NEAR / 10, window_1(4));
    let bought = coin.balance(SECOND_LARGE) - held;
    let floor_after_buy = coin.account(SECOND_LARGE, window_1(4)).unmarked.floor_left;
    let second = coin.sell(SECOND_LARGE, coin.balance(SECOND_LARGE), window_1(5));
    let cap = coin.cap(window_1(5));
    println!(
        "floor {floor}; sell 1: sold {} reason {:?}; buy {bought} tokens; floor_left \
         after buy {floor_after_buy}; sell 2: sold {} reason {:?}; total floor fills {}; cap \
         used {} vs c x real {}",
        first.tokens_sold,
        first.limit_reason,
        second.tokens_sold,
        second.limit_reason,
        first.tokens_sold + second.tokens_sold,
        cap.cap_used,
        cap.real_near_now * C_BPS / BPS,
    );
    assert!(
        first.tokens_sold + second.tokens_sold <= floor,
        "two floors in one window: {} + {} > one floor {floor}",
        first.tokens_sold,
        second.tokens_sold
    );
}

/// EARLY buys 5 NEAR in the early window (marked, large), then 1 NEAR after it (unmarked).
/// LARGE 10 NEAR, SMALL 0.5 NEAR in window 0.
fn coin_with_a_large_marked_holder() -> Coin {
    let mut coin = Coin::launched();
    coin.buy(EARLY, 5 * ONE_NEAR, early_block(5));
    coin.buy(LARGE, 10 * ONE_NEAR, after(10));
    coin.buy(EARLY, ONE_NEAR, after(11));
    coin.buy(SMALL, ONE_NEAR / 2, after(13));
    let view = coin.account(EARLY, after(14));
    assert_eq!(view.class, "large");
    assert!(view.marked_balance > 0 && view.unmarked_balance > 0);
    coin
}

/// Marked and unmarked floors at a full cap with no buy.
#[test]
fn marked_and_unmarked_floors_do_not_stack_with_no_buy() {
    let mut coin = coin_with_a_large_marked_holder();
    let at = window_1(1);
    coin.sell(LARGE, coin.balance(LARGE), at);
    coin.sell(SMALL, coin.balance(SMALL), window_1(2));
    let at = window_1(3);
    let view = coin.account(EARLY, at);
    let (marked_floor, unmarked_floor) = (view.marked.floor_left, view.unmarked.floor_left);
    let first = coin.sell(EARLY, coin.balance(EARLY), at);
    let second = coin.sell(EARLY, coin.balance(EARLY), window_1(4));
    let total = first.tokens_sold + second.tokens_sold;
    println!(
        "marked floor {marked_floor}, unmarked floor {unmarked_floor}; sell 1: sold {} \
         (marked {}) reason {:?}; sell 2: sold {} (marked {}) reason {:?}; total {total}",
        first.tokens_sold,
        first.marked_sold,
        first.limit_reason,
        second.tokens_sold,
        second.marked_sold,
        second.limit_reason,
    );
    assert!(
        total <= marked_floor.max(unmarked_floor),
        "more than one floor with no buy: {total}"
    );
}

/// EARLY: small, 0.4 NEAR marked (bought in the early window), its marked floor used up in
/// window 0, and a 0.05 NEAR buy in window 1 (tokens bought this hour). SECOND_EARLY dilutes
/// its marked share. LARGE holds 10 NEAR of tokens.
fn coin_with_a_small_marked_holder_that_bought_this_hour() -> Coin {
    let mut base = Coin::launched();
    base.buy(EARLY, 4 * ONE_NEAR / 10, early_block(5));
    base.buy(SECOND_EARLY, 2 * ONE_NEAR, early_block(6));
    base.buy(LARGE, 10 * ONE_NEAR, after(10));
    // Window 0, after the early window: EARLY uses its marked floor while the cap has room.
    let marked_floor = base.account(EARLY, after(20)).marked.floor_left;
    let used = base.sell(EARLY, marked_floor + marked_floor / 2, after(20));
    assert_eq!(used.fill, "full");
    assert_eq!(base.account(EARLY, after(21)).marked.floor_left, 0);
    // Window 1: EARLY buys a little (tokens bought this hour); it stays small.
    base.buy(EARLY, ONE_NEAR / 20, window_1(1));
    let view = base.account(EARLY, window_1(1));
    assert_eq!(view.class, "small");
    assert!(view.marked.left > 0);
    base
}

/// A small account with marked tokens (its marked floor used up)
/// and tokens bought this hour sells both. Scans how full LARGE leaves the cap first.
#[test]
fn marked_part_of_a_small_sell_stays_within_the_cap() {
    let base = coin_with_a_small_marked_holder_that_bought_this_hour();
    let large_balance = base.balance(LARGE);

    let mut breaches = Vec::new();
    for pct in (2..=100).step_by(2) {
        let mut coin = base.clone();
        coin.sell(LARGE, large_balance * pct / 1000, window_1(2));
        let real_before = coin.real_near(window_1(3));
        let sell = coin.sell(EARLY, coin.balance(EARLY), window_1(3));
        let cap = coin.cap(window_1(3));
        let ok = within_cap(cap.cap_used, cap.real_near_now);
        println!(
            "scan {pct}: EARLY sold {} (marked {}) reason {:?}; cap used {} c x real {}",
            sell.tokens_sold,
            sell.marked_sold,
            sell.limit_reason,
            cap.cap_used,
            cap.real_near_now * C_BPS / BPS
        );
        if !ok && sell.marked_sold > 0 {
            breaches.push(format!(
                "LARGE sold {pct}/1000 of its bag; EARLY sold {} (marked {}), near_out {}, \
                 reason {:?}; real before {real_before}; cap used {} > c x real left {}",
                sell.tokens_sold,
                sell.marked_sold,
                sell.near_out,
                sell.limit_reason,
                cap.cap_used,
                cap.real_near_now * C_BPS / BPS
            ));
        }
    }
    for b in &breaches {
        println!("{b}");
    }
    assert!(breaches.is_empty(), "{} breaches", breaches.len());
}

/// A large account whose marked floor is used, at a full cap.
/// Its unmarked part fills exactly its floor and the cap stops the rest.
#[test]
fn large_sell_that_fills_only_its_unmarked_floor_reports_floor() {
    let mut coin = coin_with_a_large_marked_holder();
    // Window 0: EARLY uses its marked floor while the cap has room.
    let marked_floor = coin.account(EARLY, after(20)).marked.floor_left;
    let used = coin.sell(EARLY, marked_floor + marked_floor / 2, after(20));
    assert_eq!(used.fill, "full");
    // Window 1: cap above full.
    coin.sell(LARGE, coin.balance(LARGE), window_1(1));
    coin.sell(SMALL, coin.balance(SMALL), window_1(2));
    let at = window_1(3);
    let view = coin.account(EARLY, at);
    println!(
        "before: marked floor_left {}, marked left {}, unmarked floor_left {}",
        view.marked.floor_left, view.marked.left, view.unmarked.floor_left
    );
    let sell = coin.sell(EARLY, coin.balance(EARLY), at);
    println!(
        "class {}, sold {} (marked {}), fill {}, limit_reason {:?}",
        sell.class, sell.tokens_sold, sell.marked_sold, sell.fill, sell.limit_reason
    );
    assert_eq!(sell.class, "large");
    assert_eq!(sell.marked_sold, 0);
    assert_eq!(sell.tokens_sold, view.unmarked.floor_left);
    assert_eq!(
        sell.limit_reason,
        Some("floor"),
        "INTERFACE: a large sell that fills only its floor reports floor"
    );
}

/// A large sell whose marked part the cap stops and whose unmarked part, below its floor, fills
/// in full has not filled its floor: the cap stopped it, so the reason is `cap_full`.
#[test]
fn large_sell_whose_small_unmarked_part_fills_below_its_floor_reports_cap_full() {
    let mut coin = coin_with_a_large_marked_holder();
    let marked_floor = coin.account(EARLY, after(20)).marked.floor_left;
    coin.sell(EARLY, marked_floor + marked_floor / 2, after(20));
    coin.sell(LARGE, coin.balance(LARGE), window_1(1));
    coin.sell(SMALL, coin.balance(SMALL), window_1(2));
    let at = window_1(3);
    let view = coin.account(EARLY, at);
    assert_eq!(view.marked.floor_left, 0);
    let marked_part = view.marked_balance.min(view.marked.left);
    assert!(marked_part > 0);
    let unmarked_part = view.unmarked.floor_left / 2;
    let sell = coin.sell(EARLY, marked_part + unmarked_part, at);
    println!(
        "sold {} (marked {}), fill {}, limit_reason {:?}; unmarked floor_left {}",
        sell.tokens_sold, sell.marked_sold, sell.fill, sell.limit_reason, view.unmarked.floor_left
    );
    assert_eq!(sell.class, "large");
    assert_eq!(sell.marked_sold, 0);
    assert_eq!(sell.tokens_sold, unmarked_part);
    assert_eq!(sell.limit_reason, Some("cap_full"));
}

/// ADR 0005, INTERFACE rc.7: when the same sell also has a marked part and the cap is tight, the
/// bought part may fill less. The cap holds; here the bought part fills 0.
#[test]
fn a_bought_part_next_to_a_marked_part_may_fill_less_when_the_cap_is_tight() {
    let mut coin = coin_with_a_small_marked_holder_that_bought_this_hour();
    let large_balance = coin.balance(LARGE);
    coin.sell(LARGE, large_balance * 46 / 1000, window_1(2));
    let view = coin.account(EARLY, window_1(3));
    let sell = coin.sell(EARLY, coin.balance(EARLY), window_1(3));
    let cap = coin.cap(window_1(3));
    println!(
        "EARLY unmarked {} marked {}; sold {} (marked {}), reason {:?}",
        view.unmarked_balance,
        view.marked_balance,
        sell.tokens_sold,
        sell.marked_sold,
        sell.limit_reason
    );
    assert!(sell.marked_sold > 0 && sell.marked_sold < view.marked.left);
    assert_eq!(sell.tokens_sold, sell.marked_sold, "the bought part filled");
    assert!(within_cap(cap.cap_used, cap.real_near_now));
}

/// A second sell of the bought part, under the same tight cap, does not avoid this: a sell takes
/// its marked part first, so while marked tokens are left the second sell is the same mix.
#[test]
fn a_second_sell_of_the_bought_part_takes_marked_tokens_first_and_the_bought_part_still_waits() {
    let mut coin = coin_with_a_small_marked_holder_that_bought_this_hour();
    let large_balance = coin.balance(LARGE);
    coin.sell(LARGE, large_balance * 46 / 1000, window_1(2));
    let first = coin.sell(EARLY, coin.balance(EARLY), window_1(3));
    let view = coin.account(EARLY, window_1(4));
    let used_before = coin.cap(window_1(4)).cap_used;
    let second = coin.sell(EARLY, view.unmarked_balance, window_1(4));
    let used_after = coin.cap(window_1(4)).cap_used;
    println!(
        "first: sold {} (marked {}); before second: unmarked {} marked {} marked left {}; \
         second: sold {} (marked {}), reason {:?}; cap used {used_before} -> {used_after}",
        first.tokens_sold,
        first.marked_sold,
        view.unmarked_balance,
        view.marked_balance,
        view.marked.left,
        second.tokens_sold,
        second.marked_sold,
        second.limit_reason
    );
    assert!(view.marked.left > 0);
    assert_eq!(
        second.tokens_sold, second.marked_sold,
        "the bought part filled"
    );
    assert_eq!(second.limit_reason, Some("cap_full"));
    let cap = coin.cap(window_1(4));
    assert!(within_cap(cap.cap_used, cap.real_near_now));
}

/// One floor per window, the other order: the unmarked floor first, then the marked floor. An
/// account with marked tokens sends them away, buys to become large, sells its unmarked floor at
/// a full cap, gets the marked tokens back and sells them in the same 1 h window. One floor is
/// the larger of its unmarked floor and its marked floor left at the window's first sell.
#[test]
fn an_unmarked_floor_then_a_marked_floor_is_one_floor_in_one_window() {
    const PARKS: &str = "parks-marked.test";
    const KEEPER: &str = "keeper.test";
    let mut coin = Coin::launched();
    coin.buy(PARKS, ONE_NEAR / 10, early_block(5));
    coin.buy(KEEPER, ONE_NEAR / 100, after(5));
    let marked = coin.ledger[PARKS].marked_balance;
    assert!(marked > 0);
    coin.send(PARKS, KEEPER, marked, after(10));
    coin.buy(PARKS, 10 * ONE_NEAR, after(20));
    coin.buy(LARGE, 10 * ONE_NEAR, after(21));
    coin.buy(SMALL, ONE_NEAR / 2, after(22));
    assert!(coin.ledger[PARKS].is_large);
    assert_eq!(coin.ledger[PARKS].marked_balance, 0);
    // Window 1: the cap above full.
    coin.sell(LARGE, coin.balance(LARGE), window_1(1));
    coin.sell(SMALL, coin.balance(SMALL), window_1(2));
    let cap = coin.cap(window_1(2));
    assert!(!within_cap(cap.cap_used, cap.real_near_now));

    let view = coin.account(PARKS, window_1(3));
    let one_floor = view.unmarked.floor_left.max(view.marked.floor_left);
    let first = coin.sell(PARKS, coin.balance(PARKS), window_1(3));
    coin.send(KEEPER, PARKS, marked, window_1(4));
    let back = coin.account(PARKS, window_1(4));
    let second = coin.sell(PARKS, coin.balance(PARKS), window_1(5));
    println!(
        "floors: unmarked {} marked {}; sell 1: sold {} reason {:?}; after the marked tokens \
         come back: unmarked floor_left {} marked floor_left {}; sell 2: sold {} (marked {}) \
         reason {:?}",
        view.unmarked.floor_left,
        view.marked.floor_left,
        first.tokens_sold,
        first.limit_reason,
        back.unmarked.floor_left,
        back.marked.floor_left,
        second.tokens_sold,
        second.marked_sold,
        second.limit_reason
    );
    assert!(
        first.tokens_sold + second.tokens_sold <= one_floor,
        "two floors in one window: {} + {} > one floor {one_floor}",
        first.tokens_sold,
        second.tokens_sold
    );
}
