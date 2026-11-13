//! The coin's state grows after `new`: the creator fee depth record gains its start time when
//! the first 1 h window with at least 10 real NEAR closes. On a real chain that growth must not
//! come out of the NEAR the coin owes. The coin launches with the smallest deposit `new` accepts
//! (found by a search on the sandbox), a buyer buys 11 NEAR, the sandbox fast-forwards past 2 h,
//! and the buyer sells 1 token unit. After each step the coin's balance must be at least its
//! real NEAR + its creator fee balance + its storage cost.
//!
//! The steps print as they pass (`--nocapture`).

use anyhow::{bail, Context, Result};
use fair_chance_sandbox_tests::{launch_args, World};
use near_api::types::{AccountId, NearToken};
use serde_json::{json, Value};

/// NEAR protocol storage price: 10^19 yoctoNEAR per byte.
const YOCTO_PER_STORAGE_BYTE: u128 = 10_000_000_000_000_000_000;
/// The search for the smallest launch deposit stops at this step.
const SEARCH_STEP: u128 = 10_000_000_000_000_000;
/// Blocks per fast-forward: about 2.5 h of block time on a [`World::launch_for_fast_forward`]
/// sandbox (about 93 s per block), so the next action is in the coin's window 2 and closes
/// window 1.
const FAST_FORWARD_BLOCKS: u64 = 100;

fn amount(value: &Value) -> Result<u128> {
    Ok(value.as_str().context("amount string")?.parse()?)
}

/// What the coin holds and what it owes: (balance, real NEAR + creator balance + storage cost).
async fn held_and_owed(world: &World, coin: &AccountId) -> Result<(u128, u128)> {
    let account = near_api::Account(coin.clone())
        .view()
        .fetch_from(&world.network)
        .await?
        .data;
    let pool = world.view(coin, "fc_pool", json!({})).await?;
    let owed = amount(&pool["real_near"])?
        + amount(&pool["creator_fee_balance"])?
        + u128::from(account.storage_usage) * YOCTO_PER_STORAGE_BYTE;
    Ok((account.amount.as_yoctonear(), owed))
}

async fn check(world: &World, coin: &AccountId, step: &str) -> Result<()> {
    let (held, owed) = held_and_owed(world, coin).await?;
    println!("{step}: balance {held}, real NEAR + creator balance + storage {owed}");
    assert!(
        held >= owed,
        "{step}: the coin holds {held} but owes {owed} ({} short)",
        owed - held
    );
    println!("PASS {step}");
    Ok(())
}

/// Tries one launch of `ticker` with `deposit` and no opening buy. True when the coin exists
/// after it: when `new` refuses, the factory's callback refunds the deposit and the factory call
/// itself still succeeds, so the call result does not tell.
async fn launches(world: &World, ticker: &str, deposit: u128) -> Result<bool> {
    let _ = world
        .try_launch(
            &world.creator,
            launch_args("Tight Coin", ticker, None, None, None),
            NearToken::from_yoctonear(deposit),
        )
        .await?;
    Ok(world
        .contract_state(&world.coin_for(ticker)?)
        .await?
        .is_some())
}

#[tokio::test]
async fn the_coin_balance_covers_real_near_the_creator_balance_and_storage_after_its_state_grows(
) -> Result<()> {
    let world = World::launch_for_fast_forward().await?;

    // A probe coin with a ticker of the same length shows the storage a launch uses.
    let probe = world.coin_for("probe")?;
    assert!(launches(&world, "probe", 5 * 10u128.pow(24)).await?);
    let probe_usage = near_api::Account(probe.clone())
        .view()
        .fetch_from(&world.network)
        .await?
        .data
        .storage_usage;
    // The smallest deposit `new` accepts, to SEARCH_STEP: the storage cost is enough, and the
    // gas reward of `new` can make a little less enough too. Each try has its own ticker of the
    // same length, so every coin uses the same storage; the coin of the last success stays.
    let mut enough = u128::from(probe_usage) * YOCTO_PER_STORAGE_BYTE;
    let mut too_little = enough - enough / 10;
    let mut tries = 0;
    let mut ticker = || {
        tries += 1;
        format!("t{tries:04}")
    };
    if launches(&world, &ticker(), too_little).await? {
        bail!("a launch with 90% of the storage cost went through");
    }
    let mut coin_ticker = ticker();
    assert!(launches(&world, &coin_ticker, enough).await?);
    while enough - too_little > SEARCH_STEP {
        let middle = too_little + (enough - too_little) / 2;
        let next = ticker();
        if launches(&world, &next, middle).await? {
            enough = middle;
            coin_ticker = next;
        } else {
            too_little = middle;
        }
    }
    let coin = world.coin_for(&coin_ticker)?;
    println!(
        "smallest deposit found: {enough} (storage cost of the probe {})",
        u128::from(probe_usage) * YOCTO_PER_STORAGE_BYTE
    );
    check(&world, &coin, "after a launch with the smallest deposit").await?;

    world.wait_until_after_the_early_window(&coin).await?;
    let _ = world.register(&world.alice, &coin).await?;
    let _ = world
        .call(
            &world.alice,
            &coin,
            "buy",
            json!({ "min_tokens_out": "0" }),
            NearToken::from_near(11),
        )
        .await?;
    check(&world, &coin, "after alice registers and buys 11 NEAR").await?;
    let before = world.view(&coin, "fc_creator_fees", json!({})).await?;

    world.sandbox.fast_forward(FAST_FORWARD_BLOCKS).await?;
    let _ = world
        .call(
            &world.alice,
            &coin,
            "sell",
            json!({ "amount": "1", "min_near_out": "0" }),
            NearToken::from_yoctonear(1),
        )
        .await?;
    let after = world.view(&coin, "fc_creator_fees", json!({})).await?;
    println!(
        "depth_ok_since_ns before the sell {}, after {}",
        before["depth_ok_since_ns"], after["depth_ok_since_ns"]
    );
    assert!(before["depth_ok_since_ns"].is_null());
    assert!(!after["depth_ok_since_ns"].is_null());
    check(
        &world,
        &coin,
        "after 2 h and a sell of 1 token unit (the depth record grows)",
    )
    .await
}
