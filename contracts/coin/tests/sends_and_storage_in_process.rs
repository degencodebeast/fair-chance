//! NEP-141 sends and NEP-145 storage on the coin, with the Fair Chance refusals, in near-sdk's
//! unit-test runner.

use fair_chance_coin::Coin;
use near_sdk::json_types::U128;
use near_sdk::test_utils::{get_created_receipts, get_logs, VMContextBuilder};
use near_sdk::{env, testing_env, AccountId, NearToken};
use serde_json::Value;

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const COIN: &str = "coin.factory.test";

fn account(name: &str) -> AccountId {
    name.parse().unwrap()
}

fn context(predecessor: &str, deposit: u128, block: u64) -> VMContextBuilder {
    let mut ctx = VMContextBuilder::new();
    ctx.current_account_id(account(COIN))
        .predecessor_account_id(account(predecessor))
        .signer_account_id(account(predecessor))
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .account_balance(NearToken::from_near(1_000))
        .block_height(block)
        .block_timestamp(block * 1_000_000_000);
    ctx
}

fn at(predecessor: &str, deposit: u128, block: u64) {
    testing_env!(context(predecessor, deposit, block).build());
}

fn events(standard: &str, name: &str) -> Vec<Value> {
    get_logs()
        .iter()
        .filter_map(|line| line.strip_prefix("EVENT_JSON:"))
        .map(|json| serde_json::from_str::<Value>(json).unwrap())
        .filter(|e| e["standard"] == standard && e["event"] == name)
        .collect()
}

fn launched() -> Coin {
    at("factory.test", 0, 10);
    Coin::new(
        account("creator.test"),
        "Test Coin".into(),
        "TEST".into(),
        None,
        None,
        "hash".into(),
    )
}

fn buy(coin: &mut Coin, who: &str, near_in: u128, block: u64) {
    at(who, near_in, block);
    coin.buy(U128(0));
}

fn json(value: impl near_sdk::serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}

fn min_storage(coin: &Coin) -> u128 {
    coin.storage_balance_bounds().min.as_yoctonear()
}

fn register(coin: &mut Coin, who: &str, block: u64) {
    let min = min_storage(coin);
    at(who, min, block);
    coin.storage_deposit(None, None);
}

#[test]
fn ft_transfer_moves_tokens_and_logs_ft_transfer_and_fc_send() {
    let mut coin = launched();
    buy(&mut coin, "alice.test", ONE_NEAR, 11);
    register(&mut coin, "bob.test", 12);
    at("alice.test", 1, 13);
    coin.ft_transfer(account("bob.test"), U128(1_000), Some("hi".into()));

    assert_eq!(coin.ft_balance_of(account("bob.test")).0, 1_000);
    let sends = events("fair_chance", "fc_send");
    assert_eq!(sends.len(), 1);
    let send = &sends[0]["data"][0];
    assert_eq!(send["from"], "alice.test");
    assert_eq!(send["to"], "bob.test");
    assert_eq!(send["amount"], "1000");
    assert_eq!(send["marked_amount"], "0");
    assert_eq!(send["to_marked_amount"], "0");
    let transfers = events("nep141", "ft_transfer");
    assert_eq!(transfers[0]["data"][0]["memo"], "hi");
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn ft_transfer_without_one_yocto_panics() {
    let mut coin = launched();
    buy(&mut coin, "alice.test", ONE_NEAR, 11);
    register(&mut coin, "bob.test", 12);
    at("alice.test", 0, 13);
    coin.ft_transfer(account("bob.test"), U128(1), None);
}

#[test]
#[should_panic(expected = "FC_LARGE_CANNOT_SEND")]
fn ft_transfer_from_a_large_account_panics_with_fc_large_cannot_send() {
    let mut coin = launched();
    buy(&mut coin, "large.test", 5 * ONE_NEAR, 11);
    register(&mut coin, "bob.test", 12);
    at("large.test", 1, 13);
    coin.ft_transfer(account("bob.test"), U128(1), None);
}

#[test]
fn ft_transfer_that_would_make_the_receiver_large_panics_with_fc_account_max_receive() {
    let mut coin = launched();
    buy(&mut coin, "alice.test", 1_500 * ONE_NEAR / 1_000, 11);
    buy(&mut coin, "bob.test", ONE_NEAR, 12);
    at("alice.test", 1, 13);
    let view = json(coin.fc_account(account("bob.test")));
    let max_receive: u128 = view["max_receive"].as_str().unwrap().parse().unwrap();
    assert_eq!(view["class"], "small");
    assert_eq!(view["can_send"], true);

    let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        coin.ft_transfer(account("bob.test"), U128(max_receive + 1), None);
    }));
    let message = refused
        .unwrap_err()
        .downcast::<String>()
        .map(|s| *s)
        .unwrap_or_default();
    assert!(
        message.contains(&format!("FC_RECEIVER_WOULD_BE_LARGE: {max_receive}")),
        "{message}"
    );
}

#[test]
#[should_panic(expected = "FC_SEND_TO_COIN")]
fn ft_transfer_to_the_coin_panics_with_fc_send_to_coin() {
    let mut coin = launched();
    buy(&mut coin, "alice.test", ONE_NEAR, 11);
    at("alice.test", 1, 12);
    coin.ft_transfer(account(COIN), U128(1), None);
}

#[test]
#[should_panic(expected = "Sender and receiver should be different")]
fn ft_transfer_to_yourself_panics_with_the_nep141_error() {
    let mut coin = launched();
    buy(&mut coin, "alice.test", ONE_NEAR, 11);
    at("alice.test", 1, 12);
    coin.ft_transfer(account("alice.test"), U128(1), None);
}

#[test]
#[should_panic(expected = "The account is not registered: bob.test")]
fn ft_transfer_to_an_unregistered_account_panics() {
    let mut coin = launched();
    buy(&mut coin, "alice.test", ONE_NEAR, 11);
    at("alice.test", 1, 12);
    coin.ft_transfer(account("bob.test"), U128(1), None);
}

#[test]
#[should_panic(expected = "FC_TRANSFER_CALL")]
fn ft_transfer_call_panics_with_fc_transfer_call_for_any_receiver() {
    let mut coin = launched();
    buy(&mut coin, "alice.test", ONE_NEAR, 11);
    register(&mut coin, "bob.test", 12);
    at("alice.test", 1, 13);
    let _ = coin.ft_transfer_call(account("bob.test"), U128(1), None, String::new());
}

#[test]
fn storage_deposit_registers_at_the_minimum_and_refunds_the_rest() {
    let mut coin = launched();
    let bounds = json(coin.storage_balance_bounds());
    assert_eq!(bounds["min"], bounds["max"]);
    let min = min_storage(&coin);
    assert!(min > 0);
    assert_eq!(
        json(coin.storage_balance_of(account("bob.test"))),
        Value::Null
    );

    at("payer.test", min + 5, 11);
    let balance = json(coin.storage_deposit(Some(account("bob.test")), None));
    assert_eq!(balance["total"], min.to_string());
    assert_eq!(balance["available"], "0");
    assert_eq!(json(coin.storage_balance_of(account("bob.test"))), balance);
    let refunds = get_created_receipts();
    assert_eq!(refunds.len(), 1);
    assert_eq!(refunds[0].receiver_id, account("payer.test"));

    // Already registered: the whole deposit goes back.
    at("payer.test", min, 12);
    coin.storage_deposit(Some(account("bob.test")), None);
    assert_eq!(get_created_receipts().len(), 1);
    assert_eq!(
        json(coin.storage_balance_of(account("bob.test")))["total"],
        min.to_string()
    );
}

#[test]
#[should_panic(expected = "The attached deposit is less than the minimum storage balance")]
fn storage_deposit_below_the_minimum_panics() {
    let mut coin = launched();
    let min = min_storage(&coin);
    at("bob.test", min - 1, 11);
    coin.storage_deposit(None, None);
}

#[test]
fn the_storage_minimum_pays_for_one_record_of_the_longest_account_id() {
    let mut coin = launched();
    let min = min_storage(&coin);
    let longest = "a".repeat(64);
    at("payer.test", min, 11);
    let before = env::storage_usage();
    coin.storage_deposit(Some(account(&longest)), None);
    let used = u128::from(env::storage_usage() - before);
    assert!(
        used * env::storage_byte_cost().as_yoctonear() <= min,
        "{used} bytes"
    );
}

#[test]
#[should_panic(expected = "FC_UNREGISTER_WITH_BALANCE")]
fn storage_unregister_with_force_while_holding_tokens_panics_with_fc_unregister_with_balance() {
    let mut coin = launched();
    buy(&mut coin, "alice.test", ONE_NEAR, 11);
    at("alice.test", 1, 12);
    coin.storage_unregister(Some(true));
}

#[test]
fn storage_unregister_removes_an_empty_record_and_refunds_its_deposit() {
    let mut coin = launched();
    register(&mut coin, "bob.test", 11);
    at("bob.test", 1, 12);
    assert!(coin.storage_unregister(None));
    assert_eq!(
        json(coin.storage_balance_of(account("bob.test"))),
        Value::Null
    );
    let refunds = get_created_receipts();
    assert_eq!(refunds.len(), 1);
    assert_eq!(refunds[0].receiver_id, account("bob.test"));

    at("bob.test", 1, 13);
    assert!(!coin.storage_unregister(Some(true)));
}

#[test]
fn fc_account_fc_pool_and_fc_quote_buy_report_the_class() {
    let mut coin = launched();
    at("large.test", 0, 11);
    let quote = json(coin.fc_quote_buy(account("large.test"), U128(5 * ONE_NEAR)));
    assert_eq!(quote["class_after"], "large");
    assert_eq!(quote["becomes_large"], true);
    buy(&mut coin, "large.test", 5 * ONE_NEAR, 11);
    let became = events("fair_chance", "fc_became_large");
    assert_eq!(became.len(), 1);
    assert_eq!(became[0]["data"][0]["account"], "large.test");

    let view = json(coin.fc_account(account("large.test")));
    assert_eq!(view["class"], "large");
    assert_eq!(view["is_large"], true);
    assert_eq!(view["can_send"], false);
    assert_eq!(view["max_receive"], Value::Null);
    assert_eq!(json(coin.fc_pool())["large_holders"], 1);
}

#[test]
#[should_panic(expected = "The account is not registered: alice.test")]
fn a_buy_from_an_account_with_no_storage_record_panics() {
    let mut coin = launched();
    at("alice.test", ONE_NEAR, 11);
    coin.buy(U128(0));
}
