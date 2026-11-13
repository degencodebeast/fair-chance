//! Property tests on the rule engine (proptest): random sequences of buys, sells, sends and
//! window moves across many accounts, at a creator fee share of 0. Big buys make accounts band
//! or large, and sells take them back down, so classes change inside a sequence. Every sequence
//! starts at the launch block, so it holds early-window buys (blocks 0 to 17), and three of the
//! accounts are in the creator group (the creator, a sub-account and a parent), so marked
//! tokens are bought, received, sent and sold.
//!
//! After every step:
//! - A large sell that filled above its floor left, and a band sell that filled above its first
//!   allowance, leave the cap used at most c x the real NEAR left. Floors and band first
//!   allowances fill whatever the cap says (ADR 0005), so they are outside this check.
//! - No sell pays out NEAR that buyers did not put in: NEAR in = NEAR out + real NEAR + creator
//!   balance, and the NEAR reserve never falls below virtual P.
//! - The total supply never changes.
//! - `fc_quote_sell` equals the real sell.
//! - For a band account, `penalty_applies_above` equals `max_full_fill_now`.
//! - A small sell by an account that bought no unmarked tokens in the window never counts. A
//!   small sell of tokens bought in the window counts; it fills as far as it passes the cap
//!   check or adds nothing to the cap used. Sends never count, and no account's buys lower
//!   another account's counted sells: the cap used falls by at most the buyer's own part.
//! - Every holder record has the size of an empty record, so the storage deposit for a record
//!   pays for it whatever the account does.
//! - Marked tokens: the early supply total equals the marked balances held; the creator group
//!   never holds more than 4% of supply, and `fc_pool.creator_group_share_bps` follows it; a
//!   send moves marked tokens first, and every token the group receives is marked.
//! - In the marked property, also: each marked token counts once per 24 h window. The pro-rata
//!   shares of all accounts add up to at most the shared allowance, and so do the marked tokens
//!   sold in the window.
//!
//! In the cap check, the marked part of a sell is like a floor: up to the account's part of the
//! shared floor it fills whatever the cap says.
//!
//! A second property runs one account's buys, round trips and sells in one window after a
//! random history. The NEAR it gets for the tokens it held at the window start (its payouts
//! less the NEAR its buys paid) is at most what a twin run gets: the same trades with its buys
//! and buy-backs removed (the twin run already includes its floor), plus c x F, plus the price
//! effect of F (twin NEAR out x F / the lowest NEAR reserve in the window, virtual plus real),
//! plus 3 yoctoNEAR per trade for rounding (up to three roundings per trade). F is the fees the
//! pool kept from the account's buys. The account's class in the window never drops below its
//! class at the window start (the class floor), so the twin run sells in the same class.
//!
//! Each gate run tries 10,000 sequences for each of these two properties, and 1,000 for the
//! marked property, whose steps lean to the creator group, early-window buys, sends of marked
//! tokens and 24 h waits. A failing sequence is saved in
//! `proptest-regressions/real_near_cap_properties.txt` (committed), and runs first next time.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use rule_engine::{
    apply, launch, quote_sell, view_account, view_cap, view_early_window, view_pool, Action, At,
    CoinState, Event, FcBuy, FcSell, FcSend, FixedValues, Holder, Holders, LaunchRequest,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const ACCOUNTS: usize = 8;
const BPS: u128 = 10_000;
const CREATOR: &str = "creator.hub.test";
/// The creator group: the creator, a sub-account and a parent below the top-level account.
const CREATOR_GROUP: [&str; 3] = [CREATOR, "sub.creator.hub.test", "hub.test"];
/// The creator group may hold at most 4% of supply.
const CREATOR_CAP_BPS: u128 = 400;

#[derive(Debug, Clone)]
enum Step {
    Buy {
        account: usize,
        near_in: u128,
    },
    /// Sells `share_bps` of the balance (10_000 = all of it).
    Sell {
        account: usize,
        share_bps: u128,
    },
    Send {
        from: usize,
        to: usize,
        share_bps: u128,
    },
    /// Moves the block time on, often into a new 1 h window.
    Wait {
        seconds: u64,
    },
}

fn near_in() -> impl Strategy<Value = u128> {
    prop_oneof![
        0..1_000u128,
        ONE_NEAR / 100..2 * ONE_NEAR,
        2 * ONE_NEAR..40 * ONE_NEAR,
    ]
}

fn share_bps() -> impl Strategy<Value = u128> {
    prop_oneof![Just(10_000u128), 1..=10_000u128, 1..=300u128]
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => (0..ACCOUNTS, near_in()).prop_map(|(account, near_in)| Step::Buy { account, near_in }),
        4 => (0..ACCOUNTS, share_bps()).prop_map(|(account, share_bps)| Step::Sell { account, share_bps }),
        1 => (0..ACCOUNTS, 0..ACCOUNTS, share_bps()).prop_map(|(from, to, share_bps)| Step::Send { from, to, share_bps }),
        1 => prop_oneof![1..600u64, 3_000..7_300u64].prop_map(|seconds| Step::Wait { seconds }),
    ]
}

/// Account 0 is the coin's creator, 6 its sub-account and 7 its parent: the creator group. The
/// others are plain accounts.
fn account_name(index: usize) -> String {
    match index {
        0 => CREATOR_GROUP[0].to_string(),
        6 => CREATOR_GROUP[1].to_string(),
        7 => CREATOR_GROUP[2].to_string(),
        _ => format!("account-{index}.test"),
    }
}

/// One coin and its whole ledger, with the totals the properties compare against.
struct Coin {
    state: CoinState,
    ledger: BTreeMap<String, Holder>,
    at: At,
    near_in: u128,
    near_out: u128,
    /// Each account's part of the cap used in the current window, from the changes in
    /// `fc_cap.cap_used` that its own actions made.
    cap_part: BTreeMap<String, i128>,
    cap_window_start_ns: u64,
    /// Accounts that bought unmarked tokens in the current window.
    bought_in_window: BTreeSet<String>,
    /// Check, after every step, that each marked token counts once per 24 h window.
    check_marked_shares: bool,
}

impl Coin {
    fn launched() -> Self {
        let at = At {
            block: 100,
            ns: LAUNCH_NS,
        };
        let state = launch(
            FixedValues::fixed(),
            LaunchRequest {
                coin: "coin.factory.test",
                caller: "factory.test",
                creator: CREATOR,
                code_hash: "hash",
                name: "Test Coin",
                symbol: "TEST",
                icon: None,
                description: None,
            },
            at,
        )
        .expect("launch")
        .state;
        Self {
            state,
            ledger: BTreeMap::new(),
            at,
            near_in: 0,
            near_out: 0,
            cap_part: BTreeMap::new(),
            cap_window_start_ns: LAUNCH_NS,
            bought_in_window: BTreeSet::new(),
            check_marked_shares: false,
        }
    }

    fn holders(&self, accounts: &[String]) -> Holders {
        accounts
            .iter()
            .filter_map(|a| self.ledger.get(a).map(|h| (a.clone(), h.clone())))
            .collect()
    }

    fn balance(&self, account: &str) -> u128 {
        self.ledger.get(account).map_or(0, |h| h.balance)
    }

    fn cap_used(&self) -> u128 {
        view_cap(&self.state, self.at).cap_used
    }

    /// Runs one step and checks every property after it.
    fn run(&mut self, step: &Step) -> Result<(), TestCaseError> {
        self.at = At {
            block: self.at.block + 1,
            ns: self.at.ns + 1_000_000_000,
        };
        let window_start = view_cap(&self.state, self.at).window_start_ns;
        if window_start != self.cap_window_start_ns {
            self.cap_window_start_ns = window_start;
            self.cap_part.clear();
            self.bought_in_window.clear();
        }
        match *step {
            Step::Wait { seconds } => {
                self.at.ns += seconds * 1_000_000_000;
                self.at.block += seconds;
                Ok(())
            }
            Step::Buy { account, near_in } => {
                let account = account_name(account);
                let events = self.act_applied(
                    &account,
                    Action::Buy {
                        account: account.clone(),
                        near_in,
                        min_tokens_out: 0,
                    },
                )?;
                let bought = events.and_then(|events| {
                    events.into_iter().find_map(|e| match e {
                        Event::FcBuy(b) => Some(b),
                        _ => None,
                    })
                });
                if let Some(FcBuy {
                    tokens_out,
                    marked_tokens,
                    ..
                }) = bought
                {
                    if tokens_out > marked_tokens {
                        self.bought_in_window.insert(account);
                    }
                }
                Ok(())
            }
            Step::Sell { account, share_bps } => self.sell(&account_name(account), share_bps),
            Step::Send {
                from,
                to,
                share_bps,
            } => {
                let (from, to) = (account_name(from), account_name(to));
                let amount = self.balance(&from) * share_bps / BPS;
                // The receiver needs a storage record first.
                self.ledger.entry(to.clone()).or_default();
                let used_before = self.cap_used();
                let sender_marked = self.ledger.get(&from).map_or(0, |h| h.marked_balance);
                let to_in_group = CREATOR_GROUP.contains(&to.as_str());
                let events = self.act_applied(
                    &from,
                    Action::Send {
                        from: from.clone(),
                        to,
                        amount,
                        memo: None,
                    },
                )?;
                prop_assert_eq!(self.cap_used(), used_before, "a send changed the cap used");
                let sent = events.and_then(|events| {
                    events.into_iter().find_map(|e| match e {
                        Event::FcSend(s) => Some(s),
                        _ => None,
                    })
                });
                if let Some(sent) = sent {
                    marked_tokens_leave_first(&sent, sender_marked, to_in_group)?;
                }
                Ok(())
            }
        }
    }

    fn sell(&mut self, account: &str, share_bps: u128) -> Result<(), TestCaseError> {
        let amount = self.balance(account) * share_bps / BPS;
        let before = view_account(&self.state, account, self.ledger.get(account), self.at);
        if before.class == "band" {
            // Equal unless the cap holds back the marked part, which goes first.
            let penalty_applies_above = before.penalty_applies_above.expect("band");
            if before.marked_balance == 0 {
                prop_assert_eq!(penalty_applies_above, before.max_full_fill_now);
            } else {
                prop_assert!(penalty_applies_above >= before.max_full_fill_now);
            }
        }
        let quote = quote_sell(
            &self.state,
            self.holders(&[account.to_string()]),
            account,
            amount,
            self.at,
        );
        let used_before = self.cap_used();
        let (state_before, holders_before) =
            (self.state.clone(), self.holders(&[account.to_string()]));
        let Some(sell) = self.act(
            account,
            Action::Sell {
                account: account.to_string(),
                amount,
                min_near_out: 0,
                attached: 1,
            },
        )?
        else {
            prop_assert!(quote.is_none(), "a quote for a refused sell");
            return Ok(());
        };
        let quote = quote.expect("a quote for a sell that went through");
        prop_assert_eq!(quote.tokens_filled, sell.tokens_sold);
        prop_assert_eq!(quote.near_out, sell.near_out);
        prop_assert_eq!(quote.penalty_near, sell.penalty_near);
        prop_assert_eq!(quote.fill, sell.fill);
        prop_assert_eq!(quote.limit_reason, sell.limit_reason);
        prop_assert_eq!(quote.cap_left_after, sell.cap_left);

        let cap = view_cap(&self.state, self.at);
        prop_assert_eq!(sell.cap_used, cap.cap_used);
        prop_assert_eq!(sell.cap_left, cap.cap_left);
        // What fills whatever the cap says, at most: the marked part up to its part of the
        // shared floor, then the unmarked part up to its floor (large), its first allowance
        // room (band) or in full (small with no buy in the window).
        let bought_in_window = self.bought_in_window.contains(account);
        let marked_always = before.marked_balance.min(before.marked.floor_left);
        let marked_most = before.marked_balance.min(before.marked.left);
        let unmarked_always = match sell.class {
            "large" => before.unmarked_balance.min(before.unmarked.floor_left),
            "band" => before.penalty_applies_above.expect("band") - marked_most,
            _ if bought_in_window => 0,
            _ => before.unmarked_balance,
        };
        let above_what_always_fills = sell.tokens_sold > marked_always + unmarked_always;
        // A small sell of bought tokens also fills while its unmarked part adds nothing to the
        // cap used: compare with a sell of its marked part alone.
        let small_added_nothing = sell.class == "small" && bought_in_window && {
            let used_by_marked_part = if sell.marked_sold == 0 {
                used_before
            } else {
                let marked_only = apply(
                    &state_before,
                    holders_before,
                    &Action::Sell {
                        account: account.to_string(),
                        amount: sell.marked_sold,
                        min_near_out: 0,
                        attached: 1,
                    },
                    self.at,
                )
                .expect("the marked part alone sells");
                view_cap(&marked_only.state, self.at).cap_used
            };
            cap.cap_used <= used_by_marked_part
        };
        if above_what_always_fills && !small_added_nothing {
            prop_assert!(
                cap.cap_used * BPS <= cap.real_near_now * self.state.fixed.real_cap_c_bps as u128,
                "cap used {} above c x real NEAR left {} after a {} sell",
                cap.cap_used,
                cap.real_near_now,
                sell.class
            );
        }
        if sell.class == "small" && sell.marked_sold == 0 && !bought_in_window {
            prop_assert_eq!(cap.cap_used, used_before, "a small sell counted");
        }
        Ok(())
    }

    /// Applies one action and checks the properties every action keeps. A refused action
    /// changes nothing. Returns the `fc_sell` of a sell.
    fn act(&mut self, by: &str, action: Action) -> Result<Option<FcSell>, TestCaseError> {
        Ok(self.act_applied(by, action)?.and_then(|events| {
            events.into_iter().find_map(|e| match e {
                Event::FcSell(s) => Some(s),
                _ => None,
            })
        }))
    }

    /// Applies one action and checks the properties every action keeps. A refused action
    /// changes nothing. Returns the events of an action that went through.
    fn act_applied(
        &mut self,
        by: &str,
        action: Action,
    ) -> Result<Option<Vec<Event>>, TestCaseError> {
        let used_before = self.cap_used();
        let Ok(applied) = apply(
            &self.state,
            self.holders(&action.accounts()),
            &action,
            self.at,
        ) else {
            return Ok(None);
        };
        if let Action::Buy { near_in, .. } = action {
            self.near_in += near_in;
        }
        if let Some(payout) = &applied.payout {
            self.near_out += payout.amount;
        }
        self.state = applied.state.clone();
        self.ledger.extend(applied.holders.clone());

        let pool = view_pool(&self.state, self.at);
        let held: u128 = self.ledger.values().map(|h| h.balance).sum();
        prop_assert_eq!(held + pool.token_reserve, self.state.fixed.total_supply);
        prop_assert!(
            self.near_out <= self.near_in,
            "paid out more than buyers put in"
        );
        prop_assert_eq!(
            self.near_in,
            self.near_out + pool.real_near + pool.creator_fee_balance
        );
        prop_assert!(pool.near_reserve >= self.state.fixed.virtual_near);

        let used_after = self.cap_used();
        let part = self.cap_part.entry(by.to_string()).or_insert(0);
        *part += used_after as i128 - used_before as i128;
        prop_assert!(
            *part >= 0,
            "{by}'s action took the cap used below what other accounts' sells count"
        );
        let cap = view_cap(&self.state, self.at);
        prop_assert_eq!(cap.is_full, cap.cap_left == 0);
        prop_assert_eq!(
            cap.cap_total,
            pool.real_near * self.state.fixed.real_cap_c_bps as u128 / BPS
        );

        let empty_record = borsh::to_vec(&Holder::default()).expect("borsh").len();
        for holder in self.ledger.values() {
            prop_assert_eq!(borsh::to_vec(holder).expect("borsh").len(), empty_record);
        }
        self.check_marked_tokens()?;
        Ok(Some(applied.events))
    }

    /// The marked-token properties after one step.
    fn check_marked_tokens(&self) -> Result<(), TestCaseError> {
        let supply = self.state.fixed.total_supply;
        let early = view_early_window(&self.state, self.at);
        let marked_held: u128 = self.ledger.values().map(|h| h.marked_balance).sum();
        prop_assert_eq!(early.early_supply_total, marked_held);
        for holder in self.ledger.values() {
            prop_assert!(holder.marked_balance <= holder.balance);
        }
        let group_held: u128 = CREATOR_GROUP.iter().map(|a| self.balance(a)).sum();
        prop_assert!(
            group_held * BPS <= supply * CREATOR_CAP_BPS,
            "the creator group holds {group_held}, above 4% of supply"
        );
        prop_assert_eq!(
            view_pool(&self.state, self.at).creator_group_share_bps as u128,
            group_held * BPS / supply
        );
        for account in CREATOR_GROUP {
            if let Some(holder) = self.ledger.get(account) {
                prop_assert_eq!(
                    holder.marked_balance,
                    holder.balance,
                    "{} holds unmarked",
                    account
                );
            }
        }
        if self.check_marked_shares {
            // Each marked token counts once per window: the shares and the marked tokens sold
            // in the window each add up to at most the shared allowance.
            let (mut shares, mut sold) = (0u128, 0u128);
            for account in self.ledger.keys() {
                let view = view_account(&self.state, account, self.ledger.get(account), self.at);
                prop_assert!(
                    view.marked.used <= view.marked.allowance,
                    "{} sold above its share",
                    account
                );
                shares += view.marked.allowance;
                sold += view.marked.used;
            }
            prop_assert!(
                shares <= early.shared_allowance,
                "shares {shares} above the shared allowance {}",
                early.shared_allowance
            );
            prop_assert!(
                sold <= early.shared_allowance,
                "marked sold {sold} above the shared allowance {}",
                early.shared_allowance
            );
        }
        Ok(())
    }
}

/// A send moves the sender's marked tokens first, and every token the creator group receives is
/// marked.
fn marked_tokens_leave_first(
    sent: &FcSend,
    sender_marked: u128,
    to_in_group: bool,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(sent.marked_amount, sent.amount.min(sender_marked));
    let to_marked = if to_in_group {
        sent.amount
    } else {
        sent.marked_amount
    };
    prop_assert_eq!(sent.to_marked_amount, to_marked);
    Ok(())
}

#[derive(Debug, Clone)]
enum WindowTrade {
    Buy {
        near_in: u128,
    },
    /// Sells `share_bps` of the balance at that moment (10_000 = all of it).
    Sell {
        share_bps: u128,
    },
}

fn window_trade() -> impl Strategy<Value = WindowTrade> {
    prop_oneof![
        near_in().prop_map(|near_in| WindowTrade::Buy { near_in }),
        share_bps().prop_map(|share_bps| WindowTrade::Sell { share_bps }),
    ]
}

/// What one account's trades in one window gave it.
#[derive(Debug, Default)]
struct WindowResult {
    payouts: u128,
    buys: u128,
    buy_fees: u128,
    trades: u128,
    lowest_near_reserve: u128,
    /// The actions that went through, in order.
    actions: Vec<Action>,
}

/// Runs `actions` by one account from `start`, one second apart, on a copy of the coin. A
/// sell asks for at most the balance; a refused action is left out.
fn run_window(coin: &Coin, account: &str, actions: Vec<Action>, start: At) -> WindowResult {
    let mut state = coin.state.clone();
    let mut ledger = coin.ledger.clone();
    let mut result = WindowResult {
        lowest_near_reserve: state.near_reserve(),
        ..WindowResult::default()
    };
    for (i, action) in actions.into_iter().enumerate() {
        let at = At {
            block: start.block + i as u64,
            ns: start.ns + i as u64 * 1_000_000_000,
        };
        let balance = ledger.get(account).map_or(0, |h| h.balance);
        let action = match action {
            Action::Sell { amount, .. } => Action::Sell {
                account: account.to_string(),
                amount: amount.min(balance),
                min_near_out: 0,
                attached: 1,
            },
            other => other,
        };
        let holders: Holders = ledger
            .get(account)
            .map(|h| (account.to_string(), h.clone()))
            .into_iter()
            .collect();
        let Ok(applied) = apply(&state, holders, &action, at) else {
            continue;
        };
        for event in &applied.events {
            if let Event::FcBuy(buy) = event {
                result.buys += buy.near_in;
                result.buy_fees += buy.fee_near;
            }
        }
        result.payouts += applied.payout.as_ref().map_or(0, |p| p.amount);
        result.trades += 1;
        state = applied.state;
        ledger.extend(applied.holders);
        result.lowest_near_reserve = result.lowest_near_reserve.min(state.near_reserve());
        result.actions.push(action);
    }
    result
}

/// The account's real trades: a sell asks for its share of the balance at that moment.
fn real_window_actions(
    coin: &Coin,
    account: &str,
    trades: &[WindowTrade],
    start: At,
) -> WindowResult {
    let mut actions = Vec::new();
    for trade in trades {
        // Sell amounts depend on the balance, so run the prefix to learn it.
        let action = match *trade {
            WindowTrade::Buy { near_in } => Action::Buy {
                account: account.to_string(),
                near_in,
                min_tokens_out: 0,
            },
            WindowTrade::Sell { share_bps } => {
                let so_far = run_window(coin, account, actions.clone(), start);
                let mut ledger = coin.ledger.clone();
                let mut state = coin.state.clone();
                for (i, action) in so_far.actions.iter().enumerate() {
                    let at = At {
                        block: start.block + i as u64,
                        ns: start.ns + i as u64 * 1_000_000_000,
                    };
                    let holders: Holders = ledger
                        .get(account)
                        .map(|h| (account.to_string(), h.clone()))
                        .into_iter()
                        .collect();
                    let applied = apply(&state, holders, action, at).expect("replay");
                    state = applied.state;
                    ledger.extend(applied.holders);
                }
                let balance = ledger.get(account).map_or(0, |h| h.balance);
                Action::Sell {
                    account: account.to_string(),
                    amount: balance * share_bps / BPS,
                    min_near_out: 0,
                    attached: 1,
                }
            }
        };
        actions.push(action);
    }
    run_window(coin, account, actions, start)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/real_near_cap_properties.txt",
        ))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn random_multi_account_sequences_keep_the_cap_and_pool_properties(
        steps in proptest::collection::vec(step(), 1..60)
    ) {
        let mut coin = Coin::launched();
        for step in &steps {
            coin.run(step)?;
        }
    }

    #[test]
    fn one_account_s_buys_in_a_window_gain_it_no_near_for_the_tokens_it_held_before(
        history in proptest::collection::vec(step(), 0..40),
        who in 0..ACCOUNTS,
        trades in proptest::collection::vec(window_trade(), 1..16),
    ) {
        twin_bound_holds(&history, who, &trades)?;
    }
}

/// Buys small enough that the creator group can buy several times before its 4% cap.
fn marked_near_in() -> impl Strategy<Value = u128> {
    prop_oneof![
        0..1_000u128,
        ONE_NEAR / 100..ONE_NEAR,
        ONE_NEAR..4 * ONE_NEAR,
    ]
}

/// Steps that lean to the creator group (accounts 0, 6 and 7), early-window buys (short waits
/// keep the first steps in blocks 0 to 17), sends of marked tokens and 24 h waits.
fn marked_step() -> impl Strategy<Value = Step> {
    let account = prop_oneof![2 => prop_oneof![Just(0usize), Just(6), Just(7)], 3 => 0..ACCOUNTS];
    prop_oneof![
        3 => (account.clone(), marked_near_in()).prop_map(|(account, near_in)| Step::Buy { account, near_in }),
        3 => (account.clone(), share_bps()).prop_map(|(account, share_bps)| Step::Sell { account, share_bps }),
        3 => (account.clone(), account, share_bps()).prop_map(|(from, to, share_bps)| Step::Send { from, to, share_bps }),
        1 => prop_oneof![1..5u64, 3_000..7_300u64, 80_000..90_000u64].prop_map(|seconds| Step::Wait { seconds }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1_000,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/real_near_cap_properties.txt",
        ))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn random_sequences_with_the_creator_group_and_early_buys_keep_the_marked_properties(
        steps in proptest::collection::vec(marked_step(), 1..60)
    ) {
        let mut coin = Coin::launched();
        coin.check_marked_shares = true;
        for step in &steps {
            coin.run(step)?;
        }
    }
}

/// Rounding slack per trade in the twin bound: each trade has up to three roundings (NEAR out,
/// fee, penalty). Each one rounds toward the pool, but the real run and the twin run round on
/// different bases, so each can differ by up to 1 yoctoNEAR per rounding.
const ROUNDING_YOCTO_PER_TRADE: u128 = 3;

/// The twin-run property for one history, one account and its trades in the next window.
fn twin_bound_holds(
    history: &[Step],
    who: usize,
    trades: &[WindowTrade],
) -> Result<(), TestCaseError> {
    let mut coin = Coin::launched();
    for step in history {
        coin.run(step)?;
    }
    let account = account_name(who);
    let window_end = view_cap(&coin.state, coin.at).window_end_ns;
    // The window trades start after the early window (blocks 0 to N), so the real run and the
    // twin run, whose trades sit at different block heights, sell under the same rules.
    let first_block_after_the_early_window =
        coin.state.launch_block + coin.state.fixed.early_blocks + 1;
    let start = At {
        block: (coin.at.block + 10).max(first_block_after_the_early_window),
        ns: window_end,
    };

    let real = real_window_actions(&coin, &account, trades, start);
    // The twin: the same trades with every buy (and so every buy-back) removed.
    let twin_actions = real
        .actions
        .iter()
        .filter(|a| !matches!(a, Action::Buy { .. }))
        .cloned()
        .collect();
    let twin = run_window(&coin, &account, twin_actions, start);

    let fees = real.buy_fees;
    let cap_room = (fees * coin.state.fixed.real_cap_c_bps as u128).div_ceil(BPS);
    let lowest = ethnum::U256::from(real.lowest_near_reserve);
    let price_effect = ((ethnum::U256::from(twin.payouts) * ethnum::U256::from(fees) + lowest - 1)
        / lowest)
        .as_u128();
    let bound = twin.payouts + cap_room + price_effect + real.trades * ROUNDING_YOCTO_PER_TRADE;
    let got = real.payouts as i128 - real.buys as i128;
    prop_assert!(
        got <= bound as i128,
        "{account} got {got} for its held tokens; twin {} + c x F {cap_room} + price {price_effect} + trades {}; real actions {:?}",
        twin.payouts, real.trades, real.actions
    );
    Ok(())
}

/// A saved case from the random runs: a buy, then two sells, where the real run and the twin
/// run round on different bases. It was found before marked tokens existed, so it starts after
/// the early window, and the buys that the creator made then are made by a plain account
/// (account 1): the same trades, all unmarked, as when it was found.
#[test]
fn the_twin_bound_holds_for_a_buy_then_two_sells_that_round_on_different_bases() {
    let history = [
        Step::Wait { seconds: 20 },
        Step::Buy {
            account: 5,
            near_in: 758_980_140_491_974_805_048_750,
        },
        Step::Buy {
            account: 1,
            near_in: 6_920_531_327_738_845_529_101_183,
        },
        Step::Buy {
            account: 1,
            near_in: 15_772_612_238_901_679_912_002_747,
        },
        Step::Buy {
            account: 1,
            near_in: 23_630_972_305_690_285_432_213_539,
        },
        Step::Buy {
            account: 1,
            near_in: 11_263_261_550_640_298_493_450_627,
        },
    ];
    let trades = [
        WindowTrade::Buy {
            near_in: 673_560_730_771_697_173_802_353,
        },
        WindowTrade::Sell { share_bps: 6_922 },
        WindowTrade::Sell { share_bps: 10_000 },
    ];
    twin_bound_holds(&history, 5, &trades).expect("twin bound");
}

/// A saved case from the random runs, with a larger buy: a marked holder sells (the cap counts
/// it), buys unmarked tokens (the buy offsets that counted sell) and sells all again. The marked
/// part goes first and takes the room the buy freed for tokens held at the window start. The
/// bought tokens count too when they are sold in the same window, so the resale uses that room
/// again.
#[test]
fn an_unmarked_round_trip_gains_a_marked_holder_no_cap_room_for_tokens_it_held_before() {
    let history = [
        Step::Buy {
            account: 0,
            near_in: 202_902_785_215_120_078_622_703,
        },
        Step::Buy {
            account: 4,
            near_in: 1_695_156_243_502_244_216_731_901,
        },
    ];
    let trades = [
        WindowTrade::Sell { share_bps: 10_000 },
        WindowTrade::Buy {
            near_in: ONE_NEAR / 10,
        },
        WindowTrade::Sell { share_bps: 10_000 },
    ];
    twin_bound_holds(&history, 4, &trades).expect("twin bound");
}

/// A saved case from the random runs: a band holder of marked tokens only buys X and becomes
/// large, then sells all twice. The buy NEAR offsets the marked part, so marked tokens fill into
/// the cap room, and the bought tokens left then fill through the account's own unmarked floor.
/// The twin run gets only the marked floor. This fails under the rules as written; the builder
/// decides the rule.
#[test]
fn a_buy_gives_a_marked_holder_no_second_floor_for_tokens_it_held_before() {
    let history = [
        Step::Buy {
            account: 2,
            near_in: 10_054_969_190_151_614_964_507,
        },
        Step::Buy {
            account: 3,
            near_in: 2 * ONE_NEAR,
        },
        Step::Buy {
            account: 0,
            near_in: 0,
        },
        Step::Buy {
            account: 0,
            near_in: 0,
        },
        Step::Buy {
            account: 0,
            near_in: 0,
        },
        Step::Buy {
            account: 6,
            near_in: 6_482_066_110_376_778_936_034_148,
        },
    ];
    let trades = [
        WindowTrade::Buy {
            near_in: 949_958_024_007_415_573_715_582,
        },
        WindowTrade::Sell { share_bps: 10_000 },
        WindowTrade::Sell { share_bps: 10_000 },
    ];
    twin_bound_holds(&history, 3, &trades).expect("twin bound");
}
