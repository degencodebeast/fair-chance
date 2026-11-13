//! A buy from an account with no NEP-145 storage record, on a real chain (INTERFACE `buy`): it is
//! refused with the NEP-145 text before any state change, so the attached NEAR comes back; and a
//! wallet can send `storage_deposit` and `buy` as two function-call actions in one transaction.

use anyhow::{Context, Result};
use fair_chance_sandbox_tests::{events, FunctionCall, World};
use near_api::types::{AccountId, NearToken};
use serde_json::{json, Value};

/// What a refused buy must leave unchanged on the coin: the pool view (without its block), the
/// coin's storage use (one more holder record would raise it) and the buyer's storage record.
async fn coin_state(world: &World, buyer: &AccountId) -> Result<(Value, u64, Value)> {
    let mut pool = world.view(&world.coin, "fc_pool", json!({})).await?;
    pool.as_object_mut()
        .context("fc_pool object")?
        .remove("block");
    let storage_usage = near_api::Account(world.coin.clone())
        .view()
        .fetch_from(&world.network)
        .await?
        .data
        .storage_usage;
    let record = world
        .view(
            &world.coin,
            "storage_balance_of",
            json!({ "account_id": buyer }),
        )
        .await?;
    Ok((pool, storage_usage, record))
}

async fn storage_minimum(world: &World) -> Result<NearToken> {
    let bounds = world
        .view(&world.coin, "storage_balance_bounds", json!({}))
        .await?;
    Ok(NearToken::from_yoctonear(
        bounds["min"].as_str().context("storage min")?.parse()?,
    ))
}

fn buy_call(min_tokens_out: &str, near_in: NearToken) -> FunctionCall {
    FunctionCall::new("buy", json!({ "min_tokens_out": min_tokens_out }), near_in)
}

#[tokio::test]
async fn a_buy_with_no_storage_record_is_refused_with_the_nep145_text_and_returns_the_near(
) -> Result<()> {
    let world = World::launch().await?;
    let alice = &world.alice;
    let before = coin_state(&world, &alice.id).await?;
    assert_eq!(before.2, Value::Null, "alice starts with no storage record");
    let balance_before = world.balance(&alice.id).await?.as_yoctonear();

    let result = world
        .batch(
            alice,
            &world.coin,
            vec![buy_call("0", NearToken::from_near(2))],
        )
        .await?;

    assert!(!result.is_success());
    let failure = format!("{result:?}");
    assert!(
        failure.contains(&format!("The account is not registered: {}", alice.id)),
        "{failure}"
    );
    assert_eq!(coin_state(&world, &alice.id).await?, before);
    let spent = balance_before - world.balance(&alice.id).await?.as_yoctonear();
    assert_eq!(
        spent,
        World::tokens_burnt(&result)?,
        "alice pays gas and nothing else"
    );
    Ok(())
}

#[tokio::test]
async fn storage_deposit_and_buy_in_one_transaction_register_and_buy() -> Result<()> {
    let world = World::launch().await?;
    let alice = &world.alice;
    let min = storage_minimum(&world).await?;

    let result = world
        .batch(
            alice,
            &world.coin,
            vec![
                FunctionCall::new("storage_deposit", json!({}), min),
                buy_call("0", NearToken::from_near(1)),
            ],
        )
        .await?;

    assert!(result.is_success(), "{result:?}");
    let buys = events(&result, "fair_chance", "fc_buy");
    assert_eq!(buys.len(), 1);
    assert_eq!(buys[0].event["data"][0]["account"], alice.id.to_string());
    let record = world
        .view(
            &world.coin,
            "storage_balance_of",
            json!({ "account_id": alice.id }),
        )
        .await?;
    assert_eq!(record["total"], min.as_yoctonear().to_string());
    let held = world
        .view(
            &world.coin,
            "ft_balance_of",
            json!({ "account_id": alice.id }),
        )
        .await?;
    assert_ne!(held, "0");
    Ok(())
}

#[tokio::test]
async fn a_one_transaction_first_buy_that_fails_slippage_leaves_no_record_and_returns_all_the_near(
) -> Result<()> {
    let world = World::launch().await?;
    let alice = &world.alice;
    let min = storage_minimum(&world).await?;
    let before = coin_state(&world, &alice.id).await?;
    let balance_before = world.balance(&alice.id).await?.as_yoctonear();

    // No buy of 1 NEAR gets the whole supply, so this minimum always fails slippage.
    let total_supply = world
        .view(&world.coin, "ft_total_supply", json!({}))
        .await?;
    let result = world
        .batch(
            alice,
            &world.coin,
            vec![
                FunctionCall::new("storage_deposit", json!({}), min),
                buy_call(
                    total_supply.as_str().context("supply")?,
                    NearToken::from_near(1),
                ),
            ],
        )
        .await?;

    assert!(!result.is_success());
    let failure = format!("{result:?}");
    assert!(failure.contains("FC_SLIPPAGE"), "{failure}");
    assert_eq!(coin_state(&world, &alice.id).await?, before);
    let spent = balance_before - world.balance(&alice.id).await?.as_yoctonear();
    assert_eq!(
        spent,
        World::tokens_burnt(&result)?,
        "alice pays gas and nothing else"
    );
    Ok(())
}
