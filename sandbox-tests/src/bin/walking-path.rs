//! The walking path on a fresh sandbox: the factory launches one coin, one account buys and
//! sells, and the sell logs `fc_sell`. The page's chain-read module waits for the sell to be final
//! and finds the receipt by a block scan; then the Go watcher reads it through its event adapter
//! and sends one alert to its fake sender. Each step waits for the chain itself, so neither depends
//! on the other. Exits 0 only when every step passes.
//!
//! `walking-path --hold` stops after the sell, prints the RPC URL and coin, and keeps the sandbox up
//! until it is killed.
//!
//! `walking-path --read-in-browser` also opens the built coin page in headless Google Chrome
//! (`web/scripts/read-receipt-in-browser.ts`) and checks that it shows the same receipt row. It needs
//! `npm ci` in `web/`. `FC_EVIDENCE_DIR`, when set, receives the page screenshot.

use std::path::Path;

use anyhow::{ensure, Context, Result};
use fair_chance_sandbox_tests::page_and_watcher_reads::{
    run_browser_read, run_page_read, run_watcher,
};
use fair_chance_sandbox_tests::{events, field_names, World, FC_SELL_FIELDS};
use near_api::types::NearToken;
use serde_json::json;

fn step(name: &str) {
    println!("walking-path: {name}");
}

#[tokio::main]
async fn main() -> Result<()> {
    let hold = std::env::args().any(|a| a == "--hold");
    let read_in_browser = std::env::args().any(|a| a == "--read-in-browser");

    step("start a fresh sandbox, deploy the coin code by hash and the factory, launch one coin");
    let world = World::launch().await?;
    let launch = events(&world.launch, "fair_chance_factory", "fc_launch");
    ensure!(
        launch.len() == 1,
        "expected one fc_launch, got {}",
        launch.len()
    );
    let rules = world.view(&world.coin, "fc_rules", json!({})).await?;
    let source = world
        .view(&world.coin, "contract_source_metadata", json!({}))
        .await?;
    ensure!(
        source["version"] == rule_engine::INTERFACE_VERSION,
        "coin reports version {}",
        source["version"]
    );
    println!(
        "walking-path:   coin {} launch_block {} version {}",
        world.coin, rules["launch_block"], source["version"]
    );

    step("one account buys");
    let _ = world.register(&world.alice, &world.coin).await?;
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
    let balance: u128 = balance.as_str().context("balance")?.parse()?;
    let amount = (balance / 2).to_string();

    step("the same account sells half");
    let sell = world
        .call(
            &world.alice,
            &world.coin,
            "sell",
            json!({ "amount": amount, "min_near_out": "0" }),
            NearToken::from_yoctonear(1),
        )
        .await?;
    let sells = events(&sell, "fair_chance", "fc_sell");
    ensure!(
        sells.len() == 1,
        "expected one fc_sell in the sell receipts, got {}",
        sells.len()
    );
    let fc_sell = &sells[0];
    let mut expected: Vec<String> = FC_SELL_FIELDS.iter().map(|s| s.to_string()).collect();
    expected.sort();
    ensure!(
        field_names(&fc_sell.event) == expected,
        "fc_sell fields {:?}",
        field_names(&fc_sell.event)
    );
    ensure!(fc_sell.event["version"] == rule_engine::INTERFACE_VERSION);
    let sold = &fc_sell.event["data"][0];
    println!(
        "walking-path:   fc_sell in receipt {} with the interface field names: tokens_sold {} near_out {}",
        fc_sell.receipt_id, sold["tokens_sold"], sold["near_out"]
    );

    if hold {
        println!("FC_RPC_URL={}", world.rpc_url());
        println!("FC_COIN={}", world.coin);
        println!("walking-path: holding the sandbox; stop with Ctrl-C");
        tokio::signal::ctrl_c().await?;
        return Ok(());
    }

    step("the page finds the receipt by a block scan from the launch block");
    let final_block = world.wait_final(&sell).await?;
    println!("walking-path:   the sell is final at block {final_block}");
    let page = run_page_read(world.rpc_url(), world.coin.as_str()).await?;
    let receipts = page["receipts"].as_array().context("receipts list")?;
    let found = receipts
        .iter()
        .find(|r| r["receipt_id"] == fc_sell.receipt_id.as_str())
        .with_context(|| {
            format!(
                "the page did not find receipt {}: {page}",
                fc_sell.receipt_id
            )
        })?;
    ensure!(
        found["tokens_sold"] == sold["tokens_sold"],
        "page row {found} does not match {sold}"
    );
    ensure!(
        page["source"] == "block-scan",
        "page source is {}",
        page["source"]
    );
    println!("walking-path:   page row: {}", found["text"]);

    if read_in_browser {
        step("headless Chrome opens the receipts page and shows the same receipt");
        let screenshot = std::env::var("FC_EVIDENCE_DIR")
            .ok()
            .map(|dir| Path::new(&dir).join("receipts-page.png"));
        let shown = run_browser_read(
            world.rpc_url(),
            world.coin.as_str(),
            world.factory.id.as_str(),
            &world.coin_code_hash.to_string(),
            &fc_sell.receipt_id,
            screenshot.as_deref(),
        )
        .await?;
        let want_text = format!(
            "Block {}: {}",
            found["block"],
            found["text"].as_str().context("row text")?
        );
        ensure!(
            shown["receipt_id"] == fc_sell.receipt_id.as_str()
                && shown["text"] == want_text.as_str(),
            "the browser shows {shown}, want receipt {} with text {want_text:?}",
            fc_sell.receipt_id
        );
        println!(
            "walking-path:   browser ({}) row: {}",
            shown["browser"], shown["text"]
        );
        println!(
            "walking-path:   coin page stats {} code hash {} account checks {}",
            shown["stats"], shown["code_hash"], shown["checks"]
        );
    }

    step("the watcher reads the event through its adapter and sends one alert to the fake sender");
    let alerts = run_watcher(world.rpc_url(), world.coin.as_str(), 1).await?;
    let alerts = alerts["alerts"].as_array().context("alerts list")?;
    ensure!(alerts.len() == 1, "expected one alert, got {alerts:?}");
    let text = alerts[0]["text"].as_str().context("alert text")?;
    ensure!(
        text.contains(world.alice.id.as_str()),
        "alert does not name the seller: {text}"
    );
    println!("walking-path:   alert: {text}");

    step("ok");
    Ok(())
}
