//! ADR 0005 known limit, on a real chain: a band account that buys and sells back in the same
//! window counts the bought part toward the real NEAR cap. Its first allowance (with room back
//! for the tokens it bought) fills with no penalty, but only the allowance itself never counts.

use anyhow::{Context, Result};
use ethnum::U256;
use fair_chance_sandbox_tests::{events, World};
use near_api::types::NearToken;
use serde_json::{json, Value};

fn amount(value: &Value) -> Result<u128> {
    Ok(value.as_str().context("amount string")?.parse()?)
}

#[tokio::test]
async fn a_band_account_that_buys_and_sells_back_plus_its_allowance_counts_the_bought_part_toward_the_cap(
) -> Result<()> {
    let world = World::launch().await?;
    let coin = &world.coin;
    world.register(&world.bob, coin).await?;
    world.register(&world.alice, coin).await?;
    // bob buys 1.5 NEAR (about 1.46% of supply) and sends it all to alice, so alice holds a
    // band bag with no trade of its own in this window.
    world
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
    world
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
    world
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

    // The allowance part never counts: its NEAR out at the pool before the sell, by the
    // constant-product formula (NEAR out rounded down, the 1% fee rounded up).
    let (near_reserve, token_reserve) = (
        amount(&pool["near_reserve"])?,
        amount(&pool["token_reserve"])?,
    );
    let before_fee = (U256::from(near_reserve) * U256::from(allowance)
        / U256::from(token_reserve + allowance))
    .as_u128();
    let allowance_part_out = before_fee - before_fee.div_ceil(100);
    let bought_part_out = near_out - allowance_part_out;
    assert!(bought_part_out > 0);
    assert_eq!(amount(&sold["cap_used"])?, bought_part_out);
    let cap = world.view(coin, "fc_cap", json!({})).await?;
    assert_eq!(amount(&cap["cap_used"])?, bought_part_out);
    Ok(())
}
