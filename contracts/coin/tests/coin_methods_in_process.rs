use fair_chance_coin::Coin;
use near_sdk::json_types::U128;
use near_sdk::test_utils::{get_logs, VMContextBuilder};
use near_sdk::{testing_env, AccountId, NearToken};
use serde_json::Value;

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const ICON: &str = include_str!("../../../testdata/coin-icon-96px-webp-data-url.txt");

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

fn new_coin(caller: &str) -> Coin {
    testing_env!(context(caller, 0, 10).build());
    Coin::new(
        account("creator.test"),
        "Test Coin".into(),
        "TEST".into(),
        Some(ICON.into()),
        Some("A test coin.".into()),
        "hash".into(),
    )
}

fn launched() -> Coin {
    new_coin("factory.test")
}

/// NEP-145: the storage minimum for `who`'s holder record (a buy needs one).
fn register(coin: &mut Coin, who: &str, block: u64) {
    let min = coin.storage_balance_bounds().min.as_yoctonear();
    testing_env!(context(who, min, block).build());
    coin.storage_deposit(None, None);
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
    register(&mut coin, "alice.test", 11);
    testing_env!(context("alice.test", ONE_NEAR, 11).build());
    coin.buy(U128(0));
    let tokens = coin.ft_balance_of(account("alice.test")).0;
    assert!(tokens > 0);

    testing_env!(context("alice.test", 1, 12).build());
    let _ = coin.sell(U128(tokens), U128(0));
    let sells = events("fair_chance", "fc_sell");
    assert_eq!(sells.len(), 1);
    assert_eq!(sells[0]["version"], rule_engine::INTERFACE_VERSION);
    assert_eq!(sells[0]["data"][0]["tokens_sold"], tokens.to_string());
    assert_eq!(coin.ft_balance_of(account("alice.test")).0, 0);
}

#[test]
#[should_panic(expected = "FC_ONE_YOCTO")]
fn a_sell_with_no_deposit_panics_with_fc_one_yocto() {
    let mut coin = launched();
    register(&mut coin, "alice.test", 11);
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
    assert_eq!(env!("CARGO_PKG_VERSION"), rule_engine::INTERFACE_VERSION);
}

/// The `contract_metadata` attribute needs a string literal, so it cannot use the constant.
/// This test keeps that literal equal to the interface version.
#[test]
fn the_nep330_fair_chance_standard_version_is_the_interface_version() {
    let source = include_str!("../src/lib.rs");
    let expected = format!(
        "standard(standard = \"fair_chance\", version = \"{}\")",
        rule_engine::INTERFACE_VERSION
    );
    assert!(
        source.contains(&expected),
        "contracts/coin/src/lib.rs must contain {expected}"
    );
}

#[test]
#[should_panic(expected = "new must be called by the parent account of the coin")]
fn new_called_by_an_account_that_is_not_the_parent_panics() {
    new_coin("creator.test");
}

#[test]
fn ft_metadata_returns_the_webp_icon_under_8_kb_and_no_reference() {
    let coin = launched();
    let metadata = serde_json::to_value(coin.ft_metadata()).unwrap();
    let icon = metadata["icon"].as_str().unwrap();
    assert!(icon.starts_with("data:image/webp;base64,"));
    assert!(icon.len() < 8 * 1024);
    assert_eq!(metadata["reference"], Value::Null);
    assert_eq!(metadata["reference_hash"], Value::Null);
    assert_eq!(metadata["decimals"], 18);
    assert_eq!(metadata["symbol"], "TEST");
}

#[test]
fn fc_rules_returns_the_launch_description() {
    let coin = launched();
    let rules = serde_json::to_value(coin.fc_rules()).unwrap();
    assert_eq!(rules["description"], "A test coin.");
}

#[test]
#[should_panic(expected = "FC_ONE_YOCTO")]
fn a_sell_with_two_yocto_panics_with_fc_one_yocto() {
    let mut coin = launched();
    register(&mut coin, "alice.test", 11);
    testing_env!(context("alice.test", ONE_NEAR, 11).build());
    coin.buy(U128(0));
    testing_env!(context("alice.test", 2, 12).build());
    let _ = coin.sell(U128(1), U128(0));
}

#[test]
#[should_panic(expected = "FC_SLIPPAGE")]
fn a_buy_below_min_tokens_out_panics_with_fc_slippage() {
    let mut coin = launched();
    register(&mut coin, "alice.test", 11);
    testing_env!(context("alice.test", ONE_NEAR, 11).build());
    coin.buy(U128(9_802_950_787_206_654_124_170_710));
}

#[test]
fn fc_quote_buy_equals_the_buy_that_follows() {
    let mut coin = launched();
    register(&mut coin, "alice.test", 11);
    testing_env!(context("alice.test", 2 * ONE_NEAR, 11).build());
    let quote =
        serde_json::to_value(coin.fc_quote_buy(account("alice.test"), U128(2 * ONE_NEAR))).unwrap();
    coin.buy(U128(0));
    let buys = events("fair_chance", "fc_buy");
    assert_eq!(buys.len(), 1);
    let buy = &buys[0]["data"][0];
    assert_eq!(quote["tokens_out"], buy["tokens_out"]);
    assert_eq!(quote["fee_near"], buy["fee_near"]);
    assert_eq!(buy["fee_near"], (2 * ONE_NEAR / 100).to_string());
    let pool = serde_json::to_value(coin.fc_pool()).unwrap();
    assert_eq!(pool["real_near"], (2 * ONE_NEAR).to_string());
}

#[test]
fn fc_account_unmarked_and_fc_quote_sell_match_a_large_sell_held_to_its_allowance() {
    let mut coin = launched();
    register(&mut coin, "large.test", 11);
    testing_env!(context("large.test", 5 * ONE_NEAR, 11).build());
    coin.buy(U128(0));

    // Block 3_620 is in the second 1 h window after the launch at block 10.
    let window_1 = 3_620;
    testing_env!(context("large.test", 1, window_1).build());
    let before = serde_json::to_value(coin.fc_account(account("large.test"))).unwrap();
    let unmarked = &before["unmarked"];
    let allowance: u128 = unmarked["allowance"].as_str().unwrap().parse().unwrap();
    assert!(allowance > 0);
    assert_eq!(unmarked["used"], "0");
    assert_eq!(unmarked["left"], allowance.to_string());
    assert_eq!(
        unmarked["window_end_ns"],
        ((10 + 2 * 3_600) * 1_000_000_000u64).to_string()
    );

    let asked = 2 * allowance;
    let quote =
        serde_json::to_value(coin.fc_quote_sell(account("large.test"), U128(asked))).unwrap();
    let _ = coin.sell(U128(asked), U128(0));
    let sells = events("fair_chance", "fc_sell");
    let sold = &sells[0]["data"][0];
    assert_eq!(sold["tokens_sold"], allowance.to_string());
    assert_eq!(sold["fill"], "partial");
    assert_eq!(sold["limit_reason"], "allowance");
    assert_eq!(sold["allowance_used"], allowance.to_string());
    assert_eq!(sold["allowance_left"], "0");
    assert_eq!(quote["tokens_filled"], sold["tokens_sold"]);
    assert_eq!(quote["near_out"], sold["near_out"]);
    assert_eq!(quote["fill"], sold["fill"]);
    assert_eq!(quote["limit_reason"], sold["limit_reason"]);

    let after = serde_json::to_value(coin.fc_account(account("large.test"))).unwrap();
    assert_eq!(after["unmarked"]["used"], allowance.to_string());
    assert_eq!(after["unmarked"]["left"], "0");
}

#[test]
fn a_band_sell_above_the_first_allowance_pays_the_penalty_into_the_pool_and_matches_its_quote() {
    let mut coin = launched();
    register(&mut coin, "band.test", 11);
    testing_env!(context("band.test", 3 * ONE_NEAR / 2, 11).build());
    coin.buy(U128(0));

    let window_1 = 3_620;
    testing_env!(context("band.test", 1, window_1).build());
    let view = serde_json::to_value(coin.fc_account(account("band.test"))).unwrap();
    assert_eq!(view["class"], "band");
    let allowance = &view["unmarked"]["allowance"];
    assert_eq!(&view["penalty_applies_above"], allowance);
    assert_eq!(&view["max_full_fill_now"], allowance);
    let balance: u128 = view["balance"].as_str().unwrap().parse().unwrap();
    let real_near_before: u128 = serde_json::to_value(coin.fc_pool()).unwrap()["real_near"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let quote =
        serde_json::to_value(coin.fc_quote_sell(account("band.test"), U128(balance))).unwrap();
    let _ = coin.sell(U128(balance), U128(0));
    let sells = events("fair_chance", "fc_sell");
    let sold = &sells[0]["data"][0];
    assert_eq!(sold["class"], "band");
    assert_eq!(sold["tokens_sold"], balance.to_string());
    assert_eq!(sold["fill"], "full");
    assert_eq!(sold["limit_reason"], serde_json::Value::Null);
    assert_ne!(sold["penalty_near"], "0");
    assert_eq!(quote["penalty_near"], sold["penalty_near"]);
    assert_eq!(quote["near_out"], sold["near_out"]);
    assert_eq!(quote["fill"], sold["fill"]);
    let near_out: u128 = sold["near_out"].as_str().unwrap().parse().unwrap();
    let real_near_after: u128 = serde_json::to_value(coin.fc_pool()).unwrap()["real_near"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(real_near_after, real_near_before - near_out);
}
