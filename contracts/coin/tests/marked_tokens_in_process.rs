//! Marked tokens through the coin's methods, views and log lines (near-sdk unit-test runner):
//! the JSON fields `fc_buy.marked_tokens`, `fc_account.in_creator_group`, `fc_account.marked`,
//! `fc_send` marked fields, `fc_sell.marked_sold`, `fc_early_window` and
//! `fc_pool.creator_group_share_bps`, and the refusals `FC_EARLY_WINDOW` and `FC_CREATOR_CAP`.

use fair_chance_coin::Coin;
use near_sdk::json_types::U128;
use near_sdk::test_utils::{get_logs, VMContextBuilder};
use near_sdk::{testing_env, AccountId, NearToken};
use serde_json::Value;

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const LAUNCH_BLOCK: u64 = 10;
const CREATOR: &str = "creator.test";
const SUB_ACCOUNT: &str = "pool.creator.test";
const EARLY: &str = "early.test";
const LATE: &str = "late.test";

fn account(name: &str) -> AccountId {
    name.parse().unwrap()
}

/// A call `b` blocks after the launch (b = block height - launch height), `seconds` after it.
fn call(predecessor: &str, deposit: u128, b: u64, seconds: u64) {
    testing_env!(VMContextBuilder::new()
        .current_account_id(account("coin.factory.test"))
        .predecessor_account_id(account(predecessor))
        .signer_account_id(account(predecessor))
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .account_balance(NearToken::from_near(1_000))
        .block_height(LAUNCH_BLOCK + b)
        .block_timestamp((LAUNCH_BLOCK + seconds) * 1_000_000_000)
        .build());
}

fn events(name: &str) -> Vec<Value> {
    get_logs()
        .iter()
        .filter_map(|line| line.strip_prefix("EVENT_JSON:"))
        .map(|json| serde_json::from_str::<Value>(json).unwrap())
        .filter(|e| e["standard"] == "fair_chance" && e["event"] == name)
        .collect()
}

fn amount(value: &Value) -> u128 {
    value.as_str().expect("decimal string").parse().unwrap()
}

/// The coin with the creator's opening buy of `opening_buy` at block 0.
fn launched(opening_buy: u128) -> Coin {
    call("factory.test", opening_buy, 0, 0);
    Coin::new(
        account(CREATOR),
        "Test Coin".into(),
        "TEST".into(),
        None,
        None,
        "hash".into(),
    )
}

fn register(coin: &mut Coin, who: &str, b: u64, seconds: u64) {
    let min = coin.storage_balance_bounds().min.as_yoctonear();
    call(who, min, b, seconds);
    coin.storage_deposit(None, None);
}

fn buy(coin: &mut Coin, who: &str, near_in: u128, b: u64, seconds: u64) -> Value {
    if coin.storage_balance_of(account(who)).is_none() {
        register(coin, who, b, seconds);
    }
    call(who, near_in, b, seconds);
    coin.buy(U128(0));
    events("fc_buy").remove(0)["data"][0].clone()
}

fn view(value: impl near_sdk::serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn the_opening_buy_is_marked_pays_the_early_fee_and_shows_in_the_creator_s_account_and_pool() {
    let coin = launched(2 * ONE_NEAR);
    let bought = events("fc_buy").remove(0)["data"][0].clone();
    assert_eq!(bought["marked_tokens"], bought["tokens_out"]);
    assert_eq!(amount(&bought["fee_near"]), ONE_NEAR);
    let tokens = amount(&bought["tokens_out"]);

    call(LATE, 0, 1, 1);
    let creator = view(coin.fc_account(account(CREATOR)));
    assert_eq!(creator["in_creator_group"], true);
    assert_eq!(amount(&creator["marked_balance"]), tokens);
    assert_eq!(creator["unmarked_balance"], "0");
    assert_eq!(creator["marked"]["allowance"], "0");
    assert_eq!(
        view(coin.fc_account(account(SUB_ACCOUNT)))["in_creator_group"],
        true
    );
    assert_eq!(
        view(coin.fc_account(account("ecreator.test")))["in_creator_group"],
        false
    );

    let pool = view(coin.fc_pool());
    let supply = 1_000_000_000u128 * 10u128.pow(18);
    assert_eq!(
        pool["creator_group_share_bps"].as_u64().unwrap() as u128,
        tokens * 10_000 / supply
    );
    let early = view(coin.fc_early_window());
    assert_eq!(amount(&early["early_supply_total"]), tokens);
    assert_eq!(early["blocks_left"], 17);
    assert_eq!(early["fee_end_block"], LAUNCH_BLOCK + 18);
    assert_eq!(early["shared_allowance"], "0");
    assert!(early["buy_fee_bps_now"].as_u64().unwrap() > 100);
}

#[test]
#[should_panic(expected = "FC_EARLY_WINDOW")]
fn a_sell_of_marked_tokens_in_blocks_0_to_n_panics_with_fc_early_window() {
    let mut coin = launched(0);
    let bought = buy(&mut coin, EARLY, ONE_NEAR, 3, 3);
    call(EARLY, 1, 17, 17);
    coin.sell(U128(amount(&bought["tokens_out"])), U128(0));
}

#[test]
#[should_panic(expected = "FC_CREATOR_CAP")]
fn a_creator_group_buy_above_four_percent_panics_with_fc_creator_cap() {
    let mut coin = launched(0);
    buy(&mut coin, SUB_ACCOUNT, 5 * ONE_NEAR, 30, 30);
}

#[test]
fn the_first_sell_after_block_n_logs_marked_sold_and_a_send_logs_its_marked_amounts() {
    let mut coin = launched(0);
    let bought = buy(&mut coin, EARLY, ONE_NEAR, 3, 3);
    let tokens = amount(&bought["tokens_out"]);
    assert_eq!(bought["marked_tokens"], bought["tokens_out"]);

    register(&mut coin, SUB_ACCOUNT, 20, 20);
    call(EARLY, 1, 21, 21);
    coin.ft_transfer(account(SUB_ACCOUNT), U128(tokens / 4), None);
    let sent = events("fc_send").remove(0)["data"][0].clone();
    assert_eq!(amount(&sent["marked_amount"]), tokens / 4);
    assert_eq!(amount(&sent["to_marked_amount"]), tokens / 4);

    call(EARLY, 1, 22, 22);
    coin.sell(U128(tokens / 4), U128(0));
    let sold = events("fc_sell").remove(0)["data"][0].clone();
    assert!(amount(&sold["tokens_sold"]) > 0);
    assert_eq!(sold["marked_sold"], sold["tokens_sold"]);

    let account_view = view(coin.fc_account(account(EARLY)));
    assert!(account_view["marked"]["share_bps"].as_u64().unwrap() > 0);
    assert!(amount(&account_view["marked"]["allowance"]) > 0);
    assert_eq!(
        amount(&account_view["marked"]["used"]),
        amount(&sold["marked_sold"])
    );
}
