//! Property (near-sdk in-process runner, proptest): after any sequence of launch, storage
//! deposits, buys, sells, waits and creator claims, the coin's NEAR balance is at least its real
//! NEAR + its creator fee balance + the cost of the storage it uses. So the pool can pay out all
//! of its real NEAR, the creator can claim its whole balance, and neither ever pays for storage.
//!
//! The runner does not move NEAR, so the test keeps the coin's balance itself: every attached
//! deposit comes in, every transfer the coin makes goes out, and a failed claim transfer comes
//! back. Gas is outside this count (it only adds to the coin's balance). The launch balance is
//! sometimes within 0.01 NEAR of the smallest balance `new` accepts.

use std::panic::{catch_unwind, AssertUnwindSafe};

use fair_chance_coin::Coin;
use near_sdk::json_types::{U128, U64};
use near_sdk::mock::MockAction;
use near_sdk::test_utils::{get_created_receipts, VMContextBuilder};
use near_sdk::{env, testing_env, AccountId, NearToken, PromiseResult, RuntimeFeesConfig};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const ICON: &str = include_str!("../../../testdata/coin-icon-96px-webp-data-url.txt");
const COIN: &str = "coin.factory.test";
const FACTORY: &str = "factory.test";
const CREATOR: &str = "creator.test";
const BUYERS: usize = 4;
/// The launch is at block 100, at 100 s. Blocks then run one per second, so the early window
/// (blocks 0 to 17 after launch) is over before the first step.
const LAUNCH_S: u64 = 100;

fn account(name: &str) -> AccountId {
    name.parse().unwrap()
}

fn buyer(index: usize) -> String {
    format!("buyer-{index}.test")
}

#[derive(Debug, Clone)]
enum Step {
    Buy {
        buyer: usize,
        near_in: u128,
    },
    /// Sells `share_bps` of the buyer's balance (10_000 = all of it).
    Sell {
        buyer: usize,
        share_bps: u128,
    },
    Wait {
        seconds: u64,
    },
    /// The creator claims when it can; the payout transfer then succeeds or fails.
    Claim {
        transfer_succeeds: bool,
    },
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => (0..BUYERS, prop_oneof![1..1_000u128, ONE_NEAR / 100..ONE_NEAR, ONE_NEAR..15 * ONE_NEAR])
            .prop_map(|(buyer, near_in)| Step::Buy { buyer, near_in }),
        3 => (0..BUYERS, prop_oneof![Just(10_000u128), 1..=10_000u128])
            .prop_map(|(buyer, share_bps)| Step::Sell { buyer, share_bps }),
        2 => prop_oneof![1..600u64, 3_000..7_300u64, 80_000..90_000u64]
            .prop_map(|seconds| Step::Wait { seconds }),
        1 => any::<bool>().prop_map(|transfer_succeeds| Step::Claim { transfer_succeeds }),
    ]
}

/// The coin, and the facts the runner does not keep across calls: the coin's NEAR balance, the
/// bytes it stores and the time.
struct Chain {
    coin: Coin,
    balance: u128,
    storage_usage: u64,
    second: u64,
}

fn context(
    predecessor: &str,
    deposit: u128,
    second: u64,
    balance: u128,
    storage_usage: u64,
) -> VMContextBuilder {
    let mut ctx = VMContextBuilder::new();
    ctx.current_account_id(account(COIN))
        .predecessor_account_id(account(predecessor))
        .signer_account_id(account(predecessor))
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .account_balance(NearToken::from_yoctonear(balance))
        .storage_usage(storage_usage)
        .block_height(second)
        .block_timestamp(second * 1_000_000_000);
    ctx
}

/// NEAR the coin sent out in the call just made.
fn sent_out() -> u128 {
    get_created_receipts()
        .iter()
        .flat_map(|r| r.actions.iter())
        .map(|a| match a {
            MockAction::Transfer { deposit, .. } => deposit.as_yoctonear(),
            _ => 0,
        })
        .sum()
}

/// Starts the coin on empty storage with an opening buy of `opening_buy`, on an account that
/// holds `balance` before the deposit. `None` when `new` refuses.
fn launch(balance: u128, opening_buy: u128) -> Option<Chain> {
    near_sdk::mock::with_mocked_blockchain(|b| {
        b.take_storage();
    });
    testing_env!(context(FACTORY, opening_buy, LAUNCH_S, balance, 0).build());
    let coin = catch_unwind(AssertUnwindSafe(|| {
        Coin::new(
            account(CREATOR),
            "Test Coin".into(),
            "TEST".into(),
            Some(ICON.into()),
            None,
            "hash".into(),
        )
    }))
    .ok()?;
    Some(Chain {
        coin,
        balance: balance + opening_buy,
        storage_usage: env::storage_usage(),
        second: LAUNCH_S + 20,
    })
}

/// The smallest balance before the deposit that `new` accepts for this opening buy, near enough:
/// the storage cost of a coin launched with it.
fn storage_cost_of_a_launch(opening_buy: u128) -> u128 {
    launch(1_000 * ONE_NEAR, opening_buy).expect("launch");
    env::storage_byte_cost().as_yoctonear() * u128::from(env::storage_usage())
}

impl Chain {
    /// Runs one call as `who` with `deposit` attached and counts the NEAR in and out.
    fn call<R>(&mut self, who: &str, deposit: u128, f: impl FnOnce(&mut Coin) -> R) -> R {
        self.call_with_results(who, deposit, vec![], f)
    }

    fn call_with_results<R>(
        &mut self,
        who: &str,
        deposit: u128,
        promise_results: Vec<PromiseResult>,
        f: impl FnOnce(&mut Coin) -> R,
    ) -> R {
        self.second += 1;
        testing_env!(
            context(who, deposit, self.second, self.balance, self.storage_usage).build(),
            near_sdk::test_vm_config(),
            RuntimeFeesConfig::test(),
            Default::default(),
            promise_results,
        );
        let out = f(&mut self.coin);
        self.balance = self.balance + deposit - sent_out();
        self.storage_usage = env::storage_usage();
        out
    }

    /// Reads the coin in the context of the last call: views read only storage, which is the same.
    fn view<R>(&mut self, f: impl FnOnce(&Coin) -> R) -> R {
        f(&self.coin)
    }

    /// Reads the coin at the time the next call runs, for views that depend on the time.
    fn view_at_next_call<R>(&mut self, f: impl FnOnce(&Coin) -> R) -> R {
        testing_env!(context(
            CREATOR,
            0,
            self.second + 1,
            self.balance,
            self.storage_usage
        )
        .build());
        f(&self.coin)
    }

    fn run(&mut self, step: &Step) {
        match *step {
            Step::Wait { seconds } => self.second += seconds,
            Step::Buy {
                buyer: index,
                near_in,
            } => {
                let who = buyer(index);
                let registered = self.view(|c| c.storage_balance_of(account(&who)).is_some());
                if !registered {
                    let min = self.view(|c| c.storage_balance_bounds().min.as_yoctonear());
                    self.call(&who, min, |c| {
                        c.storage_deposit(None, None);
                    });
                }
                self.call(&who, near_in, |c| c.buy(U128(0)));
            }
            Step::Sell {
                buyer: index,
                share_bps,
            } => {
                let who = buyer(index);
                let held = self.view(|c| c.ft_balance_of(account(&who)).0);
                let amount = held * share_bps / 10_000;
                if amount > 0 {
                    self.call(&who, 1, |c| {
                        let _ = c.sell(U128(amount), U128(0));
                    });
                }
            }
            Step::Claim { transfer_succeeds } => {
                let fees =
                    self.view_at_next_call(|c| serde_json::to_value(c.fc_creator_fees()).unwrap());
                if fees["claimable_now"] != true {
                    return;
                }
                let amount: u128 = fees["balance"].as_str().unwrap().parse().unwrap();
                let since: u64 = fees["depth_ok_since_ns"].as_str().unwrap().parse().unwrap();
                self.call(CREATOR, 1, |c| {
                    let _ = c.claim_creator_fees();
                });
                let result = if transfer_succeeds {
                    PromiseResult::Successful(vec![])
                } else {
                    // The failed transfer's NEAR comes back to the coin.
                    self.balance += amount;
                    PromiseResult::Failed
                };
                self.call_with_results(COIN, 0, vec![result], |c| {
                    c.on_creator_claim(U128(amount), U64(since))
                });
            }
        }
    }

    fn check(&mut self) -> Result<(), TestCaseError> {
        let pool = self.view(|c| serde_json::to_value(c.fc_pool()).unwrap());
        let real_near: u128 = pool["real_near"].as_str().unwrap().parse().unwrap();
        let creator_fees: u128 = pool["creator_fee_balance"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let storage = env::storage_byte_cost().as_yoctonear() * u128::from(self.storage_usage);
        prop_assert!(
            self.balance >= real_near + creator_fees + storage,
            "balance {} < real NEAR {real_near} + creator balance {creator_fees} + storage {storage}",
            self.balance
        );
        Ok(())
    }
}

fn launch_balance() -> impl Strategy<Value = Option<i128>> {
    // None: a roomy 5 NEAR above the storage cost. Some(d): the storage cost plus d yoctoNEAR,
    // d within 0.01 NEAR either side.
    prop_oneof![
        Just(None),
        (-(ONE_NEAR as i128) / 100..(ONE_NEAR as i128) / 100).prop_map(Some),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1_000,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/coin_balance_covers_pool_and_creator_balance.txt",
        ))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn the_coin_balance_always_covers_real_near_the_creator_balance_and_storage(
        opening_buy in prop_oneof![Just(0u128), ONE_NEAR / 100..2 * ONE_NEAR],
        launch_offset in launch_balance(),
        steps in proptest::collection::vec(step(), 0..40),
    ) {
        let cost = storage_cost_of_a_launch(opening_buy);
        let balance = match launch_offset {
            None => cost + 5 * ONE_NEAR,
            Some(d) => (cost as i128 + d) as u128,
        };
        let Some(mut chain) = launch(balance, opening_buy) else {
            return Ok(());
        };
        chain.check()?;
        for step in &steps {
            chain.run(step);
            chain.check()?;
        }
    }
}
