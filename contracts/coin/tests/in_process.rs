use fc_coin::Coin;
use near_sdk::json_types::U128;
use near_sdk::test_utils::{get_logs, VMContextBuilder};
use near_sdk::{testing_env, AccountId, NearToken};
use serde_json::Value;

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;

fn account(name: &str) -> AccountId {
    name.parse().unwrap()
}

fn context(predecessor: &str, deposit: u128, block: u64) -> VMContextBuilder {
    let mut ctx = VMContextBuilder::new();
    ctx.current_account_id(account("coin.factory.test"))
        .predecessor_account_id(account(predecessor))
        .signer_account_id(account(predecessor))
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .account_balance(NearToken::from_near(1_000))
        .block_height(block)
        .block_timestamp(block * 1_000_000_000);
    ctx
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
    testing_env!(context("factory.test", 0, 10).build());
    Coin::new(
        account("creator.test"),
        "Test Coin".into(),
        "TEST".into(),
        None,
        "hash".into(),
    )
}

#[test]
fn new_mints_the_whole_supply_once_into_the_pool() {
    let coin = launched();
    let mints = events("nep141", "ft_mint");
    assert_eq!(mints.len(), 1);
    assert_eq!(mints[0]["data"][0]["owner_id"], "coin.factory.test");
    assert_eq!(
        coin.ft_total_supply().0,
        1_000_000_000_000_000_000_000_000_000
    );
}

#[test]
fn a_sell_logs_fc_sell_with_the_interface_version() {
    let mut coin = launched();
    testing_env!(context("alice.test", ONE_NEAR, 11).build());
    coin.buy(U128(0));
    let tokens = coin.ft_balance_of(account("alice.test")).0;
    assert!(tokens > 0);

    testing_env!(context("alice.test", 1, 12).build());
    let _ = coin.sell(U128(tokens), U128(0));
    let sells = events("fair_chance", "fc_sell");
    assert_eq!(sells.len(), 1);
    assert_eq!(sells[0]["version"], "1.0.0-rc.4");
    assert_eq!(sells[0]["data"][0]["tokens_sold"], tokens.to_string());
    assert_eq!(coin.ft_balance_of(account("alice.test")).0, 0);
}

#[test]
#[should_panic(expected = "FC_ONE_YOCTO")]
fn a_sell_with_no_deposit_panics_with_fc_one_yocto() {
    let mut coin = launched();
    testing_env!(context("alice.test", ONE_NEAR, 11).build());
    coin.buy(U128(0));
    testing_env!(context("alice.test", 0, 12).build());
    let _ = coin.sell(U128(1), U128(0));
}

#[test]
fn fc_rules_reports_the_launch_block_and_creator() {
    let coin = launched();
    let rules = serde_json::to_value(coin.fc_rules()).unwrap();
    assert_eq!(rules["launch_block"], 10);
    assert_eq!(rules["creator"], "creator.test");
    assert_eq!(rules["brake_drop_bps"], Value::Null);
}

#[test]
fn the_package_version_is_the_interface_version() {
    assert_eq!(env!("CARGO_PKG_VERSION"), fc_rules::INTERFACE_VERSION);
}
