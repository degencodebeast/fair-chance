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
//! Each gate run tries 1,000 sequences. A failing sequence is saved in
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

fn account_name(index: usize) -> String {
    format!("account-{index}.test")
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
}
