use anyhow::Result;
use fair_chance_sandbox_tests::{events, field_names, World, FC_SELL_FIELDS};
use near_api::types::NearToken;
use serde_json::json;

#[tokio::test]
async fn launch_runs_the_batch_and_emits_fc_launch() -> Result<()> {
    let world = World::launch().await?;

    let launches = events(&world.launch, "fair_chance_factory", "fc_launch");
    assert_eq!(launches.len(), 1);
    let data = &launches[0].event["data"][0];
    assert_eq!(data["coin"], world.coin.to_string());
    assert_eq!(data["creator"], world.creator.id.to_string());
    assert_eq!(data["code_hash"], world.coin_code_hash.to_string());

    let rules = world.view(&world.coin, "fc_rules", json!({})).await?;
    assert_eq!(rules["code_hash"], world.coin_code_hash.to_string());
    assert_eq!(rules["creator"], world.creator.id.to_string());

    let source = world
        .view(&world.coin, "contract_source_metadata", json!({}))
        .await?;
    assert_eq!(source["version"], rule_engine::INTERFACE_VERSION);

    let mints = events(&world.launch, "nep141", "ft_mint");
    assert_eq!(mints.len(), 1);
    assert_eq!(mints[0].executor, world.coin.to_string());
    Ok(())
}

#[tokio::test]
async fn a_sell_receipt_logs_fc_sell_with_the_interface_field_names() -> Result<()> {
    let world = World::launch().await?;
    let _bought = world
        .call(
            &world.alice,
            &world.coin,
            "buy",
            json!({ "min_tokens_out": "0" }),
            NearToken::from_near(1),
        )
        .await?;
    let balance = world
        .view(
            &world.coin,
            "ft_balance_of",
            json!({ "account_id": world.alice.id }),
        )
        .await?;
    let sell = world
        .call(
            &world.alice,
            &world.coin,
            "sell",
            json!({ "amount": balance, "min_near_out": "0" }),
            NearToken::from_yoctonear(1),
        )
        .await?;

    let sells = events(&sell, "fair_chance", "fc_sell");
    assert_eq!(sells.len(), 1);
    assert_eq!(sells[0].executor, world.coin.to_string());
    assert_eq!(sells[0].event["version"], rule_engine::INTERFACE_VERSION);
    let mut expected: Vec<String> = FC_SELL_FIELDS.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(field_names(&sells[0].event), expected);
    assert_eq!(sells[0].event["data"][0]["tokens_sold"], balance);
    Ok(())
}

#[tokio::test]
async fn a_sell_with_no_deposit_is_refused_with_fc_one_yocto() -> Result<()> {
    let world = World::launch().await?;
    let _bought = world
        .call(
            &world.alice,
            &world.coin,
            "buy",
            json!({ "min_tokens_out": "0" }),
            NearToken::from_near(1),
        )
        .await?;
    let failure = world
        .call_refused(
            &world.alice,
            &world.coin,
            "sell",
            json!({ "amount": "1", "min_near_out": "0" }),
            NearToken::from_yoctonear(0),
        )
        .await?;
    assert!(failure.contains("FC_ONE_YOCTO"), "{failure}");
    Ok(())
}
