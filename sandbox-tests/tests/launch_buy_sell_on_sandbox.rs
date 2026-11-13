use anyhow::Result;
use fair_chance_sandbox_tests::{
    events, field_names, World, COIN_DESCRIPTION, COIN_ICON, FC_SELL_FIELDS,
};
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
    assert_eq!(rules["description"], COIN_DESCRIPTION);
    assert_eq!(rules["brake_drop_bps"], serde_json::Value::Null);

    let metadata = world.view(&world.coin, "ft_metadata", json!({})).await?;
    assert_eq!(metadata["icon"], COIN_ICON);
    assert!(COIN_ICON.starts_with("data:image/webp;base64,") && COIN_ICON.len() < 8 * 1024);

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
    let failure = world
        .call_refused(
            &world.alice,
            &world.coin,
            "sell",
            json!({ "amount": "1", "min_near_out": "0" }),
            NearToken::from_yoctonear(2),
        )
        .await?;
    assert!(failure.contains("FC_ONE_YOCTO"), "{failure}");
    Ok(())
}

#[tokio::test]
async fn a_buy_below_min_tokens_out_is_refused_with_fc_slippage_and_a_quote_matches_the_buy(
) -> Result<()> {
    let world = World::launch().await?;
    let quote = world
        .view(
            &world.coin,
            "fc_quote_buy",
            json!({ "account_id": world.alice.id, "near_in": NearToken::from_near(1).as_yoctonear().to_string() }),
        )
        .await?;
    let tokens_out: u128 = quote["tokens_out"].as_str().unwrap().parse()?;
    let failure = world
        .call_refused(
            &world.alice,
            &world.coin,
            "buy",
            json!({ "min_tokens_out": (tokens_out + 1).to_string() }),
            NearToken::from_near(1),
        )
        .await?;
    assert!(failure.contains("FC_SLIPPAGE"), "{failure}");
    let bought = world
        .call(
            &world.alice,
            &world.coin,
            "buy",
            json!({ "min_tokens_out": tokens_out.to_string() }),
            NearToken::from_near(1),
        )
        .await?;
    let buys = events(&bought, "fair_chance", "fc_buy");
    assert_eq!(buys.len(), 1);
    assert_eq!(buys[0].event["data"][0]["tokens_out"], quote["tokens_out"]);
    let transfers = events(&bought, "nep141", "ft_transfer");
    assert_eq!(transfers.len(), 1);
    assert_eq!(
        transfers[0].event["data"][0]["old_owner_id"],
        world.coin.to_string()
    );
    assert!(events(&bought, "nep141", "ft_mint").is_empty());
    Ok(())
}

#[tokio::test]
async fn a_send_to_a_registered_account_logs_fc_send_and_ft_transfer_call_is_refused() -> Result<()>
{
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
    let bounds = world
        .view(&world.coin, "storage_balance_bounds", json!({}))
        .await?;
    let min: u128 = bounds["min"].as_str().unwrap().parse()?;
    let receiver = world.factory.id.clone();
    let _registered = world
        .call(
            &world.factory,
            &world.coin,
            "storage_deposit",
            json!({}),
            NearToken::from_yoctonear(min),
        )
        .await?;

    let sent = world
        .call(
            &world.alice,
            &world.coin,
            "ft_transfer",
            json!({ "receiver_id": receiver, "amount": "1000" }),
            NearToken::from_yoctonear(1),
        )
        .await?;
    let sends = events(&sent, "fair_chance", "fc_send");
    assert_eq!(sends.len(), 1);
    assert_eq!(
        field_names(&sends[0].event),
        ["amount", "from", "marked_amount", "to", "to_marked_amount"]
    );
    assert_eq!(sends[0].event["data"][0]["amount"], "1000");
    let balance = world
        .view(
            &world.coin,
            "ft_balance_of",
            json!({ "account_id": receiver }),
        )
        .await?;
    assert_eq!(balance, "1000");

    let failure = world
        .call_refused(
            &world.alice,
            &world.coin,
            "ft_transfer_call",
            json!({ "receiver_id": receiver, "amount": "1", "msg": "" }),
            NearToken::from_yoctonear(1),
        )
        .await?;
    assert!(failure.contains("FC_TRANSFER_CALL"), "{failure}");
    Ok(())
}
