//! The factory `launch` batch on a real chain (ADR 0006): CreateAccount, Transfer,
//! UseGlobalContract by code hash, FunctionCall `new`, and no AddKey.

use anyhow::Result;
use fair_chance_sandbox_tests::{
    events, launch_args, ContractState, World, COIN_DESCRIPTION, COIN_ICON, COIN_TICKER,
    LAUNCH_DEPOSIT,
};
use near_api::types::NearToken;
use serde_json::json;

#[tokio::test]
async fn fc_launch_reports_the_block_of_new_and_the_description() -> Result<()> {
    let world = World::launch().await?;

    let launches = events(&world.launch, "fair_chance_factory", "fc_launch");
    assert_eq!(launches.len(), 1);
    assert_eq!(launches[0].executor, world.factory.id.to_string());
    let data = &launches[0].event["data"][0];
    assert_eq!(launches[0].event["version"], rule_engine::INTERFACE_VERSION);

    // `new` is the receipt on the coin account that logs the one ft_mint.
    let mints = events(&world.launch, "nep141", "ft_mint");
    assert_eq!(mints.len(), 1);
    let new_block = world.block_height_of(mints[0].block_hash).await?;
    let callback_block = world.block_height_of(launches[0].block_hash).await?;
    assert!(callback_block > new_block, "fc_launch is logged after new");

    assert_eq!(data["launch_block"], new_block);
    let rules = world.view(&world.coin, "fc_rules", json!({})).await?;
    assert_eq!(rules["launch_block"], new_block);
    assert_eq!(data["description"], COIN_DESCRIPTION);
    assert_eq!(data["coin"], world.coin.to_string());
    assert_eq!(data["creator"], world.creator.id.to_string());
    assert_eq!(data["code_hash"], world.coin_code_hash.to_string());
    Ok(())
}

#[tokio::test]
async fn fc_launch_reports_a_null_description_when_none_was_given() -> Result<()> {
    let world = World::launch().await?;
    let result = world
        .try_launch(
            &world.alice,
            launch_args("Plain", "plain", Some(COIN_ICON), None, None),
            LAUNCH_DEPOSIT,
        )
        .await?;
    assert!(result.is_success(), "{result:?}");
    let launches = events(&result, "fair_chance_factory", "fc_launch");
    assert_eq!(launches.len(), 1);
    let data = &launches[0].event["data"][0];
    assert!(data.as_object().unwrap().contains_key("description"));
    assert_eq!(data["description"], serde_json::Value::Null);
    Ok(())
}

#[tokio::test]
async fn the_coin_runs_the_recorded_global_code_by_hash_and_has_no_access_keys() -> Result<()> {
    let world = World::launch().await?;

    let recorded = world
        .view(&world.factory.id, "coin_code_hash", json!({}))
        .await?;
    assert_eq!(recorded, world.coin_code_hash.to_string());

    let state = world.contract_state(&world.coin).await?;
    assert_eq!(state, Some(ContractState::GlobalHash(world.coin_code_hash)));

    let rules = world.view(&world.coin, "fc_rules", json!({})).await?;
    assert_eq!(rules["code_hash"], recorded);

    assert_eq!(world.access_key_count(&world.coin).await?, 0);
    Ok(())
}

#[tokio::test]
async fn a_second_new_is_refused_even_from_the_factory() -> Result<()> {
    let world = World::launch().await?;
    let args = json!({
        "creator": world.alice.id,
        "name": "Again",
        "symbol": "AGAIN",
        "icon": null,
        "description": null,
        "code_hash": world.coin_code_hash.to_string(),
    });
    for caller in [&world.factory, &world.creator] {
        let failure = world
            .call_refused(
                caller,
                &world.coin,
                "new",
                args.clone(),
                NearToken::from_yoctonear(0),
            )
            .await?;
        assert!(failure.contains("already been initialized"), "{failure}");
    }
    let rules = world.view(&world.coin, "fc_rules", json!({})).await?;
    assert_eq!(rules["creator"], world.creator.id.to_string());
    Ok(())
}

#[tokio::test]
async fn a_launch_whose_new_fails_leaves_no_coin_account_and_returns_the_deposit() -> Result<()> {
    let world = World::launch().await?;
    let coin = world.coin_for("badicon")?;
    let before = world.balance(&world.alice.id).await?;
    let result = world
        .try_launch(
            &world.alice,
            launch_args(
                "Bad",
                "badicon",
                Some("data:image/png;base64,AAAA"),
                None,
                None,
            ),
            LAUNCH_DEPOSIT,
        )
        .await?;

    assert!(events(&result, "fair_chance_factory", "fc_launch").is_empty());
    assert_eq!(world.contract_state(&coin).await?, None);
    // The refund arrives in a later receipt of the same transaction; allow for gas only.
    let after = world.balance(&world.alice.id).await?;
    let spent = before.saturating_sub(after);
    assert!(spent < NearToken::from_millinear(100), "spent {spent}");
    Ok(())
}

#[tokio::test]
async fn a_launch_on_a_taken_ticker_logs_no_fc_launch_and_leaves_the_first_coin() -> Result<()> {
    let world = World::launch().await?;
    let before = world.balance(&world.alice.id).await?;
    let result = world
        .try_launch(
            &world.alice,
            launch_args("Copy", COIN_TICKER, None, Some("a copy"), None),
            LAUNCH_DEPOSIT,
        )
        .await?;

    assert!(events(&result, "fair_chance_factory", "fc_launch").is_empty());
    let rules = world.view(&world.coin, "fc_rules", json!({})).await?;
    assert_eq!(rules["creator"], world.creator.id.to_string());
    assert_eq!(rules["description"], COIN_DESCRIPTION);
    let spent = before.saturating_sub(world.balance(&world.alice.id).await?);
    assert!(spent < NearToken::from_millinear(100), "spent {spent}");
    Ok(())
}

/// The fixed values table of the build spec, as `fc_rules` reports them.
fn spec_fixed_values() -> serde_json::Value {
    json!({
        "virtual_near": "100000000000000000000000000",
        "p_bps": 200,
        "window_unmarked_s": 3600,
        "window_early_s": 86400,
        "early_blocks": 17,
        "large_threshold_bps": 200,
        "creator_cap_bps": 400,
        "real_cap_c_bps": 500,
        "floor_bps": 10,
        "penalty_bps": 1000,
        "swap_fee_bps": 100,
        "brake_drop_bps": null,
        "early_fee_start_bps": 5000,
        "claim_depth_hold_s": 86400,
        "creator_fee_share_bps": 3000,
        "claim_min_real_near": "10000000000000000000000000",
        "unclaimed_to_pool_s": null,
    })
}

/// `fc_rules` without the fields that differ per coin by design.
fn shared_rules(mut rules: serde_json::Value) -> serde_json::Value {
    let object = rules.as_object_mut().unwrap();
    for per_coin in ["creator", "launch_block", "launch_ns", "description"] {
        object.remove(per_coin);
    }
    rules
}

/// One event's data object without the named fields.
fn data_without(event: &serde_json::Value, fields: &[&str]) -> serde_json::Value {
    let mut data = event["data"][0].clone();
    for field in fields {
        data.as_object_mut().unwrap().remove(*field);
    }
    data
}

#[tokio::test]
async fn two_coins_have_equal_fixed_rules_and_independent_state() -> Result<()> {
    let world = World::launch().await?;
    let second = world.coin_for("second")?;
    let launched = world
        .try_launch(
            &world.alice,
            launch_args(
                "Second",
                "second",
                Some(COIN_ICON),
                Some("Another coin."),
                None,
            ),
            LAUNCH_DEPOSIT,
        )
        .await?;
    assert!(launched.is_success(), "{launched:?}");

    let first_rules = world.view(&world.coin, "fc_rules", json!({})).await?;
    let second_rules = world.view(&second, "fc_rules", json!({})).await?;
    assert_eq!(
        shared_rules(first_rules.clone()),
        shared_rules(second_rules.clone())
    );
    assert_eq!(second_rules["code_hash"], world.coin_code_hash.to_string());
    for (name, value) in spec_fixed_values().as_object().unwrap() {
        assert_eq!(&first_rules[name], value, "fc_rules.{name}");
    }
    assert_eq!(second_rules["creator"], world.alice.id.to_string());
    assert_eq!(second_rules["description"], "Another coin.");
    assert_ne!(first_rules["launch_block"], second_rules["launch_block"]);

    let second_pool = world.view(&second, "fc_pool", json!({})).await?;
    let _ = world.register(&world.creator, &world.coin).await?;
    let _bought = world
        .call(
            &world.creator,
            &world.coin,
            "buy",
            json!({ "min_tokens_out": "0" }),
            NearToken::from_near(1),
        )
        .await?;
    let first_pool = world.view(&world.coin, "fc_pool", json!({})).await?;
    assert_eq!(
        first_pool["real_near"],
        NearToken::from_near(1).as_yoctonear().to_string()
    );
    let second_pool_after = world.view(&second, "fc_pool", json!({})).await?;
    assert_eq!(
        data_without(&json!({ "data": [second_pool] }), &["block"]),
        data_without(&json!({ "data": [second_pool_after] }), &["block"]),
    );
    let balance_on_second = world
        .view(
            &second,
            "ft_balance_of",
            json!({ "account_id": world.creator.id }),
        )
        .await?;
    assert_eq!(balance_on_second, "0");
    Ok(())
}

#[tokio::test]
async fn coin_methods_called_by_the_factory_give_the_same_result_as_for_any_account() -> Result<()>
{
    let world = World::launch().await?;
    let other = world.coin_for("other")?;
    let launched = world
        .try_launch(
            &world.alice,
            launch_args("Other", "other", None, None, None),
            LAUNCH_DEPOSIT,
        )
        .await?;
    assert!(launched.is_success(), "{launched:?}");

    // Same pool state on both coins, both after the early window, so only the caller differs:
    // the factory (parent of the first coin) and the creator account (outside the second
    // coin's creator group).
    let early_blocks = 17;
    let last_launch = world.view(&other, "fc_rules", json!({})).await?["launch_block"]
        .as_u64()
        .unwrap();
    world
        .wait_for_height(last_launch + early_blocks + 2)
        .await?;

    let one_near = NearToken::from_near(1);
    let _ = world.register(&world.factory, &world.coin).await?;
    let factory_buy = world
        .call(
            &world.factory,
            &world.coin,
            "buy",
            json!({ "min_tokens_out": "0" }),
            one_near,
        )
        .await?;
    let _ = world.register(&world.creator, &other).await?;
    let other_buy = world
        .call(
            &world.creator,
            &other,
            "buy",
            json!({ "min_tokens_out": "0" }),
            one_near,
        )
        .await?;
    let factory_buy = &events(&factory_buy, "fair_chance", "fc_buy")[0].event;
    let other_buy = &events(&other_buy, "fair_chance", "fc_buy")[0].event;
    assert_eq!(
        data_without(factory_buy, &["account", "block"]),
        data_without(other_buy, &["account", "block"])
    );

    for (caller, coin) in [(&world.factory, &world.coin), (&world.creator, &other)] {
        let failure = world
            .call_refused(
                caller,
                coin,
                "sell",
                json!({ "amount": "1", "min_near_out": "0" }),
                NearToken::from_yoctonear(0),
            )
            .await?;
        assert!(failure.contains("FC_ONE_YOCTO"), "{failure}");
    }

    let mut sells = Vec::new();
    for (caller, coin) in [(&world.factory, &world.coin), (&world.creator, &other)] {
        let balance = world
            .view(coin, "ft_balance_of", json!({ "account_id": caller.id }))
            .await?;
        let sold = world
            .call(
                caller,
                coin,
                "sell",
                json!({ "amount": balance, "min_near_out": "0" }),
                NearToken::from_yoctonear(1),
            )
            .await?;
        sells.push(events(&sold, "fair_chance", "fc_sell")[0].event.clone());
    }
    assert_eq!(
        data_without(&sells[0], &["account", "window_end_ns"]),
        data_without(&sells[1], &["account", "window_end_ns"])
    );
    Ok(())
}

#[tokio::test]
async fn an_opening_buy_is_the_creators_buy_inside_new() -> Result<()> {
    let world = World::launch().await?;
    let coin = world.coin_for("opening")?;
    let opening_buy = NearToken::from_near(1);
    let result = world
        .try_launch(
            &world.alice,
            launch_args("Opening", "opening", None, None, Some(opening_buy)),
            LAUNCH_DEPOSIT.saturating_add(opening_buy),
        )
        .await?;
    assert!(result.is_success(), "{result:?}");

    let buys = events(&result, "fair_chance", "fc_buy");
    assert_eq!(buys.len(), 1);
    let buy = &buys[0].event["data"][0];
    assert_eq!(buys[0].executor, coin.to_string());
    assert_eq!(buy["account"], world.alice.id.to_string());
    assert_eq!(buy["near_in"], opening_buy.as_yoctonear().to_string());

    let rules = world.view(&coin, "fc_rules", json!({})).await?;
    assert_eq!(buy["block"], rules["launch_block"]);
    let balance = world
        .view(
            &coin,
            "ft_balance_of",
            json!({ "account_id": world.alice.id }),
        )
        .await?;
    assert_eq!(balance, buy["tokens_out"]);
    assert_ne!(balance, "0");
    let pool = world.view(&coin, "fc_pool", json!({})).await?;
    assert_eq!(pool["real_near"], opening_buy.as_yoctonear().to_string());
    assert_eq!(events(&result, "fair_chance_factory", "fc_launch").len(), 1);
    Ok(())
}

#[tokio::test]
async fn an_opening_buy_of_the_whole_deposit_is_refused() -> Result<()> {
    let world = World::launch().await?;
    let failure = world
        .call_refused(
            &world.alice,
            &world.factory.id,
            "launch",
            launch_args("Greedy", "greedy", None, None, Some(LAUNCH_DEPOSIT)),
            LAUNCH_DEPOSIT,
        )
        .await?;
    assert!(
        failure.contains("opening_buy must be less than the attached deposit"),
        "{failure}"
    );
    assert_eq!(
        world.contract_state(&world.coin_for("greedy")?).await?,
        None
    );
    Ok(())
}

/// NEAR protocol storage price: 10^19 yoctoNEAR per byte.
const YOCTO_PER_STORAGE_BYTE: u128 = 10_000_000_000_000_000_000;

/// The coin's NEAR balance minus its storage lock: the most NEAR it can pay out.
async fn payable_near(world: &World, coin: &near_api::types::AccountId) -> Result<u128> {
    let account = near_api::Account(coin.clone())
        .view()
        .fetch_from(&world.network)
        .await?
        .data;
    let lock = u128::from(account.storage_usage) * YOCTO_PER_STORAGE_BYTE;
    Ok(account.amount.as_yoctonear().saturating_sub(lock))
}

#[tokio::test]
async fn after_an_opening_buy_the_coin_can_pay_out_all_of_its_real_near() -> Result<()> {
    let world = World::launch().await?;
    let coin = world.coin_for("covered")?;
    let opening_buy = NearToken::from_near(1);
    let result = world
        .try_launch(
            &world.alice,
            launch_args("Covered", "covered", None, None, Some(opening_buy)),
            LAUNCH_DEPOSIT.saturating_add(opening_buy),
        )
        .await?;
    assert_eq!(events(&result, "fair_chance_factory", "fc_launch").len(), 1);
    let pool = world.view(&coin, "fc_pool", json!({})).await?;
    let real_near: u128 = pool["real_near"].as_str().unwrap().parse()?;
    assert!(payable_near(&world, &coin).await? >= real_near);
    Ok(())
}

#[tokio::test]
async fn an_opening_buy_that_leaves_too_little_for_storage_makes_no_coin() -> Result<()> {
    let world = World::launch().await?;
    let coin = world.coin_for("thin")?;
    let opening_buy = NearToken::from_near(1);
    let before = world.balance(&world.alice.id).await?;
    let result = world
        .try_launch(
            &world.alice,
            launch_args("Thin", "thin", None, None, Some(opening_buy)),
            opening_buy.saturating_add(NearToken::from_yoctonear(1)),
        )
        .await?;
    assert!(events(&result, "fair_chance_factory", "fc_launch").is_empty());
    assert_eq!(world.contract_state(&coin).await?, None);
    let spent = before.saturating_sub(world.balance(&world.alice.id).await?);
    assert!(spent < NearToken::from_millinear(100), "spent {spent}");
    Ok(())
}

/// 4% of the 1,000,000,000-token supply, in base units (18 decimals): the creator-group cap.
const CREATOR_CAP_TOKENS: u128 = 40_000_000 * 10u128.pow(18);

#[tokio::test]
async fn an_opening_buy_is_marked_and_inside_the_creator_group_cap() -> Result<()> {
    let world = World::launch().await?;
    let coin = world.coin_for("marked")?;
    // At block 0 the buy fee is 50%, so 8 NEAR buys about 3.85% of supply.
    let opening_buy = NearToken::from_near(8);
    let result = world
        .try_launch(
            &world.bob,
            launch_args("Marked", "marked", None, None, Some(opening_buy)),
            LAUNCH_DEPOSIT.saturating_add(opening_buy),
        )
        .await?;
    assert_eq!(events(&result, "fair_chance_factory", "fc_launch").len(), 1);
    let buy = &events(&result, "fair_chance", "fc_buy")[0].event["data"][0];
    assert_eq!(buy["marked_tokens"], buy["tokens_out"]);
    let tokens_out: u128 = buy["tokens_out"].as_str().unwrap().parse()?;
    assert!(
        tokens_out > 0 && tokens_out <= CREATOR_CAP_TOKENS,
        "{tokens_out}"
    );

    let account = world
        .view(&coin, "fc_account", json!({ "account_id": world.bob.id }))
        .await?;
    assert_eq!(account["in_creator_group"], true);
    assert_eq!(account["marked_balance"], buy["tokens_out"]);
    assert_eq!(account["unmarked_balance"], "0");
    Ok(())
}

#[tokio::test]
async fn an_opening_buy_above_the_creator_group_cap_is_refused_and_makes_no_coin() -> Result<()> {
    let world = World::launch().await?;
    let coin = world.coin_for("toolarge")?;
    // At block 0 the buy fee is 50%, so 10 NEAR would buy about 4.76% of supply.
    let opening_buy = NearToken::from_near(10);
    let result = world
        .try_launch(
            &world.bob,
            launch_args("Too Large", "toolarge", None, None, Some(opening_buy)),
            LAUNCH_DEPOSIT.saturating_add(opening_buy),
        )
        .await?;
    assert!(events(&result, "fair_chance_factory", "fc_launch").is_empty());
    assert!(
        format!("{result:?}").contains("FC_CREATOR_CAP"),
        "the launch did not fail with FC_CREATOR_CAP: {result:?}"
    );
    assert_eq!(world.contract_state(&coin).await?, None);
    Ok(())
}
