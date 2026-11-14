//! The launch deposit with the largest icon the coin takes (a WebP data URL of 8191 bytes) and no
//! opening buy, in two cases:
//!
//! - The launch form's longest text: a 32-character name, an 8-character ticker and an
//!   80-character description, with 4-byte characters in the name and description, so the most
//!   bytes the form can send. With 0.25 NEAR the coin must exist, return that icon in
//!   `ft_metadata` and hold enough for its real NEAR, its creator balance and its storage.
//! - A stress case past the form: the coin and the factory set no length limit on the name or
//!   the description, and the ticker is bounded only by the 64-character account ID. This case
//!   uses the longest ticker the account ID allows and a 3500-byte name and description (the
//!   factory reads `fc_rules`, which holds the description, up to 4096 bytes). It prints what
//!   happens.
//!
//! Each case tries 0.25 NEAR, then 0.20 NEAR; when a deposit is refused, a search finds the
//! smallest deposit that works, to 1 milliNEAR. The numbers print as they pass (`--nocapture`).

use anyhow::{Context, Result};
use fair_chance_sandbox_tests::{events, launch_args, World};
use near_api::types::{AccountId, NearToken};
use serde_json::{json, Value};

/// NEAR protocol storage price: 10^19 yoctoNEAR per byte.
const YOCTO_PER_STORAGE_BYTE: u128 = 10_000_000_000_000_000_000;
const ICON_PREFIX: &str = "data:image/webp;base64,";
/// The largest icon the coin takes: under 8 KB, so 8 * 1024 - 1 bytes.
const LARGEST_ICON_BYTES: usize = 8191;
/// The launch form's limits, in characters.
const FORM_NAME_CHARS: usize = 32;
const FORM_TICKER_CHARS: usize = 8;
const FORM_DESCRIPTION_CHARS: usize = 80;
/// A 4-byte UTF-8 character.
const WIDE: &str = "\u{1F980}";
/// The stress case's name and description, in bytes.
const STRESS_TEXT_BYTES: usize = 3500;
/// NEAR account IDs are at most 64 characters.
const ACCOUNT_ID_MAX: usize = 64;

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

/// One launch's text: name, description and a ticker of a fixed length for each try.
struct Text {
    label: &'static str,
    name: String,
    description: String,
    ticker_chars: usize,
}

/// A ticker of `chars` characters that starts with `tag`, so each try has its own coin with the
/// same storage.
fn ticker(tag: &str, chars: usize) -> String {
    format!("{tag}{}", "x".repeat(chars - tag.len()))
}

fn amount(value: &Value) -> Result<u128> {
    Ok(value.as_str().context("amount string")?.parse()?)
}

fn near(yocto: u128) -> String {
    format!("{:.6} NEAR", yocto as f64 / 1e24)
}

/// What one launch left, when a coin exists after it.
struct Outcome {
    icon: Option<String>,
    balance: u128,
    storage_bytes: u64,
    owed: u128,
    fc_launch_logged: bool,
}

async fn launch(world: &World, text: &Text, tag: &str, millinear: u128) -> Result<Option<Outcome>> {
    let ticker = ticker(tag, text.ticker_chars);
    let icon = largest_icon();
    let result = world
        .try_launch(
            &world.creator,
            launch_args(
                &text.name,
                &ticker,
                Some(&icon),
                Some(&text.description),
                None,
            ),
            NearToken::from_millinear(millinear),
        )
        .await;
    // A failed factory callback (for example an fc_rules answer over its read limit) can make
    // the call an error while the coin exists; the coin account decides below.
    if let Err(error) = &result {
        println!(
            "{}: the launch call at {millinear} milliNEAR gave an error: {error:#}",
            text.label
        );
    }
    let coin: AccountId = world.coin_for(&ticker)?;
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
        fc_launch_logged: result
            .as_ref()
            .is_ok_and(|r| !events(r, "fair_chance_factory", "fc_launch").is_empty()),
    }))
}

fn print(label: &str, deposit: &str, outcome: &Option<Outcome>) {
    match outcome {
        None => println!("{label}, {deposit}: refused; no coin, the deposit is refunded"),
        Some(o) => println!(
            "{label}, {deposit}: coin exists; storage {} bytes, locked storage {}; balance {}; real NEAR + creator balance + storage {}; margin {}; fc_launch logged {}",
            o.storage_bytes,
            near(u128::from(o.storage_bytes) * YOCTO_PER_STORAGE_BYTE),
            near(o.balance),
            near(o.owed),
            near(o.balance.saturating_sub(o.owed)),
            o.fc_launch_logged,
        ),
    }
}

/// Launches at 0.25 and 0.20 NEAR and, when one is refused, searches the smallest deposit that
/// works. Returns the 0.25 NEAR outcome.
async fn run_case(world: &World, text: &Text, tag: char) -> Result<Option<Outcome>> {
    println!(
        "{}: name {} bytes, ticker {} characters, description {} bytes, icon {} bytes",
        text.label,
        text.name.len(),
        text.ticker_chars,
        text.description.len(),
        LARGEST_ICON_BYTES
    );
    let at_25 = launch(world, text, &format!("{tag}a"), 250).await?;
    print(text.label, "0.25 NEAR", &at_25);
    let at_20 = launch(world, text, &format!("{tag}b"), 200).await?;
    print(text.label, "0.20 NEAR", &at_20);
    let (mut too_little, mut enough) = match (&at_20, &at_25) {
        (Some(_), _) => return Ok(at_25),
        (None, Some(_)) => (200u128, 250u128),
        (None, None) => (250u128, 1000u128),
    };
    if launch(world, text, &format!("{tag}c"), enough)
        .await?
        .is_none()
    {
        println!(
            "{}: refused at {} too; no search",
            text.label,
            near(enough * 10u128.pow(21))
        );
        return Ok(at_25);
    }
    let mut tries = 0;
    while enough - too_little > 1 {
        tries += 1;
        let middle = too_little + (enough - too_little) / 2;
        if launch(world, text, &format!("{tag}{tries}"), middle)
            .await?
            .is_some()
        {
            enough = middle;
        } else {
            too_little = middle;
        }
    }
    println!(
        "{}: smallest deposit that works {} ({} is refused)",
        text.label,
        near(enough * 10u128.pow(21)),
        near(too_little * 10u128.pow(21))
    );
    Ok(at_25)
}

#[tokio::test]
async fn a_launch_with_the_largest_icon_and_the_longest_form_text_goes_through_with_0_25_near(
) -> Result<()> {
    let world = World::launch().await?;

    let form = Text {
        label: "form limits",
        name: WIDE.repeat(FORM_NAME_CHARS),
        description: WIDE.repeat(FORM_DESCRIPTION_CHARS),
        ticker_chars: FORM_TICKER_CHARS,
    };
    let coin = run_case(&world, &form, 'f')
        .await?
        .context("form limits, 0.25 NEAR: no coin after the launch")?;

    let stress = Text {
        label: "stress past the form",
        name: "n".repeat(STRESS_TEXT_BYTES),
        description: "d".repeat(STRESS_TEXT_BYTES),
        ticker_chars: ACCOUNT_ID_MAX - 1 - world.factory.id.as_str().len(),
    };
    let _ = run_case(&world, &stress, 's').await?;

    assert_eq!(
        coin.icon.as_deref(),
        Some(largest_icon().as_str()),
        "ft_metadata does not return the 8191-byte icon"
    );
    assert!(
        coin.balance >= coin.owed,
        "form limits, 0.25 NEAR: the coin holds {} but owes {}",
        near(coin.balance),
        near(coin.owed)
    );
    assert!(
        coin.fc_launch_logged,
        "form limits, 0.25 NEAR: no fc_launch"
    );
    println!(
        "PASS 0.25 NEAR launch with the 8191-byte icon, the longest form text and no opening buy"
    );
    Ok(())
}
