//! The factory `launch` batch on a real chain (ADR 0006): CreateAccount, Transfer,
//! UseGlobalContract by code hash, FunctionCall `new`, and no AddKey.

use anyhow::Result;
use fair_chance_sandbox_tests::{
    events, launch_args, World, COIN_DESCRIPTION, COIN_ICON, LAUNCH_DEPOSIT,
};
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
