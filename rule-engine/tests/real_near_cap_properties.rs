//! Property tests on the rule engine (proptest): random sequences of buys, sells, sends and
//! window moves across many accounts, at a creator fee share of 0. Big buys make accounts band
//! or large, and sells take them back down, so classes change inside a sequence.
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
//! - Small sells never count, sends never count, and no account's buys lower another
//!   account's counted sells: the cap used falls by at most the buyer's own part.
//! - Every holder record has the size of an empty record, so the storage deposit for a record
//!   pays for it whatever the account does.
//!
//!
//! A second property runs one account's buys, round trips and sells in one window after a
//! random history. The NEAR it gets for the tokens it held at the window start (its payouts
//! less the NEAR its buys paid) is at most what a twin run gets: the same trades with its buys
//! and buy-backs removed (the twin run already includes its floor), plus c x F, plus the price
//! effect of F (twin NEAR out x F / the lowest NEAR reserve in the window, virtual plus real),
//! plus 1 yoctoNEAR per trade for rounding. F is the fees the pool kept from the account's buys.
//!
//! Each gate run tries 1,000 sequences per property. A failing sequence is saved in
//! `proptest-regressions/real_near_cap_properties.txt` (committed), and runs first next time.

use std::collections::BTreeMap;

use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use rule_engine::{
    apply, launch, quote_sell, view_account, view_cap, view_pool, Action, At, CoinState, Event,
    FcSell, FixedValues, Holder, Holders, LaunchRequest,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_NS: u64 = 100_000_000_000;
const ACCOUNTS: usize = 8;
const BPS: u128 = 10_000;

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

/// Account 0 is the coin's creator; the others are plain accounts.
fn account_name(index: usize) -> String {
    if index == 0 {
        "creator.test".to_string()
    } else {
        format!("account-{index}.test")
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
                creator: "creator.test",
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
        }
        match *step {
            Step::Wait { seconds } => {
                self.at.ns += seconds * 1_000_000_000;
                self.at.block += seconds;
                Ok(())
            }
            Step::Buy { account, near_in } => {
                let account = account_name(account);
                self.act(
                    &account,
                    Action::Buy {
                        account: account.clone(),
                        near_in,
                        min_tokens_out: 0,
                    },
                )?;
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
                self.act(
                    &from,
                    Action::Send {
                        from: from.clone(),
                        to,
                        amount,
                        memo: None,
                    },
                )?;
                prop_assert_eq!(self.cap_used(), used_before, "a send changed the cap used");
                Ok(())
            }
        }
    }

    fn sell(&mut self, account: &str, share_bps: u128) -> Result<(), TestCaseError> {
        let amount = self.balance(account) * share_bps / BPS;
        let before = view_account(&self.state, self.ledger.get(account), self.at);
        if before.class == "band" {
            prop_assert_eq!(before.penalty_applies_above, Some(before.max_full_fill_now));
        }
        let quote = quote_sell(
            &self.state,
            self.holders(&[account.to_string()]),
            account,
            amount,
            self.at,
        );
        let used_before = self.cap_used();
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
        let above_what_always_fills = match sell.class {
            "large" => sell.tokens_sold > before.unmarked.floor_left,
            "band" => sell.tokens_sold > before.max_full_fill_now,
            _ => false,
        };
        if above_what_always_fills {
            prop_assert!(
                cap.cap_used * BPS <= cap.real_near_now * self.state.fixed.real_cap_c_bps as u128,
                "cap used {} above c x real NEAR left {} after a {} sell",
                cap.cap_used,
                cap.real_near_now,
                sell.class
            );
        }
        if sell.class == "small" {
            prop_assert_eq!(cap.cap_used, used_before, "a small sell counted");
        }
        Ok(())
    }

    /// Applies one action and checks the properties every action keeps. A refused action
    /// changes nothing. Returns the `fc_sell` of a sell.
    fn act(&mut self, by: &str, action: Action) -> Result<Option<FcSell>, TestCaseError> {
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
        Ok(applied.events.iter().find_map(|e| match e {
            Event::FcSell(s) => Some(s.clone()),
            _ => None,
        }))
    }
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
        cases: 1_000,
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

/// Rounding slack per trade in the twin bound: each trade has up to three roundings (NEAR out,
/// fee, penalty), each toward the pool, but on different bases in the two runs.
const ROUNDING_YOCTO_PER_TRADE: u128 = 1;

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
    let start = At {
        block: coin.at.block + 10,
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
/// run round on different bases.
#[test]
fn the_twin_bound_holds_for_a_buy_then_two_sells_that_round_on_different_bases() {
    let history = [
        Step::Buy {
            account: 5,
            near_in: 758_980_140_491_974_805_048_750,
        },
        Step::Buy {
            account: 0,
            near_in: 6_920_531_327_738_845_529_101_183,
        },
        Step::Buy {
            account: 0,
            near_in: 15_772_612_238_901_679_912_002_747,
        },
        Step::Buy {
            account: 0,
            near_in: 23_630_972_305_690_285_432_213_539,
        },
        Step::Buy {
            account: 0,
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
