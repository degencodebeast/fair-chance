//! ADR 0005, on a real chain: a band account that buys and sells back in the same window counts
//! the bought part toward the real NEAR cap, net of its buy. Its first allowance (with room back
//! for the tokens it bought) fills with no penalty. For the cap count the bought tokens go first
//! and count; only the allowance of tokens held before the window never counts.

use anyhow::{Context, Result};
use ethnum::U256;
use fair_chance_sandbox_tests::{events, World};
use near_api::types::NearToken;
use serde_json::{json, Value};

fn amount(value: &Value) -> Result<u128> {
    Ok(value.as_str().context("amount string")?.parse()?)
}

#[tokio::test]
async fn a_band_account_that_buys_and_sells_back_plus_its_allowance_counts_the_bought_part_net_of_its_buy(
) -> Result<()> {
    let world = World::launch().await?;
    let coin = &world.coin;
    let _ = world.register(&world.bob, coin).await?;
    let _ = world.register(&world.alice, coin).await?;
    // bob buys 1.5 NEAR (about 1.46% of supply) and sends it all to alice, so alice holds a
    // band bag with no trade of its own in this window.
    let _ = world
        .call(
            &world.bob,
            coin,
            "buy",
            json!({ "min_tokens_out": "0" }),
            NearToken::from_millinear(1_500),
        )
        .await?;
    let held = world
        .view(coin, "ft_balance_of", json!({ "account_id": world.bob.id }))
        .await?;
    let _ = world
        .call(
            &world.bob,
            coin,
            "ft_transfer",
            json!({ "receiver_id": world.alice.id, "amount": held }),
            NearToken::from_yoctonear(1),
        )
        .await?;
    let alice = world
        .view(coin, "fc_account", json!({ "account_id": world.alice.id }))
        .await?;
    assert_eq!(alice["class"], "band");
    let allowance = amount(&alice["unmarked"]["allowance"])?;

    // alice buys 0.3 NEAR of tokens, then sells those tokens plus one allowance.
    let _ = world
        .call(
            &world.alice,
            coin,
            "buy",
            json!({ "min_tokens_out": "0" }),
            NearToken::from_millinear(300),
        )
        .await?;
    let bought = amount(
        &world
            .view(
                coin,
                "ft_balance_of",
                json!({ "account_id": world.alice.id }),
            )
            .await?,
    )? - amount(&held)?;
    let pool = world.view(coin, "fc_pool", json!({})).await?;
    let sell = world
        .call(
            &world.alice,
            coin,
            "sell",
            json!({ "amount": (bought + allowance).to_string(), "min_near_out": "0" }),
            NearToken::from_yoctonear(1),
        )
        .await?;
    let sold = &events(&sell, "fair_chance", "fc_sell")[0].event["data"][0];
    assert_eq!(sold["fill"], "full");
    assert_eq!(sold["penalty_near"], "0");
    let near_out = amount(&sold["near_out"])?;

    // The bought part counts and goes first: its NEAR out at the pool before the sell, by the
    // constant-product formula (NEAR out rounded down, the 1% fee rounded up). The account's
    // net is that minus the 0.3 NEAR it paid, below zero, so the cap used stays 0.
    let (near_reserve, token_reserve) = (
        amount(&pool["near_reserve"])?,
        amount(&pool["token_reserve"])?,
    );
    let before_fee = (U256::from(near_reserve) * U256::from(bought)
        / U256::from(token_reserve + bought))
    .as_u128();
    let bought_part_out = before_fee - before_fee.div_ceil(100);
    let paid = NearToken::from_millinear(300).as_yoctonear();
    assert!(bought_part_out > 0 && bought_part_out < paid);
    assert!(near_out > bought_part_out);
    assert_eq!(amount(&sold["cap_used"])?, 0);
    let cap = world.view(coin, "fc_cap", json!({})).await?;
    assert_eq!(amount(&cap["cap_used"])?, 0);
    Ok(())
}
