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
