//! `claim_creator_fees` and its callback `on_creator_claim` in the near-sdk in-process runner
//! (ADR 0010). The claim needs exactly 1 yoctoNEAR, the creator account, and 24 h of pool depth
//! at or above X = 10 real NEAR. The callback logs `fc_creator_claim` only when the payout
//! transfer succeeded, and puts the amount back in the creator balance when it failed.

use fair_chance_coin::Coin;
use near_sdk::json_types::{U128, U64};
use near_sdk::test_utils::{get_logs, VMContextBuilder};
use near_sdk::{testing_env, AccountId, NearToken, PromiseResult, RuntimeFeesConfig};
use serde_json::Value;

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const ICON: &str = include_str!("../../../testdata/coin-icon-96px-webp-data-url.txt");
const COIN: &str = "coin.factory.test";
const CREATOR: &str = "creator.test";
const HOUR_S: u64 = 3_600;
/// The launch is at block 10, at 10 s.
const LAUNCH_S: u64 = 10;

fn account(name: &str) -> AccountId {
    name.parse().unwrap()
}

/// The context of a call at `second` (block time in seconds); the block height runs 18 blocks
/// ahead of the second after the launch, so buys are after the early window.
fn context(predecessor: &str, deposit: u128, second: u64) -> VMContextBuilder {
    let height = if second > LAUNCH_S {
        second + 18
    } else {
        second
    };
    let mut ctx = VMContextBuilder::new();
    ctx.current_account_id(account(COIN))
        .predecessor_account_id(account(predecessor))
        .signer_account_id(account(predecessor))
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .account_balance(NearToken::from_near(1_000))
        .block_height(height)
        .block_timestamp(second * 1_000_000_000);
    ctx
}

/// A second `seconds` into 1 h window `window` after the launch.
fn in_window(window: u64, seconds: u64) -> u64 {
    LAUNCH_S + window * HOUR_S + seconds
}

fn events(name: &str) -> Vec<Value> {
    get_logs()
        .iter()
        .filter_map(|line| line.strip_prefix("EVENT_JSON:"))
        .map(|json| serde_json::from_str::<Value>(json).unwrap())
        .filter(|e| e["standard"] == "fair_chance" && e["event"] == name)
        .collect()
}

fn fees(coin: &Coin) -> Value {
    serde_json::to_value(coin.fc_creator_fees()).unwrap()
}

/// Twenty-one accounts buy 0.5 NEAR each in window 0: about 10.47 real NEAR, above X. The
/// creator balance is 30% of their 1% fees: 0.0315 NEAR.
fn coin_with_depth() -> Coin {
    testing_env!(context("factory.test", 0, LAUNCH_S).build());
    let mut coin = Coin::new(
        account(CREATOR),
        "Test Coin".into(),
        "TEST".into(),
        Some(ICON.into()),
        None,
        "hash".into(),
    );
    let min = coin.storage_balance_bounds().min.as_yoctonear();
    for i in 0..21 {
        let who = format!("friend-{i}.test");
        testing_env!(context(&who, min, in_window(0, 1 + i)).build());
        coin.storage_deposit(None, None);
        testing_env!(context(&who, ONE_NEAR / 2, in_window(0, 1 + i)).build());
        coin.buy(U128(0));
    }
    coin
}

const BALANCE: u128 = 21 * 1_500_000_000_000_000_000_000;

fn depth_ok_since_ns() -> u64 {
    in_window(1, 0) * 1_000_000_000
}

#[test]
#[should_panic(expected = "FC_ONE_YOCTO")]
fn a_claim_without_exactly_one_yocto_is_refused() {
    let mut coin = coin_with_depth();
    testing_env!(context(CREATOR, 0, in_window(25, 0)).build());
    let _ = coin.claim_creator_fees();
}

#[test]
#[should_panic(expected = "FC_NOT_CREATOR")]
fn a_claim_by_any_account_but_the_creator_is_refused() {
    let mut coin = coin_with_depth();
    testing_env!(context("friend-1.test", 1, in_window(25, 0)).build());
    let _ = coin.claim_creator_fees();
}

#[test]
#[should_panic(expected = "FC_CLAIM_NOT_READY")]
fn a_claim_before_24_h_of_depth_is_refused() {
    let mut coin = coin_with_depth();
    testing_env!(context(CREATOR, 1, in_window(24, 3_599)).build());
    let _ = coin.claim_creator_fees();
}

#[test]
fn a_claim_after_24_h_of_depth_sends_the_balance_and_the_callback_logs_fc_creator_claim() {
    let mut coin = coin_with_depth();
    let at = in_window(25, 0);
    testing_env!(context(CREATOR, 1, at).build());
    let view = fees(&coin);
    assert_eq!(view["balance"], BALANCE.to_string());
    assert_eq!(view["claimable_now"], true);
    assert_eq!(view["depth_ok_since_ns"], depth_ok_since_ns().to_string());
    let real_near = serde_json::to_value(coin.fc_pool()).unwrap()["real_near"].clone();

    let _ = coin.claim_creator_fees();
    assert!(events("fc_creator_claim").is_empty());
    assert_eq!(fees(&coin)["balance"], "0");
    assert_eq!(
        serde_json::to_value(coin.fc_pool()).unwrap()["real_near"],
        real_near
    );

    testing_env!(
        context(COIN, 0, at + 1).build(),
        near_sdk::test_vm_config(),
        RuntimeFeesConfig::test(),
        Default::default(),
        vec![PromiseResult::Successful(vec![])],
    );
    assert!(coin.on_creator_claim(U128(BALANCE), U64(depth_ok_since_ns())));
    let claims = events("fc_creator_claim");
    assert_eq!(claims.len(), 1);
    assert_eq!(
        claims[0]["data"][0],
        serde_json::json!({
            "creator": CREATOR,
            "amount": BALANCE.to_string(),
            "depth_ok_since_ns": depth_ok_since_ns().to_string(),
            "block": at + 1 + 18,
        })
    );
    let view = fees(&coin);
    assert_eq!(view["balance"], "0");
    assert_eq!(view["claimed_total"], BALANCE.to_string());
}

#[test]
fn a_failed_payout_transfer_puts_the_amount_back_in_the_creator_balance() {
    let mut coin = coin_with_depth();
    let at = in_window(25, 0);
    testing_env!(context(CREATOR, 1, at).build());
    let _ = coin.claim_creator_fees();
    testing_env!(
        context(COIN, 0, at + 1).build(),
        near_sdk::test_vm_config(),
        RuntimeFeesConfig::test(),
        Default::default(),
        vec![PromiseResult::Failed],
    );
    assert!(!coin.on_creator_claim(U128(BALANCE), U64(depth_ok_since_ns())));
    assert!(events("fc_creator_claim").is_empty());
    let view = fees(&coin);
    assert_eq!(view["balance"], BALANCE.to_string());
    assert_eq!(view["claimed_total"], "0");
}

/// Runs `new` with a 1 NEAR opening buy on a coin account that holds `balance` yoctoNEAR before
/// the deposit, on empty storage.
fn new_with_opening_buy(balance: u128) {
    near_sdk::mock::with_mocked_blockchain(|b| {
        b.take_storage();
    });
    let mut ctx = context("factory.test", ONE_NEAR, LAUNCH_S);
    ctx.account_balance(NearToken::from_yoctonear(balance));
    testing_env!(ctx.build());
    let _ = Coin::new(
        account(CREATOR),
        "Test Coin".into(),
        "TEST".into(),
        Some(ICON.into()),
        None,
        "hash".into(),
    );
}

/// The creator balance is owed NEAR too: `new` must not pay the coin's storage out of it.
#[test]
#[should_panic(expected = "the launch deposit does not cover the coin's storage")]
fn new_refuses_an_opening_buy_whose_storage_would_come_out_of_the_creator_balance() {
    new_with_opening_buy(1_000 * ONE_NEAR);
    let byte_cost = near_sdk::env::storage_byte_cost().as_yoctonear();
    let lock = byte_cost * u128::from(near_sdk::env::storage_usage());
    // The runner adds the 1 NEAR attached to the balance: 1 NEAR plus the lock less 1 yocto. The
    // real NEAR (1 NEAR less the 0.003 NEAR creator share) and the lock fit only if the creator
    // share pays for storage.
    new_with_opening_buy(lock - 1);
}
