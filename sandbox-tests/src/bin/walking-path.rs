//! The walking path on a fresh sandbox: the factory launches one coin, one account buys and
//! sells, and the sell logs `fc_sell`. The page's chain-read module waits for the sell to be final
//! and finds the receipt by a block scan; then the Go watcher reads it through its event adapter
//! and sends one alert to its fake sender. Each step waits for the chain itself, so neither depends
//! on the other. Exits 0 only when every step passes.
//!
//! `walking-path --hold` stops after the sell, prints the RPC URL and coin, and keeps the sandbox up
//! until it is killed.
//!
//! `walking-path --read-in-browser` also opens the built receipts page in headless Google Chrome
//! (`web/scripts/read-receipt-in-browser.ts`) and checks that it shows the same receipt row. It needs
//! `npm ci` in `web/`. `FC_EVIDENCE_DIR`, when set, receives the page screenshot.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use fair_chance_sandbox_tests::{events, field_names, World, FC_SELL_FIELDS};
use near_api::types::NearToken;
use serde_json::{json, Value};
use tokio::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

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
        "walking-path:   fc_sell in receipt {} with the rc.4 field names: tokens_sold {} near_out {}",
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
    let page = run_page_read(&world).await?;
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
        let shown = run_browser_read(&world, &fc_sell.receipt_id).await?;
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
    }

    step("the watcher reads the event through its adapter and sends one alert to the fake sender");
    let alerts = run_watcher(&world).await?;
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

async fn run_watcher(world: &World) -> Result<Value> {
    let bin = std::env::var("FC_WATCHER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root().join("target/watcher"));
    let out = Command::new(&bin)
        .args(["-once", "-want", "1", "-timeout", "60s"])
        .env("FC_RPC_URL", world.rpc_url())
        .env("FC_COINS", world.coin.as_str())
        .env("FC_SENDER", "fake")
        .stderr(Stdio::inherit())
        .output();
    let out = tokio::time::timeout(Duration::from_secs(90), out)
        .await
        .context("watcher timed out")?
        .with_context(|| format!("run {}", bin.display()))?;
    if !out.status.success() {
        bail!(
            "watcher exited with {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stdout)
        );
    }
    serde_json::from_slice(&out.stdout).context("watcher output is not JSON")
}

async fn run_browser_read(world: &World, receipt_id: &str) -> Result<Value> {
    let query = format!("?coin={}&rpc={}", world.coin, urlencode(world.rpc_url()));
    let mut command = Command::new("node");
    command
        .arg(repo_root().join("web/scripts/read-receipt-in-browser.ts"))
        .arg(&query)
        .arg(receipt_id);
    if let Ok(dir) = std::env::var("FC_EVIDENCE_DIR") {
        command.arg(Path::new(&dir).join("receipts-page.png"));
    }
    let out = command.stderr(Stdio::inherit()).output();
    let out = tokio::time::timeout(Duration::from_secs(180), out)
        .await
        .context("browser read timed out")?
        .context("run node")?;
    if !out.status.success() {
        bail!(
            "browser read exited with {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stdout)
        );
    }
    serde_json::from_slice(&out.stdout).context("browser read output is not JSON")
}

async fn run_page_read(world: &World) -> Result<Value> {
    let page_url = format!(
        "http://localhost/?coin={}&rpc={}",
        world.coin,
        urlencode(world.rpc_url())
    );
    let out = Command::new("node")
        .arg(repo_root().join("web/scripts/read-receipts.ts"))
        .arg(&page_url)
        .stderr(Stdio::inherit())
        .output();
    let out = tokio::time::timeout(Duration::from_secs(90), out)
        .await
        .context("page read timed out")?
        .context("run node")?;
    if !out.status.success() {
        bail!(
            "page read exited with {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stdout)
        );
    }
    serde_json::from_slice(&out.stdout).context("page read output is not JSON")
}

fn urlencode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
