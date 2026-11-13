//! Marked tokens (early supply), the creator group and the early window (ADR 0001, ADR 0008),
//! through the engine interface: `apply` and the views, at a creator fee share of 0.
//!
//! Tokens bought in blocks 0 to N (N = 17), bought by the creator group at any time, or received
//! by the creator group are marked. The creator group may hold at most 4% of supply. Marked
//! tokens leave first on a send, cannot be sold in blocks 0 to N, and share one allowance per
//! 24 h window, split pro rata by each account's marked balance just before its first touch in
//! the window, over all marked supply at the window start.

use std::collections::BTreeMap;

use rule_engine::{
    apply, launch, quote_buy, view_account, view_cap, view_early_window, view_pool, AccountView,
    Action, Applied, At, CoinState, Event, FcBuy, FcSell, FcSend, FixedValues, Holder, Holders,
    LaunchRequest, Refusal,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_BLOCK: u64 = 100;
const LAUNCH_NS: u64 = 100_000_000_000;
const SECOND_NS: u64 = 1_000_000_000;
const HOUR_S: u64 = 3_600;
const DAY_S: u64 = 86_400;
const SUPPLY: u128 = 1_000_000_000 * 1_000_000_000_000_000_000;

const CREATOR: &str = "alice.near";
const SUB_ACCOUNT: &str = "pool.alice.near";
const DEEP_SUB_ACCOUNT: &str = "one.pool.alice.near";
const LOOKALIKE: &str = "malice.near";
const EARLY: &str = "early.near";
const SECOND_EARLY: &str = "second-early.near";
const LATE: &str = "late.near";
const FRESH: &str = "fresh.near";

/// Block b of the coin (b = block height - launch height), b seconds after launch.
fn block(b: u64) -> At {
    At {
        block: LAUNCH_BLOCK + b,
        ns: LAUNCH_NS + b * SECOND_NS,
    }
}

/// `seconds` after launch, with one block per second: after the early window once `seconds` is
/// 18 or more.
fn after(seconds: u64) -> At {
    block(seconds)
}

/// One coin and its whole ledger, driven only through `apply` and the views.
#[derive(Clone)]
struct Coin {
    state: CoinState,
    ledger: BTreeMap<String, Holder>,
}

impl Coin {
    fn launched_by(creator: &str) -> Self {
        let state = launch(
            FixedValues::fixed(),
            LaunchRequest {
                coin: "coin.factory.near",
                caller: "factory.near",
                creator,
                code_hash: "hash",
                name: "Test Coin",
                symbol: "TEST",
                icon: None,
                description: None,
            },
            block(0),
        )
        .expect("launch")
        .state;
        Self {
            state,
            ledger: BTreeMap::new(),
        }
    }

    fn launched() -> Self {
        Self::launched_by(CREATOR)
    }

    fn holders(&self, accounts: &[String]) -> Holders {
        accounts
            .iter()
            .filter_map(|a| self.ledger.get(a).map(|h| (a.clone(), h.clone())))
            .collect()
    }

    fn try_act(&mut self, action: Action, at: At) -> Result<Applied, Refusal> {
        let applied = apply(&self.state, self.holders(&action.accounts()), &action, at)?;
        self.state = applied.state.clone();
        self.ledger.extend(applied.holders.clone());
        Ok(applied)
    }

    fn try_buy(&mut self, account: &str, near_in: u128, at: At) -> Result<FcBuy, Refusal> {
        let applied = self.try_act(
            Action::Buy {
                account: account.into(),
                near_in,
                min_tokens_out: 0,
            },
            at,
        )?;
        Ok(buy_event(&applied))
    }

    fn buy(&mut self, account: &str, near_in: u128, at: At) -> FcBuy {
        self.try_buy(account, near_in, at).expect("buy")
    }

    fn try_sell(&mut self, account: &str, amount: u128, at: At) -> Result<FcSell, Refusal> {
        let applied = self.try_act(
            Action::Sell {
                account: account.into(),
                amount,
                min_near_out: 0,
                attached: 1,
            },
            at,
        )?;
        Ok(sell_event(&applied))
    }

    fn sell(&mut self, account: &str, amount: u128, at: At) -> FcSell {
        self.try_sell(account, amount, at).expect("sell")
    }

    fn try_send(&mut self, from: &str, to: &str, amount: u128, at: At) -> Result<FcSend, Refusal> {
        self.ledger.entry(to.to_string()).or_default();
        let applied = self.try_act(
            Action::Send {
                from: from.into(),
                to: to.into(),
                amount,
                memo: None,
            },
            at,
        )?;
        Ok(applied
            .events
            .iter()
            .find_map(|e| match e {
                Event::FcSend(s) => Some(s.clone()),
                _ => None,
            })
            .expect("fc_send"))
    }

    fn send(&mut self, from: &str, to: &str, amount: u128, at: At) -> FcSend {
        self.try_send(from, to, amount, at).expect("send")
    }

    fn account(&self, account: &str, at: At) -> AccountView {
        view_account(&self.state, account, self.ledger.get(account), at)
    }

    fn balance(&self, account: &str) -> u128 {
        self.ledger.get(account).map_or(0, |h| h.balance)
    }
}

fn buy_event(applied: &Applied) -> FcBuy {
    applied
        .events
        .iter()
        .find_map(|e| match e {
            Event::FcBuy(b) => Some(b.clone()),
            _ => None,
        })
        .expect("fc_buy")
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

fn in_group(coin: &Coin, account: &str) -> bool {
    coin.account(account, after(30)).in_creator_group
}

#[test]
fn the_creator_group_is_the_creator_and_its_sub_accounts_on_the_dot_boundary() {
    let coin = Coin::launched();
    assert!(in_group(&coin, CREATOR));
    assert!(in_group(&coin, SUB_ACCOUNT));
    assert!(in_group(&coin, DEEP_SUB_ACCOUNT));
    // A plain suffix test is wrong: `malice.near` ends with `alice.near` but not with `.alice.near`.
    assert!(!in_group(&coin, LOOKALIKE));
    assert!(!in_group(&coin, "xalice.near"));
    assert!(!in_group(&coin, "alice.nearx"));
    assert!(!in_group(&coin, "alice.near.evil"));
    // The top-level account is never a parent in the group.
    assert!(!in_group(&coin, "near"));
    assert!(!in_group(&coin, LATE));
}

#[test]
fn the_creator_group_holds_the_parents_of_the_creator_below_the_top_level_account() {
    let coin = Coin::launched_by("bob.user.hot.tg");
    assert!(in_group(&coin, "bob.user.hot.tg"));
    assert!(in_group(&coin, "user.hot.tg"));
    assert!(in_group(&coin, "hot.tg"));
    assert!(!in_group(&coin, "tg"));
    // A sibling under the same parent is not in the group, nor is a sub-account of a parent.
    assert!(!in_group(&coin, "eve.user.hot.tg"));
    assert!(!in_group(&coin, "other.hot.tg"));
    assert!(!in_group(&coin, "ser.hot.tg"));
    // Sub-accounts of the creator are in the group.
    assert!(in_group(&coin, "x.bob.user.hot.tg"));
}

#[test]
fn a_buy_in_block_n_is_marked_and_a_buy_in_block_n_plus_one_is_not() {
    let mut coin = Coin::launched();
    let early = coin.buy(EARLY, ONE_NEAR / 10, block(17));
    assert!(early.tokens_out > 0);
    assert_eq!(early.marked_tokens, early.tokens_out);
    let late = coin.buy(LATE, ONE_NEAR / 10, block(18));
    assert_eq!(late.marked_tokens, 0);

    let early_view = coin.account(EARLY, block(19));
    assert_eq!(early_view.marked_balance, early.tokens_out);
    assert_eq!(early_view.unmarked_balance, 0);
    let late_view = coin.account(LATE, block(19));
    assert_eq!(late_view.marked_balance, 0);
    assert_eq!(late_view.unmarked_balance, late.tokens_out);
    assert_eq!(
        view_early_window(&coin.state, block(19)).early_supply_total,
        early.tokens_out
    );
}

#[test]
fn a_creator_group_buy_is_marked_at_any_time_and_its_quote_says_so() {
    let mut coin = Coin::launched();
    let at = after(2 * DAY_S);
    let quote = quote_buy(&coin.state, Holders::new(), SUB_ACCOUNT, ONE_NEAR, at).expect("quote");
    let bought = coin.buy(SUB_ACCOUNT, ONE_NEAR, at);
    assert_eq!(bought.marked_tokens, bought.tokens_out);
    assert_eq!(quote.marked_tokens, bought.tokens_out);
    let quote_late = quote_buy(&coin.state, Holders::new(), LATE, ONE_NEAR, at).expect("quote");
    assert_eq!(quote_late.marked_tokens, 0);
    let view = coin.account(SUB_ACCOUNT, at);
    assert!(view.in_creator_group);
    assert_eq!(view.marked_balance, bought.tokens_out);
    assert_eq!(view.unmarked_balance, 0);
}

/// floor(a x b / c) on 256-bit integers.
fn mul_div(a: u128, b: u128, c: u128) -> u128 {
    (ethnum::U256::from(a) * ethnum::U256::from(b) / ethnum::U256::from(c)).as_u128()
}

fn creator_cap() -> u128 {
    SUPPLY * 400 / 10_000
}

fn group_share_bps(coin: &Coin, at: At) -> u32 {
    view_pool(&coin.state, at).creator_group_share_bps
}

#[test]
fn a_creator_group_buy_that_takes_the_group_above_four_percent_is_refused_with_fc_creator_cap() {
    let mut coin = Coin::launched();
    let first = coin.buy(CREATOR, 4 * ONE_NEAR, after(20));
    assert!(first.tokens_out < creator_cap());
    // The cap is on the whole group, so a sub-account cannot buy around it.
    let refused = coin
        .try_buy(SUB_ACCOUNT, ONE_NEAR / 2, after(21))
        .expect_err("above the cap");
    assert_eq!(refused.to_string(), "FC_CREATOR_CAP");
    let refused = coin
        .try_buy(CREATOR, ONE_NEAR / 2, after(21))
        .expect_err("above the cap");
    assert_eq!(refused.to_string(), "FC_CREATOR_CAP");
    // A buy that keeps the group at or below 4% goes through.
    let small = coin.buy(SUB_ACCOUNT, ONE_NEAR / 100, after(22));
    assert!(first.tokens_out + small.tokens_out <= creator_cap());
    // A plain account is not held by the creator cap.
    coin.buy(LOOKALIKE, 5 * ONE_NEAR, after(23));
}

#[test]
fn a_send_into_the_creator_group_above_four_percent_is_refused_and_a_smaller_one_is_marked() {
    let mut coin = Coin::launched();
    coin.buy(CREATOR, 4 * ONE_NEAR, after(20));
    let late = coin.buy(LATE, ONE_NEAR, after(21));
    let room = creator_cap() - coin.balance(CREATOR);
    assert!(late.tokens_out > room);
    let refused = coin
        .try_send(LATE, SUB_ACCOUNT, room + 1, after(22))
        .expect_err("above the cap");
    assert_eq!(refused.to_string(), "FC_CREATOR_CAP");

    // Receives by the creator group are marked, even when the tokens left the sender unmarked.
    let sent = coin.send(LATE, SUB_ACCOUNT, room, after(23));
    assert_eq!(sent.marked_amount, 0);
    assert_eq!(sent.to_marked_amount, room);
    let receiver = coin.account(SUB_ACCOUNT, after(24));
    assert_eq!(receiver.marked_balance, room);
    assert_eq!(receiver.unmarked_balance, 0);
    assert_eq!(
        coin.balance(CREATOR) + coin.balance(SUB_ACCOUNT),
        creator_cap()
    );
    assert_eq!(group_share_bps(&coin, after(24)), 400);
}

#[test]
fn marked_tokens_leave_first_on_a_send_and_the_mark_follows_the_tokens() {
    let mut coin = Coin::launched();
    let marked = coin.buy(EARLY, ONE_NEAR / 10, block(1)).tokens_out;
    let unmarked = coin.buy(EARLY, ONE_NEAR / 10, after(30)).tokens_out;

    let first = coin.send(EARLY, FRESH, marked / 2, after(31));
    assert_eq!(first.marked_amount, marked / 2);
    assert_eq!(first.to_marked_amount, marked / 2);
    // The second send takes the rest of the marked tokens first, then unmarked ones.
    let rest = marked - marked / 2;
    let second = coin.send(EARLY, FRESH, rest + unmarked / 2, after(32));
    assert_eq!(second.marked_amount, rest);
    assert_eq!(second.to_marked_amount, rest);

    let sender = coin.account(EARLY, after(33));
    assert_eq!(sender.marked_balance, 0);
    assert_eq!(sender.unmarked_balance, unmarked - unmarked / 2);
    let receiver = coin.account(FRESH, after(33));
    assert_eq!(receiver.marked_balance, marked);
    assert_eq!(receiver.unmarked_balance, unmarked / 2);
    assert_eq!(
        view_early_window(&coin.state, after(33)).early_supply_total,
        marked
    );
}

#[test]
fn fc_pool_creator_group_share_follows_group_buys_sends_in_and_out_and_sells() {
    let mut coin = Coin::launched();
    assert_eq!(group_share_bps(&coin, after(20)), 0);
    let bought = coin.buy(CREATOR, 2 * ONE_NEAR, after(20)).tokens_out;
    let expected = |held: u128| (held * 10_000 / SUPPLY) as u32;
    assert_eq!(group_share_bps(&coin, after(20)), expected(bought));
    let late = coin.buy(LATE, ONE_NEAR / 2, after(21)).tokens_out;
    coin.send(LATE, SUB_ACCOUNT, late, after(22));
    assert_eq!(group_share_bps(&coin, after(22)), expected(bought + late));
    // A send inside the group does not change the group balance.
    coin.send(SUB_ACCOUNT, DEEP_SUB_ACCOUNT, late / 2, after(23));
    assert_eq!(group_share_bps(&coin, after(23)), expected(bought + late));
    // A send out of the group lowers it.
    coin.send(SUB_ACCOUNT, LATE, late - late / 2, after(24));
    assert_eq!(
        group_share_bps(&coin, after(24)),
        expected(bought + late / 2)
    );
    let sold = coin
        .sell(DEEP_SUB_ACCOUNT, late / 2, after(DAY_S + 10))
        .tokens_sold;
    assert!(sold > 0);
    assert_eq!(
        group_share_bps(&coin, after(DAY_S + 10)),
        expected(bought + late / 2 - sold)
    );
}

#[test]
fn the_early_buy_fee_is_fifty_percent_at_block_0_and_falls_each_block_to_one_percent_at_block_18() {
    let mut coin = Coin::launched();
    // 50% at b = 0.
    assert_eq!(coin.buy(EARLY, ONE_NEAR, block(0)).fee_near, ONE_NEAR / 2);
    // 1% + 49% x 1/18 at b = 17: on 18 NEAR that is 0.18 + 0.49 = 0.67 NEAR.
    assert_eq!(
        coin.buy(SECOND_EARLY, 18 * ONE_NEAR, block(17)).fee_near,
        670_000_000_000_000_000_000_000
    );
    // 1% from b = 18.
    assert_eq!(coin.buy(LATE, ONE_NEAR, block(18)).fee_near, ONE_NEAR / 100);
    assert_eq!(
        coin.buy(LATE, ONE_NEAR, block(500)).fee_near,
        ONE_NEAR / 100
    );
}

#[test]
fn fc_early_window_reports_the_buy_fee_now_and_the_block_it_ends() {
    let coin = Coin::launched();
    let at_0 = view_early_window(&coin.state, block(0));
    assert_eq!(at_0.buy_fee_bps_now, 5_000);
    assert_eq!(at_0.early_blocks, 17);
    assert_eq!(at_0.fee_end_block, LAUNCH_BLOCK + 18);
    // 1% + 49% / 18 = 372.2 bps, shown rounded up.
    assert_eq!(
        view_early_window(&coin.state, block(17)).buy_fee_bps_now,
        373
    );
    assert_eq!(
        view_early_window(&coin.state, block(9)).buy_fee_bps_now,
        2_550
    );
    assert_eq!(
        view_early_window(&coin.state, block(18)).buy_fee_bps_now,
        100
    );
}

#[test]
fn the_creator_pays_the_early_fee_too_and_its_share_is_of_the_normal_fee_only() {
    let mut early_coin = Coin::launched();
    let early = early_coin.buy(CREATOR, ONE_NEAR, block(0));
    assert_eq!(early.fee_near, ONE_NEAR / 2);
    let quote = quote_buy(
        &Coin::launched().state,
        Holders::new(),
        CREATOR,
        ONE_NEAR,
        block(0),
    )
    .expect("quote");
    assert_eq!(quote.fee_near, early.fee_near);
    assert_eq!(quote.tokens_out, early.tokens_out);
    // The whole NEAR in stays in the pool except the creator share.
    assert_eq!(
        view_pool(&early_coin.state, block(0)).real_near + early.creator_fee_near,
        ONE_NEAR
    );
    // The creator share does not grow with the early-fee extra: the same buy after the early
    // window gives the same creator share.
    let mut late_coin = Coin::launched();
    let late = late_coin.buy(CREATOR, ONE_NEAR, block(18));
    assert_eq!(early.creator_fee_near, late.creator_fee_near);
    assert!(early.creator_fee_near <= ONE_NEAR / 100);
}

/// 30 plain accounts buy 1 NEAR each, one per second from `from_second` (after the early
/// window), so the real NEAR cap has room (about 1.5 NEAR).
fn deepen_the_pool(coin: &mut Coin, from_second: u64) {
    for i in 0..30 {
        let buy = coin.buy(&format!("depth-{i}.near"), ONE_NEAR, after(from_second + i));
        assert_eq!(buy.marked_tokens, 0);
    }
}

fn shared_allowance(coin: &Coin, at: At) -> u128 {
    view_early_window(&coin.state, at).shared_allowance
}

fn cap_used(coin: &Coin, at: At) -> u128 {
    rule_engine::view_cap(&coin.state, at).cap_used
}

#[test]
fn marked_tokens_cannot_be_sold_in_blocks_0_to_n_and_the_first_sell_after_block_n_works() {
    let mut coin = Coin::launched();
    let bought = coin.buy(EARLY, ONE_NEAR / 5, block(1)).tokens_out;
    for b in [1, 5, 17] {
        let refused = coin
            .try_sell(EARLY, bought, block(b))
            .expect_err("early window");
        assert_eq!(refused.to_string(), "FC_EARLY_WINDOW");
    }
    let view = coin.account(EARLY, block(17));
    assert_eq!(view.marked.allowance, 0);
    assert_eq!(
        view_early_window(&coin.state, block(17)).shared_allowance,
        0
    );

    let sold = coin.sell(EARLY, bought, block(18));
    assert!(sold.tokens_sold > 0);
    assert_eq!(sold.marked_sold, sold.tokens_sold);
}

#[test]
fn no_buy_in_blocks_0_to_n_records_a_share_of_zero() {
    let mut coin = Coin::launched();
    let first = coin.buy(EARLY, ONE_NEAR / 10, block(0)).tokens_out;
    let last = coin.buy(SECOND_EARLY, ONE_NEAR / 10, block(17)).tokens_out;
    // The first action after block N records the shares and the marked supply at the start.
    coin.buy(LATE, ONE_NEAR, block(18));
    let total = first + last;
    for (account, held) in [(EARLY, first), (SECOND_EARLY, last)] {
        let view = coin.account(account, block(19));
        assert!(view.marked.allowance > 0, "{account} has a share of zero");
        assert_eq!(
            view.marked.share_bps.expect("share") as u128,
            held * 10_000 / total
        );
    }
}

#[test]
fn paper_case_e_a_three_percent_buy_in_block_2_sells_only_at_its_pro_rata_share_per_24_h() {
    let mut coin = Coin::launched();
    let held = coin.buy(EARLY, 6 * ONE_NEAR, block(2)).tokens_out;
    assert!(
        held > SUPPLY * 3 / 100 && held < SUPPLY * 35 / 1_000,
        "{held}"
    );
    let other = coin.buy(SECOND_EARLY, ONE_NEAR, block(3)).tokens_out;
    let refused = coin
        .try_sell(EARLY, held, block(10))
        .expect_err("early window");
    assert_eq!(refused.to_string(), "FC_EARLY_WINDOW");

    deepen_the_pool(&mut coin, 20);
    let at = after(60);
    let view = coin.account(EARLY, at);
    let shared = shared_allowance(&coin, at);
    assert!(shared > 0);
    assert_eq!(
        view.marked.share_bps.expect("share") as u128,
        held * 10_000 / (held + other)
    );
    assert_eq!(view.marked.allowance, mul_div(shared, held, held + other));
    assert_eq!(view.marked.window_end_ns, LAUNCH_NS + DAY_S * SECOND_NS);

    // A sell of the whole bag fills in part, up to the share; the rest stays in the wallet.
    let sold = coin.sell(EARLY, held, at);
    assert_eq!(sold.tokens_sold, view.marked.allowance);
    assert_eq!(sold.marked_sold, sold.tokens_sold);
    assert_eq!(sold.fill, "partial");
    assert_eq!(sold.limit_reason, Some("allowance"));
    assert_eq!(sold.penalty_near, 0);
    let early = view_early_window(&coin.state, at);
    assert_eq!(early.shared_allowance_used, sold.marked_sold);
    assert_eq!(early.shared_allowance_left, shared - sold.marked_sold);
    assert_eq!(early.window_end_ns, LAUNCH_NS + DAY_S * SECOND_NS);
    // The share is used for this 24 h window, also in the next 1 h windows.
    let again = coin.sell(EARLY, coin.balance(EARLY), after(5 * HOUR_S));
    assert_eq!(again.tokens_sold, 0);
    assert_eq!(again.fill, "none");
    assert_eq!(again.limit_reason, Some("allowance"));
    // The next 24 h window on the launch grid gives a new share.
    let next = coin.sell(EARLY, coin.balance(EARLY), after(DAY_S));
    assert!(next.tokens_sold > 0);
    assert_eq!(next.marked_sold, next.tokens_sold);
    // The next window's shared figures start again.
    let early = view_early_window(&coin.state, after(DAY_S));
    assert_eq!(early.shared_allowance_used, next.marked_sold);
    assert_eq!(early.window_end_ns, LAUNCH_NS + 2 * DAY_S * SECOND_NS);
}

#[test]
fn the_shared_marked_floor_is_split_by_balance_when_the_cap_is_full() {
    let mut coin = Coin::launched();
    let one = coin.buy(EARLY, ONE_NEAR / 10, block(1)).tokens_out;
    let two = coin.buy(SECOND_EARLY, ONE_NEAR / 5, block(1)).tokens_out;
    let total = one + two;
    // No other buys: the real NEAR is small, so the cap holds both sells at their floor. The
    // sells come in the next 1 h window, so the sellers' own buys do not offset them.
    let at = after(HOUR_S + 30);
    let shared_floor = (SUPPLY - total) * 10 / 10_000;
    for (account, held) in [(EARLY, one), (SECOND_EARLY, two)] {
        let view = coin.account(account, at);
        assert_eq!(view.marked.floor_left, mul_div(shared_floor, held, total));
        let sold = coin.sell(account, held, at);
        assert_eq!(sold.tokens_sold, view.marked.floor_left, "{account}");
        assert_eq!(sold.limit_reason, Some("floor"));
    }
}

#[test]
fn a_marked_token_counts_once_per_window_when_it_moves_after_the_first_touch() {
    let mut coin = Coin::launched();
    let held = coin.buy(EARLY, ONE_NEAR / 10, block(1)).tokens_out;
    let other = coin.buy(SECOND_EARLY, ONE_NEAR / 10, block(1)).tokens_out;
    deepen_the_pool(&mut coin, 20);
    let at = after(60);
    let shared = shared_allowance(&coin, at);
    let before = coin.account(EARLY, at).marked.allowance;
    assert_eq!(before, mul_div(shared, held, held + other));

    let sent = coin.send(EARLY, FRESH, held / 2, at);
    assert_eq!(sent.marked_amount, held / 2);
    // The sender keeps the share its balance at its first touch gave it.
    assert_eq!(coin.account(EARLY, after(61)).marked.allowance, before);
    // The receiver's first touch was the receive, with no marked balance before it.
    let fresh = coin.account(FRESH, after(61));
    assert_eq!(fresh.marked_balance, held / 2);
    assert_eq!(fresh.marked.allowance, 0);
    assert_eq!(fresh.marked.share_bps, Some(0));
    let refused_room = coin.sell(FRESH, held / 2, after(62));
    assert_eq!(refused_room.tokens_sold, 0);
    // A creator-group buy in the window adds marked supply but not to the base of the shares.
    let creator = coin.buy(CREATOR, ONE_NEAR, after(63)).tokens_out;
    assert_eq!(
        view_early_window(&coin.state, after(63)).early_supply_total,
        held + other + creator
    );
    assert_eq!(coin.account(EARLY, after(64)).marked.allowance, before);
    assert_eq!(coin.account(CREATOR, after(64)).marked.allowance, 0);

    // In the next window each account's share follows its balance at its first touch.
    let next = after(DAY_S + 10);
    coin.buy(LATE, ONE_NEAR / 100, next);
    let total = held + other + creator;
    let fresh_next = coin.account(FRESH, next);
    assert_eq!(
        fresh_next.marked.share_bps.expect("share") as u128,
        (held / 2) * 10_000 / total
    );
    assert!(fresh_next.marked.allowance > 0);
}

#[test]
fn the_marked_allowance_counts_gross_so_a_buy_gives_no_room_back() {
    let mut coin = Coin::launched();
    coin.buy(CREATOR, ONE_NEAR, block(1));
    // A large other marked holder makes the creator's share smaller than its balance.
    coin.buy(SECOND_EARLY, 3 * ONE_NEAR, block(1));
    deepen_the_pool(&mut coin, 20);
    let at = after(60);
    let share = coin.account(CREATOR, at).marked.allowance;
    let sold = coin.sell(CREATOR, coin.balance(CREATOR), at);
    assert_eq!(sold.tokens_sold, share);
    let bought = coin.buy(CREATOR, ONE_NEAR / 10, after(61));
    assert_eq!(bought.marked_tokens, bought.tokens_out);
    let view = coin.account(CREATOR, after(62));
    assert_eq!(view.marked.left, 0);
    assert_eq!(view.marked.used, share);
    // The marked buy is not unmarked room either.
    assert_eq!(view.unmarked.used, 0);
    let after_buy = coin.sell(CREATOR, bought.tokens_out, after(62));
    assert_eq!(after_buy.tokens_sold, 0);
    assert_eq!(after_buy.limit_reason, Some("allowance"));
}

#[test]
fn a_sell_fills_the_marked_part_up_to_its_share_then_the_unmarked_part() {
    let mut coin = Coin::launched();
    let marked = coin.buy(EARLY, ONE_NEAR / 10, block(1)).tokens_out;
    // A large other marked holder makes this account's share smaller than its marked balance.
    coin.buy(SECOND_EARLY, 3 * ONE_NEAR, block(1));
    deepen_the_pool(&mut coin, 20);
    let unmarked = coin.buy(EARLY, ONE_NEAR / 10, after(55)).tokens_out;
    let at = after(60);
    let view = coin.account(EARLY, at);
    assert_eq!(view.class, "small");
    assert_eq!(view.marked_balance, marked);
    assert_eq!(view.unmarked_balance, unmarked);
    assert!(view.marked.left > 0 && view.marked.left < marked);

    let sold = coin.sell(EARLY, marked + unmarked, at);
    assert_eq!(sold.marked_sold, view.marked.left);
    assert_eq!(sold.tokens_sold, view.marked.left + unmarked);
    assert_eq!(sold.fill, "partial");
    assert_eq!(sold.limit_reason, Some("allowance"));
    let left = coin.account(EARLY, after(61));
    assert_eq!(left.marked_balance, marked - view.marked.left);
    assert_eq!(left.unmarked_balance, 0);
}

#[test]
fn band_rules_apply_to_the_unmarked_part_only() {
    let mut coin = Coin::launched();
    let marked = coin.buy(EARLY, ONE_NEAR / 2, block(1)).tokens_out;
    coin.buy(SECOND_EARLY, 3 * ONE_NEAR, block(1));
    deepen_the_pool(&mut coin, 20);
    let unmarked = coin.buy(EARLY, 3 * ONE_NEAR / 2, after(55)).tokens_out;
    // Sell in the next 1 h window, so the unmarked buy gives no room back.
    let at = after(HOUR_S + 60);
    let view = coin.account(EARLY, at);
    assert_eq!(view.class, "band");
    assert!(unmarked > view.unmarked.allowance);
    let marked_left = view.marked.left;
    assert!(marked_left > 0 && marked_left < marked);
    // The penalty starts above the marked share plus the unmarked first allowance.
    let free = view.penalty_applies_above.expect("band");
    assert_eq!(free, marked_left + view.unmarked.allowance);
    let quote = |amount| {
        rule_engine::quote_sell(
            &coin.state,
            coin.holders(&[EARLY.to_string()]),
            EARLY,
            amount,
            at,
        )
        .expect("quote")
    };
    assert_eq!(quote(free).penalty_near, 0);
    assert!(quote(free + ONE_NEAR).penalty_near > 0);

    let sold = coin.sell(EARLY, marked + unmarked, at);
    // The marked part fills in part, at its share, with no penalty path; the unmarked part
    // fills in full, with the penalty above its first allowance.
    assert_eq!(sold.marked_sold, marked_left);
    assert_eq!(sold.tokens_sold, marked_left + unmarked);
    assert!(sold.penalty_near > 0);
    assert_eq!(sold.class, "band");
    assert_eq!(coin.account(EARLY, at).marked_balance, marked - marked_left);
}

#[test]
fn a_band_account_with_only_marked_tokens_gets_a_partial_fill_and_no_penalty() {
    let mut coin = Coin::launched();
    let held = coin.buy(EARLY, 3 * ONE_NEAR, block(1)).tokens_out;
    coin.buy(SECOND_EARLY, 3 * ONE_NEAR, block(1));
    deepen_the_pool(&mut coin, 20);
    let at = after(60);
    let view = coin.account(EARLY, at);
    assert_eq!(view.class, "band");
    let sold = coin.sell(EARLY, held, at);
    assert_eq!(sold.tokens_sold, view.marked.left);
    assert_eq!(sold.fill, "partial");
    assert_eq!(sold.penalty_near, 0);
}

#[test]
fn a_marked_round_trip_by_one_account_adds_the_marked_sell_to_the_cap() {
    let mut coin = Coin::launched();
    coin.buy(CREATOR, 2 * ONE_NEAR, block(1));
    deepen_the_pool(&mut coin, 20);
    let at = after(HOUR_S + 10);
    assert_eq!(cap_used(&coin, at), 0);
    let bought = coin.buy(CREATOR, ONE_NEAR / 5, at);
    let sold = coin.sell(CREATOR, bought.tokens_out, after(HOUR_S + 11));
    assert_eq!(sold.tokens_sold, bought.tokens_out);
    assert_eq!(sold.marked_sold, sold.tokens_sold);
    // Marked sells count gross: the buy does not offset them.
    assert_eq!(cap_used(&coin, after(HOUR_S + 11)), sold.near_out);
}

#[test]
fn a_creator_that_buys_in_one_group_account_and_sells_from_another_counts_the_sell_in_full() {
    let mut coin = Coin::launched();
    coin.buy(CREATOR, 2 * ONE_NEAR, block(1));
    deepen_the_pool(&mut coin, 20);
    let at = after(HOUR_S + 10);
    let bought = coin.buy(SUB_ACCOUNT, ONE_NEAR / 5, at);
    assert_eq!(bought.marked_tokens, bought.tokens_out);
    let sold = coin.sell(CREATOR, bought.tokens_out, after(HOUR_S + 11));
    assert_eq!(sold.tokens_sold, bought.tokens_out);
    assert!(sold.near_out > 0);
    assert_eq!(cap_used(&coin, after(HOUR_S + 11)), sold.near_out);
}

#[test]
fn splitting_at_launch_across_many_wallets_gains_nothing_over_one_wallet() {
    let one_wallet = {
        let mut coin = Coin::launched();
        coin.buy(EARLY, ONE_NEAR, block(1));
        coin.buy(SECOND_EARLY, 3 * ONE_NEAR, block(1));
        deepen_the_pool(&mut coin, 20);
        coin.account(EARLY, after(60)).marked.allowance
    };
    let mut coin = Coin::launched();
    let wallets: Vec<String> = (0..10).map(|i| format!("split-{i}.near")).collect();
    for wallet in &wallets {
        coin.buy(wallet, ONE_NEAR / 10, block(1));
    }
    coin.buy(SECOND_EARLY, 3 * ONE_NEAR, block(1));
    deepen_the_pool(&mut coin, 20);
    let split: u128 = wallets
        .iter()
        .map(|w| coin.account(w, after(60)).marked.allowance)
        .sum();
    assert!(split > 0);
    assert!(
        split <= one_wallet,
        "split {split} above one wallet {one_wallet}"
    );
}

#[test]
fn paper_case_6_bundlers_who_buy_after_the_cutoff_are_unmarked_a_known_limit() {
    let mut coin = Coin::launched();
    let bundle: Vec<String> = (0..10).map(|i| format!("bundle-{i}.near")).collect();
    for (i, wallet) in bundle.iter().enumerate() {
        let buy = coin.buy(wallet, ONE_NEAR / 2, block(18 + i as u64));
        assert_eq!(buy.marked_tokens, 0);
    }
    // Each wallet is small, so each sells its whole bag at once.
    for (i, wallet) in bundle.iter().enumerate() {
        let held = coin.balance(wallet);
        let sold = coin.sell(wallet, held, after(40 + i as u64));
        assert_eq!(sold.tokens_sold, held);
        assert_eq!(sold.marked_sold, 0);
        assert_eq!(sold.limit_reason, None);
    }
}

#[test]
fn paper_case_7_a_creator_buy_at_135_seconds_is_marked_and_waits_for_the_next_24_h_share() {
    let mut coin = Coin::launched();
    coin.buy(EARLY, ONE_NEAR / 10, block(1));
    deepen_the_pool(&mut coin, 20);
    let bought = coin.buy(CREATOR, 2 * ONE_NEAR, after(135));
    assert_eq!(bought.marked_tokens, bought.tokens_out);
    assert!(coin.balance(CREATOR) <= creator_cap());
    // Its first touch in window 0 was this buy, with no marked balance before it.
    let now = coin.sell(CREATOR, bought.tokens_out, after(200));
    assert_eq!(now.tokens_sold, 0);
    assert_eq!(now.limit_reason, Some("allowance"));
    let next = coin.sell(CREATOR, bought.tokens_out, after(DAY_S + 10));
    assert!(next.tokens_sold > 0);
    assert!(next.tokens_sold < bought.tokens_out);
    assert_eq!(next.marked_sold, next.tokens_sold);
}

#[test]
fn paper_case_8_a_creator_that_sells_in_the_first_minutes_is_refused_then_limited() {
    let mut coin = Coin::launched();
    let opening = coin.buy(CREATOR, 3 * ONE_NEAR, block(0));
    assert_eq!(opening.fee_near, 3 * ONE_NEAR / 2);
    for b in [1, 10, 17] {
        let refused = coin
            .try_sell(CREATOR, opening.tokens_out, block(b))
            .expect_err("early window");
        assert_eq!(refused.to_string(), "FC_EARLY_WINDOW");
    }
    coin.buy(EARLY, 2 * ONE_NEAR, block(5));
    deepen_the_pool(&mut coin, 20);
    let sold = coin.sell(CREATOR, opening.tokens_out, after(180));
    assert_eq!(sold.fill, "partial");
    assert_eq!(sold.marked_sold, sold.tokens_sold);
    assert!(sold.tokens_sold <= coin.account(CREATOR, after(180)).marked.allowance);
}

fn real_near(coin: &Coin, at: At) -> u128 {
    view_pool(&coin.state, at).real_near
}

/// The marked account sells all its marked tokens, so a resale of X fills only the X tokens it
/// bought in the window. They count toward the cap, net of the buy.
#[test]
fn a_marked_account_that_sells_buys_x_and_resells_x_counts_the_resale() {
    let mut coin = Coin::launched();
    let held = coin.buy(EARLY, ONE_NEAR / 10, block(2)).tokens_out;
    coin.buy(SECOND_EARLY, ONE_NEAR / 5, block(3));
    deepen_the_pool(&mut coin, 20);
    let at = after(HOUR_S + 10);
    let first = coin.sell(EARLY, held, at);
    assert_eq!(first.marked_sold, first.tokens_sold);
    assert!(first.tokens_sold > 0);
    assert_eq!(cap_used(&coin, at), first.near_out);

    let near_in = ONE_NEAR / 10;
    let bought = coin.buy(EARLY, near_in, after(HOUR_S + 11));
    assert_eq!(bought.marked_tokens, 0);
    assert_eq!(first.tokens_sold, held);
    assert_eq!(coin.account(EARLY, after(HOUR_S + 11)).marked_balance, 0);
    let resale = coin.sell(EARLY, bought.tokens_out, after(HOUR_S + 12));
    assert_eq!(resale.tokens_sold, bought.tokens_out);
    assert_eq!(resale.marked_sold, 0);
    let used = cap_used(&coin, after(HOUR_S + 12));
    // The marked sell counts gross; the resale counts net of the buy, at least 0.
    assert_eq!(
        used,
        first.near_out + resale.near_out.saturating_sub(near_in)
    );
    assert!(used * 10_000 <= 500 * real_near(&coin, after(HOUR_S + 12)));
}

#[test]
fn a_marked_account_with_unmarked_tokens_from_before_the_window_counts_a_resale_of_its_buy() {
    let mut coin = Coin::launched();
    let marked = coin.buy(EARLY, ONE_NEAR, block(2)).tokens_out;
    coin.buy(SECOND_EARLY, ONE_NEAR / 5, block(3));
    deepen_the_pool(&mut coin, 20);
    let unmarked = coin.buy(EARLY, ONE_NEAR / 5, after(60)).tokens_out;
    assert_eq!(coin.account(EARLY, after(60)).unmarked_balance, unmarked);
    let at = after(HOUR_S + 10);
    let view = coin.account(EARLY, at);
    let share = view.marked.left.min(view.marked_balance);
    let first = coin.sell(EARLY, share, at);
    assert_eq!(first.marked_sold, share);
    assert_eq!(first.tokens_sold, share);

    let near_in = ONE_NEAR / 10;
    let bought = coin.buy(EARLY, near_in, after(HOUR_S + 11));
    let resale = coin.sell(EARLY, bought.tokens_out, after(HOUR_S + 12));
    assert_eq!(resale.tokens_sold, bought.tokens_out);
    assert_eq!(resale.marked_sold, 0);
    assert_eq!(
        cap_used(&coin, after(HOUR_S + 12)),
        first.near_out + resale.near_out.saturating_sub(near_in)
    );
}

const OTHER: &str = "other.near";

/// EARLY holds marked tokens (block 1) and unmarked tokens (after block N) and is large; OTHER
/// fills the cap at the start of the next 1 h window.
fn large_marked_holder_at_a_full_cap() -> (Coin, At) {
    let mut coin = Coin::launched();
    coin.buy(EARLY, 2 * ONE_NEAR, block(1));
    coin.buy(EARLY, 3 * ONE_NEAR, after(20));
    coin.buy(OTHER, 3 * ONE_NEAR, after(21));
    let at = after(HOUR_S + 30);
    coin.sell(OTHER, coin.balance(OTHER), at);
    assert!(view_cap(&coin.state, at).is_full);
    assert_eq!(coin.account(EARLY, at).class, "large");
    (coin, at)
}

/// One floor per account per window (ADR 0005): the marked part of a sell uses up the unmarked
/// floor of the same sell, so one sell of everything fills the same tokens as a sell of the
/// marked part and then a sell of the rest.
#[test]
fn one_sell_and_two_sells_of_marked_then_unmarked_tokens_fill_the_same_at_a_full_cap() {
    let (coin, at) = large_marked_holder_at_a_full_cap();
    let marked = coin.account(EARLY, at).marked_balance;

    let mut one = coin.clone();
    let all = one.sell(EARLY, one.balance(EARLY), at);

    let mut two = coin.clone();
    let first = two.sell(EARLY, marked, at);
    let rest = two.sell(EARLY, two.balance(EARLY), at);

    assert!(first.marked_sold >= coin.account(EARLY, at).unmarked.floor_left);
    assert_eq!(rest.tokens_sold, 0);
    assert_eq!(all.marked_sold, first.marked_sold);
    assert_eq!(all.tokens_sold - all.marked_sold, 0);
    assert_eq!(all.tokens_sold, first.tokens_sold + rest.tokens_sold);
}

/// The largest amount whose real sell fills in full with no penalty, by bisection on the real
/// sell from `known_full` (which fills in full) up to the balance.
fn largest_real_full_fill(coin: &Coin, account: &str, known_full: u128, at: At) -> u128 {
    let fills_full = |amount: u128| {
        let mut trial = coin.clone();
        let sold = trial.sell(account, amount, at);
        sold.fill == "full" && sold.penalty_near == 0
    };
    let balance = coin.balance(account);
    if fills_full(balance) {
        return balance;
    }
    let (mut full, mut not_full) = (known_full, balance);
    while not_full - full > 1 {
        let middle = full + (not_full - full) / 2;
        if fills_full(middle) {
            full = middle;
        } else {
            not_full = middle;
        }
    }
    full
}

/// `max_full_fill_now` is the largest sell that fills in full now (INTERFACE), also for a large
/// account that holds marked tokens.
#[test]
fn max_full_fill_now_equals_the_largest_real_full_fill_for_a_large_marked_holder() {
    for (late_buy, buy_in_window) in [(0u128, ONE_NEAR / 10), (ONE_NEAR / 10, 0)] {
        let mut coin = Coin::launched();
        coin.buy(EARLY, 4 * ONE_NEAR, block(1));
        if late_buy > 0 {
            coin.buy(EARLY, late_buy, after(20));
        }
        let at = after(HOUR_S + 30);
        if buy_in_window > 0 {
            coin.buy(EARLY, buy_in_window, at);
        }
        let view = coin.account(EARLY, at);
        let mut trial = coin.clone();
        let at_view = trial.sell(EARLY, view.max_full_fill_now, at);
        assert_eq!(at_view.fill, "full");
        assert_eq!(at_view.penalty_near, 0);
        assert_eq!(
            view.max_full_fill_now,
            largest_real_full_fill(&coin, EARLY, view.max_full_fill_now, at),
            "late buy {late_buy}, buy in window {buy_in_window}"
        );
    }
}

/// `fc_early_window.blocks_left` counts the blocks of the early window still open, the current
/// block included: 18 in block 0, 1 in block N = 17 (still early: the early fee applies and
/// marked tokens cannot sell), 0 from block 18.
#[test]
fn blocks_left_counts_the_open_block_and_is_zero_from_block_n_plus_one() {
    let mut coin = Coin::launched();
    assert_eq!(view_early_window(&coin.state, block(0)).blocks_left, 18);
    assert_eq!(view_early_window(&coin.state, block(17)).blocks_left, 1);
    assert_eq!(view_early_window(&coin.state, block(18)).blocks_left, 0);
    let bought = coin.buy(EARLY, ONE_NEAR, block(17));
    assert!(bought.fee_near > ONE_NEAR / 100);
    let refused = coin
        .try_sell(EARLY, bought.tokens_out, block(17))
        .expect_err("early window");
    assert_eq!(refused.to_string(), "FC_EARLY_WINDOW");
}
