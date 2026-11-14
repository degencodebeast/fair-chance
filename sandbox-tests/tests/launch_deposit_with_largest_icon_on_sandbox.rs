//! A launch with no opening buy and the largest icon the coin takes (a WebP data URL of 8191
//! bytes) goes through with a 0.25 NEAR deposit: the coin exists, `ft_metadata` returns that
//! icon, and the coin's balance covers its real NEAR, its creator balance and its storage. The
//! same launch with 0.20 NEAR is tried too, to show the margin; when it fails, a search finds the
//! smallest deposit that works, to 1 milliNEAR. The numbers print as they pass (`--nocapture`).

use anyhow::{Context, Result};
use fair_chance_sandbox_tests::{launch_args, World, COIN_DESCRIPTION};
use near_api::types::{AccountId, NearToken};
use serde_json::{json, Value};

/// NEAR protocol storage price: 10^19 yoctoNEAR per byte.
const YOCTO_PER_STORAGE_BYTE: u128 = 10_000_000_000_000_000_000;
const ICON_PREFIX: &str = "data:image/webp;base64,";
/// The largest icon the coin takes: under 8 KB, so 8 * 1024 - 1 bytes.
const LARGEST_ICON_BYTES: usize = 8191;

/// The prefix and base64 characters up to 8191 bytes. The coin checks the prefix, the length and
/// the base64 alphabet; it does not decode the picture.
fn largest_icon() -> String {
    let icon = format!(
        "{ICON_PREFIX}{}",
        "A".repeat(LARGEST_ICON_BYTES - ICON_PREFIX.len())
    );
    assert_eq!(icon.len(), LARGEST_ICON_BYTES);
    icon
}

fn amount(value: &Value) -> Result<u128> {
    Ok(value.as_str().context("amount string")?.parse()?)
}

fn near(yocto: u128) -> String {
    format!("{:.6} NEAR", yocto as f64 / 1e24)
}

/// What one launch left: None when no coin exists after it.
struct Outcome {
    icon: Option<String>,
    balance: u128,
    storage_bytes: u64,
    owed: u128,
}

async fn launch(world: &World, ticker: &str, deposit: NearToken) -> Result<Option<Outcome>> {
    let icon = largest_icon();
    let _ = world
        .try_launch(
            &world.creator,
            launch_args(
                "Deposit Check",
                ticker,
                Some(&icon),
                Some(COIN_DESCRIPTION),
                None,
            ),
            deposit,
        )
        .await?;
    let coin: AccountId = world.coin_for(ticker)?;
    // When `new` refuses, the factory refunds the deposit and its own call still succeeds, so
    // only the coin account tells whether the launch went through.
    if world.contract_state(&coin).await?.is_none() {
        return Ok(None);
    }
    let account = near_api::Account(coin.clone())
        .view()
        .fetch_from(&world.network)
        .await?
        .data;
    let metadata = world.view(&coin, "ft_metadata", json!({})).await?;
    let pool = world.view(&coin, "fc_pool", json!({})).await?;
    let storage = u128::from(account.storage_usage) * YOCTO_PER_STORAGE_BYTE;
    Ok(Some(Outcome {
        icon: metadata["icon"].as_str().map(str::to_string),
        balance: account.amount.as_yoctonear(),
        storage_bytes: account.storage_usage,
        owed: amount(&pool["real_near"])? + amount(&pool["creator_fee_balance"])? + storage,
    }))
}

fn print(deposit: &str, outcome: &Option<Outcome>) {
    match outcome {
        None => println!("{deposit}: no coin; the launch was refused and the deposit refunded"),
        Some(o) => println!(
            "{deposit}: coin exists; storage {} bytes, locked storage {}; balance {}; real NEAR + creator balance + storage {}; margin {}",
            o.storage_bytes,
            near(u128::from(o.storage_bytes) * YOCTO_PER_STORAGE_BYTE),
            near(o.balance),
            near(o.owed),
            near(o.balance.saturating_sub(o.owed)),
        ),
    }
}

#[tokio::test]
async fn a_launch_with_the_largest_icon_and_no_opening_buy_goes_through_with_0_25_near(
) -> Result<()> {
    let world = World::launch().await?;

    let at_25 = launch(&world, "dep250", NearToken::from_millinear(250)).await?;
    print("0.25 NEAR", &at_25);
    let at_20 = launch(&world, "dep200", NearToken::from_millinear(200)).await?;
    print("0.20 NEAR", &at_20);
    if at_20.is_none() {
        // Each try has its own 6-character ticker, so every coin uses the same storage.
        let (mut too_little, mut enough) = (200u128, 250u128);
        while enough - too_little > 1 {
            let middle = too_little + (enough - too_little) / 2;
            let tried = launch(
                &world,
                &format!("d{middle:05}"),
                NearToken::from_millinear(middle),
            )
            .await?;
            if tried.is_some() {
                enough = middle;
            } else {
                too_little = middle;
            }
        }
        println!(
            "smallest deposit that works: 0.{enough:03} NEAR (0.{too_little:03} NEAR is refused)"
        );
    }

    let coin = at_25.context("0.25 NEAR: no coin after the launch")?;
    assert_eq!(
        coin.icon.as_deref(),
        Some(largest_icon().as_str()),
        "ft_metadata does not return the 8191-byte icon"
    );
    assert!(
        coin.balance >= coin.owed,
        "0.25 NEAR: the coin holds {} but owes {}",
        near(coin.balance),
        near(coin.owed)
    );
    println!("PASS 0.25 NEAR launch with the 8191-byte icon and no opening buy");
    Ok(())
}
