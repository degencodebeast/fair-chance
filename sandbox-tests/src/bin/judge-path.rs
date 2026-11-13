//! The judge path on a fresh sandbox, in one command. The factory refuses an opening buy above
//! the 4% creator-group cap, then launches the demo coin with a smaller opening buy. The creator's
//! sell in the early window is refused. After the window, one account buys and becomes large, its
//! send is refused, the creator's sell fills in part at its share of the early-supply allowance,
//! and the large account's sell fills the real NEAR cap. The coin page shows the partial fill in
//! headless Chrome, and the Go watcher sends the alerts to its fake sender.
//!
//! The run ends with one `PASS <check>: <detail>` line per check and exits 0. The first failed
//! check prints `FAIL <check>: <reason>` and exits 1. `FC_EVIDENCE_DIR`, when set, receives the
//! page screenshot.

use std::path::Path;

use anyhow::{bail, ensure, Context, Result};
use fair_chance_sandbox_tests::page_and_watcher_reads::{run_browser_read, run_watcher};
use fair_chance_sandbox_tests::pass_list::PassList;
use fair_chance_sandbox_tests::refusal_texts::refused_with;
use fair_chance_sandbox_tests::{
    events, launch_args, transact, Account, LoggedEvent, World, COIN_ICON, LAUNCH_DEPOSIT,
};
use near_api::types::transaction::result::TransactionResult;
use near_api::types::{AccountId, NearToken};
use serde_json::{json, Value};

const TICKER: &str = "bag";
const NAME: &str = "Bag Holder";
const DESCRIPTION: &str = "You buy. The creator dumps on you. You hold the bag.";
/// At block 0 the buy fee is 50%, so 10 NEAR would buy about 4.8% of supply: above the cap.
const OPENING_BUY_ABOVE_CAP: NearToken = NearToken::from_near(10);
/// 8 NEAR at block 0 buys about 3.85% of supply: inside the 4% cap.
const OPENING_BUY: NearToken = NearToken::from_near(8);
/// Enough to pass 2% of supply, and enough real NEAR in the pool that the creator's sell is held
/// by its allowance before the cap.
const LARGE_BUY: NearToken = NearToken::from_near(30);
const TOKEN_UNITS_PER_WHOLE_TOKEN: u128 = 1_000_000_000_000_000_000;
const SUPPLY_WHOLE_TOKENS: u128 = 1_000_000_000;

fn step(text: &str) {
    println!("judge-path: {text}");
}

#[tokio::main]
async fn main() {
    let mut list = PassList::default();
    match walk(&mut list).await {
        Ok(()) => {
            for line in list.lines() {
                println!("{line}");
            }
        }
        Err(failure) => {
            for line in list.lines() {
                println!("{line}");
            }
            let text = format!("{failure:#}");
            if text.starts_with("FAIL ") {
                println!("{text}");
            } else {
                println!("FAIL setup: {text}");
            }
            std::process::exit(1);
        }
    }
}

async fn walk(list: &mut PassList) -> Result<()> {
    step("start a fresh sandbox, deploy the coin code by hash and the factory");
    let world = World::launch().await?;
    let coin: AccountId = world.coin_for(TICKER)?;
    let creator = &world.creator;

    step("the creator asks for an opening buy above the 4% creator-group cap");
    let refused = world
        .try_launch(
            creator,
            bag_launch_args(OPENING_BUY_ABOVE_CAP),
            LAUNCH_DEPOSIT.saturating_add(OPENING_BUY_ABOVE_CAP),
        )
        .await?;
    list.check(
        "opening buy above 4% refused",
        async {
            ensure!(
                events(&refused, "fair_chance_factory", "fc_launch").is_empty(),
                "the factory logged fc_launch"
            );
            ensure!(
                world.contract_state(&coin).await?.is_none(),
                "the coin account {coin} exists"
            );
            let text = refused_with(&format!("{refused:?}"), "FC_CREATOR_CAP")?;
            Ok(format!(
                "{} NEAR at block 0, no coin made: {text}",
                whole_near(OPENING_BUY_ABOVE_CAP)
            ))
        }
        .await,
    )?;

    step("the creator launches the coin with a smaller opening buy");
    let launched = world
        .try_launch(
            creator,
            bag_launch_args(OPENING_BUY),
            LAUNCH_DEPOSIT.saturating_add(OPENING_BUY),
        )
        .await?;
    // The early-window sell goes out at once, before any view, so it lands in blocks 0 to N.
    let opening_tokens = only_event(&launched, "fc_buy")
        .ok()
        .map(|b| b.event["data"][0]["tokens_out"].clone());
    let early_sell = match &opening_tokens {
        Some(Value::String(amount)) => Some(sell(&world, creator, &coin, amount).await?),
        _ => None,
    };

    let rules = world.view(&coin, "fc_rules", json!({})).await;
    list.check(
        "coin launched",
        async {
            ensure!(launched.is_success(), "launch failed: {launched:?}");
            let launch = events(&launched, "fair_chance_factory", "fc_launch");
            ensure!(
                launch.len() == 1,
                "expected one fc_launch, got {}",
                launch.len()
            );
            let keys = world.access_key_count(&coin).await?;
            ensure!(keys == 0, "the coin has {keys} access keys");
            let rules = rules.as_ref().map_err(|e| anyhow::anyhow!("{e:#}"))?;
            ensure!(
                rules["code_hash"] == world.coin_code_hash.to_string(),
                "coin code hash {}",
                rules["code_hash"]
            );
            Ok(format!(
                "{coin} by the factory in one batch, zero access keys, code hash {}",
                world.coin_code_hash
            ))
        }
        .await,
    )?;
    let rules = rules?;
    let launch_block = rules["launch_block"]
        .as_u64()
        .context("fc_rules.launch_block")?;
    let early_blocks = rules["early_blocks"]
        .as_u64()
        .context("fc_rules.early_blocks")?;

    list.check(
        "opening buy marked",
        async {
            let buy = only_event(&launched, "fc_buy")?;
            let data = &buy.event["data"][0];
            ensure!(
                data["account"] == creator.id.as_str(),
                "the opening buy is by {}",
                data["account"]
            );
            ensure!(
                data["block"] == launch_block,
                "the opening buy is in block {}, launch block {launch_block}",
                data["block"]
            );
            ensure!(
                data["marked_tokens"] == data["tokens_out"],
                "marked {} of {}",
                data["marked_tokens"],
                data["tokens_out"]
            );
            let account = world
                .view(&coin, "fc_account", json!({ "account_id": creator.id }))
                .await?;
            ensure!(
                account["in_creator_group"] == true,
                "the creator is not in the creator group"
            );
            let tokens = amount(&data["tokens_out"])?;
            ensure!(
                tokens * 10_000 <= 400 * SUPPLY_WHOLE_TOKENS * TOKEN_UNITS_PER_WHOLE_TOKEN,
                "above 4% of supply"
            );
            Ok(format!(
                "{} NEAR at block 0 bought {} tokens ({} of supply), all marked as early supply",
                whole_near(OPENING_BUY),
                whole_tokens(tokens),
                share(tokens)
            ))
        }
        .await,
    )?;

    list.check(
        "early-window sell refused",
        async {
            let sold = early_sell.context("no opening buy to sell")?;
            ensure!(
                !sold.is_success(),
                "the creator sold in the early window: {sold:?}"
            );
            let block = failed_receipt_block(&world, &sold).await?;
            let b = block
                .checked_sub(launch_block)
                .context("refused before the launch block")?;
            ensure!(
                b <= early_blocks,
                "the sell landed in block {b}, after the early window"
            );
            let text = refused_with(&format!("{sold:?}"), "FC_EARLY_WINDOW")?;
            Ok(format!(
                "the creator's sell in block {b} of 0 to {early_blocks}: {text}"
            ))
        }
        .await,
    )?;

    step("wait for the end of the early window");
    world.wait_until_after_the_early_window(&coin).await?;

    step("one account buys and passes 2% of supply");
    let buyer = &world.alice;
    let _ = world.register(buyer, &coin).await?;
    let _ = world.register(&world.bob, &coin).await?;
    let bought = transact(
        &world.network,
        buyer,
        &coin,
        "buy",
        json!({ "min_tokens_out": "0" }),
        LARGE_BUY,
    )
    .await?;
    list.check(
        "large buy",
        async {
            ensure!(bought.is_success(), "the buy failed: {bought:?}");
            let large = only_event(&bought, "fc_became_large")?;
            ensure!(large.event["data"][0]["account"] == buyer.id.as_str());
            let balance = amount(&large.event["data"][0]["balance"])?;
            Ok(format!(
                "{} bought {} tokens ({} of supply) for {} NEAR and is large (fc_became_large)",
                buyer.id,
                whole_tokens(balance),
                share(balance),
                whole_near(LARGE_BUY)
            ))
        }
        .await,
    )?;

    step("the large account tries to send tokens");
    let sent = transact(
        &world.network,
        buyer,
        &coin,
        "ft_transfer",
        json!({ "receiver_id": world.bob.id, "amount": TOKEN_UNITS_PER_WHOLE_TOKEN.to_string() }),
        NearToken::from_yoctonear(1),
    )
    .await?;
    list.check(
        "large send refused",
        async {
            ensure!(
                !sent.is_success(),
                "the large account sent tokens: {sent:?}"
            );
            let text = refused_with(&format!("{sent:?}"), "FC_LARGE_CANNOT_SEND")?;
            Ok(format!(
                "{} sends 1 token to {}: {text}",
                buyer.id, world.bob.id
            ))
        }
        .await,
    )?;

    step("the creator sells every token after the early window");
    let creator_balance = balance(&world, &coin, creator).await?;
    let creator_sell = sell(&world, creator, &coin, &creator_balance.to_string()).await?;
    let limited = only_event(&creator_sell, "fc_sell");
    list.check("limited sell filled in part", async {
        let receipt = limited.as_ref().map_err(|e| anyhow::anyhow!("{e:#}"))?;
        let data = &receipt.event["data"][0];
        ensure!(data["fill"] == "partial", "fill is {}", data["fill"]);
        ensure!(data["limit_reason"] == "allowance", "limit_reason is {}", data["limit_reason"]);
        let asked = amount(&data["tokens_asked"])?;
        let sold = amount(&data["tokens_sold"])?;
        let kept = balance(&world, &coin, creator).await?;
        ensure!(sold > 0 && sold < asked && kept == asked - sold, "asked {asked}, sold {sold}, wallet holds {kept}");
        Ok(format!(
            "the creator asked {} tokens, the early-supply allowance filled {} for {} NEAR, {} stay in the wallet (limit_reason allowance)",
            whole_tokens(asked), whole_tokens(sold), near(&data["near_out"])?, whole_tokens(kept)
        ))
    }.await)?;
    let limited = limited?;

    step("the large account sells every token");
    let buyer_balance = balance(&world, &coin, buyer).await?;
    let large_sell = sell(&world, buyer, &coin, &buyer_balance.to_string()).await?;
    list.check("cap full", async {
        let filled = only_event(&large_sell, "fc_cap_filled")?;
        let cap = &filled.event["data"][0];
        ensure!(cap["by_account"] == buyer.id.as_str(), "the cap was filled by {}", cap["by_account"]);
        let data = &only_event(&large_sell, "fc_sell")?.event["data"][0];
        ensure!(data["limit_reason"] == "cap_full", "limit_reason is {}", data["limit_reason"]);
        ensure!(data["fill"] != "full", "the sell filled in full");
        Ok(format!(
            "{} asked {} tokens and got {} NEAR; fc_cap_filled at {} of {} NEAR real NEAR cap (limit_reason cap_full)",
            buyer.id, whole_tokens(amount(&data["tokens_asked"])?), near(&data["near_out"])?, near(&cap["cap_used"])?, near(&cap["cap_total"])?
        ))
    }.await)?;

    step("headless Chrome opens the coin page and shows the creator's partial fill");
    world.wait_final(&large_sell).await?;
    let screenshot = std::env::var("FC_EVIDENCE_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("judge-path-receipt.png"));
    let shown = run_browser_read(
        world.rpc_url(),
        coin.as_str(),
        world.factory.id.as_str(),
        &world.coin_code_hash.to_string(),
        &limited.receipt_id,
        screenshot.as_deref(),
    )
    .await;
    list.check(
        "receipt on the coin page",
        async {
            let shown = shown?;
            ensure!(
                shown["receipt_id"] == limited.receipt_id.as_str(),
                "the page shows {shown}"
            );
            let text = shown["text"].as_str().context("row text")?;
            ensure!(
                text.contains(creator.id.as_str()),
                "the row does not name the creator: {text}"
            );
            Ok(format!(
                "Chrome {} shows \"{text}\"",
                shown["browser"].as_str().unwrap_or("?")
            ))
        }
        .await,
    )?;

    step("the watcher reads the coin's events and sends alerts to its fake sender");
    // The watcher scans blocks from the launch block. The launch transaction sits in an earlier
    // chunk, so the scan does not read the opening buy's events (open question); count the
    // alerts of the transactions after the launch.
    let all = [&bought, &creator_sell, &large_sell];
    let want = ["fc_sell", "fc_cap_filled", "fc_became_large"]
        .iter()
        .map(|name| {
            all.iter()
                .map(|r| events(r, "fair_chance", name).len())
                .sum::<usize>()
        })
        .sum();
    let alerts = run_watcher(world.rpc_url(), coin.as_str(), want).await;
    list.check(
        "alert",
        async {
            let alerts = alerts?;
            let texts: Vec<&str> = alerts["alerts"]
                .as_array()
                .context("alerts list")?
                .iter()
                .filter_map(|a| a["text"].as_str())
                .collect();
            ensure!(
                texts.len() == want,
                "{} alerts, want {want}: {texts:?}",
                texts.len()
            );
            let partial = texts
                .iter()
                .find(|t| t.starts_with(&format!("{} tried to sell", creator.id)))
                .with_context(|| format!("no alert for the creator's partial fill: {texts:?}"))?;
            ensure!(
                texts.iter().any(|t| t.starts_with("Pool cap full")),
                "no cap-full alert: {texts:?}"
            );
            Ok(format!(
                "{want} alerts to the fake sender, one reads \"{partial}\""
            ))
        }
        .await,
    )?;
    Ok(())
}

fn bag_launch_args(opening_buy: NearToken) -> Value {
    launch_args(
        NAME,
        TICKER,
        Some(COIN_ICON),
        Some(DESCRIPTION),
        Some(opening_buy),
    )
}

async fn sell(
    world: &World,
    who: &Account,
    coin: &AccountId,
    amount: &str,
) -> Result<TransactionResult> {
    transact(
        &world.network,
        who,
        coin,
        "sell",
        json!({ "amount": amount, "min_near_out": "0" }),
        NearToken::from_yoctonear(1),
    )
    .await
}

async fn balance(world: &World, coin: &AccountId, who: &Account) -> Result<u128> {
    amount(
        &world
            .view(coin, "ft_balance_of", json!({ "account_id": who.id }))
            .await?,
    )
}

/// The one `fair_chance` event with this name in the transaction's receipts.
fn only_event(result: &TransactionResult, name: &str) -> Result<LoggedEvent> {
    let mut found = events(result, "fair_chance", name);
    if found.len() != 1 {
        bail!("expected one {name}, got {}: {result:?}", found.len());
    }
    Ok(found.remove(0))
}

/// The height of the block that holds the transaction's failed receipt.
async fn failed_receipt_block(world: &World, result: &TransactionResult) -> Result<u64> {
    let TransactionResult::Full(full) = result else {
        bail!("the transaction has no execution result")
    };
    let failed = full
        .receipt_outcomes()
        .iter()
        .find(|o| !o.is_success())
        .context("no failed receipt")?;
    world.block_height_of(failed.block_hash).await
}

fn amount(value: &Value) -> Result<u128> {
    Ok(value
        .as_str()
        .with_context(|| format!("{value} is not a decimal string"))?
        .parse()?)
}

/// yoctoNEAR as NEAR with 4 decimals, cut and not rounded.
fn near(value: &Value) -> Result<String> {
    let yocto = amount(value)?;
    let whole = yocto / 10u128.pow(24);
    let part = yocto % 10u128.pow(24) / 10u128.pow(20);
    Ok(format!("{whole}.{part:04}"))
}

fn whole_near(n: NearToken) -> u128 {
    n.as_yoctonear() / 10u128.pow(24)
}

/// Tokens in millions with one decimal, cut: 38.4M.
fn whole_tokens(units: u128) -> String {
    let tenths = units / (TOKEN_UNITS_PER_WHOLE_TOKEN * 100_000);
    format!("{}.{}M", tenths / 10, tenths % 10)
}

/// Share of the fixed supply in percent with two decimals, cut: 3.84%.
fn share(units: u128) -> String {
    let bps = units * 10_000 / (SUPPLY_WHOLE_TOKENS * TOKEN_UNITS_PER_WHOLE_TOKEN);
    format!("{}.{:02}%", bps / 100, bps % 100)
}
