//! Runs the two readers outside Rust against a sandbox coin: the page's chain-read module (Node),
//! the coin page in headless Google Chrome, and the Go watcher with its fake sender. Each returns
//! the JSON its tool prints.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use tokio::process::Command;

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The watcher (`target/watcher`, or `FC_WATCHER_BIN`) reads the coin's events from the launch
/// block and stops after `want` alerts to its fake sender: `{"alerts": [{"text": ..}, ..]}`.
pub async fn run_watcher(rpc_url: &str, coin: &str, want: usize) -> Result<Value> {
    let bin = std::env::var("FC_WATCHER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root().join("target/watcher"));
    let out = Command::new(&bin)
        .args(["-once", "-want", &want.to_string(), "-timeout", "60s"])
        .env("FC_RPC_URL", rpc_url)
        .env("FC_COINS", coin)
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

/// The page's receipts for the coin, found by a block scan from the launch block
/// (`web/scripts/read-receipts.ts`).
pub async fn run_page_read(rpc_url: &str, coin: &str) -> Result<Value> {
    let page_url = format!("http://localhost/?coin={coin}&rpc={}", urlencode(rpc_url));
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

/// The coin page as headless Chrome shows it, read for one receipt
/// (`web/scripts/read-receipt-in-browser.ts`). The link also names the known factory and coin
/// code hash, so the page can check the coin account against them. With `screenshot`, the page
/// picture goes there.
pub async fn run_browser_read(
    rpc_url: &str,
    coin: &str,
    factory: &str,
    coin_code_hash: &str,
    receipt_id: &str,
    screenshot: Option<&Path>,
) -> Result<Value> {
    let query = format!(
        "?coin={coin}&rpc={}&factory={factory}&code_hash={coin_code_hash}",
        urlencode(rpc_url)
    );
    let mut command = Command::new("node");
    command
        .arg(repo_root().join("web/scripts/read-receipt-in-browser.ts"))
        .arg(&query)
        .arg(receipt_id);
    if let Some(path) = screenshot {
        command.arg(path);
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
