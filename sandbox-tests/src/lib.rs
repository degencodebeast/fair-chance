//! A fresh near-sandbox with the factory deployed and one coin launched.
//!
//! Each [`World`] starts its own sandbox: the near-sandbox crate picks a free RPC and network
//! port and makes a new temporary data folder, so runs in parallel do not share either.
//! The binary comes from `NEAR_SANDBOX_BIN_PATH`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use near_api::types::transaction::result::TransactionResult;
use near_api::types::{AccountId, CryptoHash, Data, NearToken};
use near_api::{Contract, NetworkConfig, Signer};
use near_sandbox::{GenesisAccount, Sandbox, SandboxConfig};
use serde_json::{json, Value};

pub const COIN_TICKER: &str = "walk";
/// The walking-path coin's icon: a 96 x 96 WebP data URL under 8 KB.
pub const COIN_ICON: &str = include_str!("../../testdata/coin-icon-96px-webp-data-url.txt");
pub const COIN_DESCRIPTION: &str = "The walking-path coin.";

/// The near-sandbox (neard) release these tests run on. `NEAR_SANDBOX_BIN_PATH` must point to it.
pub const SANDBOX_RELEASE: &str = "2.13.4";

/// Fails unless `NEAR_SANDBOX_BIN_PATH` is set and its binary reports [`SANDBOX_RELEASE`].
pub fn check_sandbox_binary() -> Result<()> {
    let bin = std::env::var("NEAR_SANDBOX_BIN_PATH").context(
        "set NEAR_SANDBOX_BIN_PATH to a near-sandbox binary; the tests never download one",
    )?;
    let out = std::process::Command::new(&bin)
        .arg("--version")
        .output()
        .with_context(|| format!("run {bin} --version"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let want = format!("(release {SANDBOX_RELEASE})");
    if !text.contains(&want) {
        bail!(
            "near-sandbox at {bin} is not release {SANDBOX_RELEASE}: {}",
            text.trim()
        );
    }
    Ok(())
}

pub struct Account {
    pub id: AccountId,
    pub signer: Arc<Signer>,
}

pub struct World {
    pub sandbox: Sandbox,
    pub network: NetworkConfig,
    pub factory: Account,
    pub creator: Account,
    pub alice: Account,
    pub coin: AccountId,
    pub coin_code_hash: CryptoHash,
    pub launch: TransactionResult,
}

/// One `EVENT_JSON` log from a successful receipt outcome.
#[derive(Debug, Clone)]
pub struct LoggedEvent {
    pub receipt_id: String,
    pub executor: String,
    pub event: Value,
}

/// The built wasm of one contract crate, refused when it is missing or older than any of its
/// sources, so a sandbox test never runs against stale code.
pub fn wasm(name: &str) -> Result<Vec<u8>> {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let contract_dir = match name {
        "fair_chance_coin" => "contracts/coin",
        "fair_chance_factory" => "contracts/factory",
        other => bail!("unknown contract {other}"),
    };
    let path = repo
        .join("target/near")
        .join(name)
        .join(format!("{name}.wasm"));
    let sources: Vec<PathBuf> = [
        contract_dir,
        "rule-engine",
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
    ]
    .iter()
    .map(|p| repo.join(p))
    .collect();
    let newer = newer_sources(&path, &sources)
        .context("build the contracts first: scripts/build-contracts.sh")?;
    if !newer.is_empty() {
        bail!(
            "{} is older than {} source file(s), first {}; run scripts/build-contracts.sh",
            path.display(),
            newer.len(),
            newer[0].display()
        );
    }
    std::fs::read(&path).with_context(|| format!("read {}", path.display()))
}

/// Every file under `sources` (files or folders, `target` folders skipped) that changed after
/// `artifact`. An error when `artifact` does not exist.
pub fn newer_sources(artifact: &Path, sources: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let built = std::fs::metadata(artifact)
        .and_then(|m| m.modified())
        .with_context(|| format!("missing {}", artifact.display()))?;
    let mut newer = Vec::new();
    let mut stack: Vec<PathBuf> = sources.to_vec();
    while let Some(path) = stack.pop() {
        let meta = std::fs::metadata(&path).with_context(|| format!("read {}", path.display()))?;
        if meta.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            for entry in std::fs::read_dir(&path)? {
                stack.push(entry?.path());
            }
        } else if path != artifact && meta.modified()? > built {
            newer.push(path);
        }
    }
    newer.sort();
    Ok(newer)
}

fn genesis(name: &str) -> Result<(GenesisAccount, Account)> {
    let account = GenesisAccount::generate_with_name(name.parse()?);
    let signer = Signer::from_secret_key(account.private_key.parse()?)?;
    let handle = Account {
        id: account.account_id.clone(),
        signer,
    };
    Ok((account, handle))
}

impl World {
    /// Start a fresh sandbox, put the coin code in global storage by hash, deploy the factory
    /// and launch one coin from the creator account.
    pub async fn launch() -> Result<Self> {
        check_sandbox_binary()?;
        let coin_code = wasm("fair_chance_coin")?;
        let factory_code = wasm("fair_chance_factory")?;
        let (factory_g, factory) = genesis("factory.sandbox")?;
        let (creator_g, creator) = genesis("creator.sandbox")?;
        let (alice_g, alice) = genesis("alice.sandbox")?;
        let sandbox = Sandbox::start_sandbox_with_config(SandboxConfig {
            additional_accounts: vec![factory_g, creator_g, alice_g],
            // The sandbox makes blocks fast and by default drops blocks older than a few
            // epochs of 60 blocks. The block scan reads from the launch block, so keep all.
            additional_config: Some(json!({ "gc_num_epochs_to_keep": 10_000 })),
            ..Default::default()
        })
        .await?;
        let network = NetworkConfig::from_rpc_url("sandbox", sandbox.rpc_addr.parse()?);

        let coin_code_hash = CryptoHash::hash(&coin_code);
        Contract::deploy_global_contract_code(coin_code)
            .as_hash()
            .with_signer(factory.id.clone(), factory.signer.clone())
            .send_to(&network)
            .await?
            .assert_success();
        Contract::deploy(factory.id.clone())
            .use_code(factory_code)
            .with_init_call(
                "new",
                json!({ "coin_code_hash": coin_code_hash.to_string() }),
            )?
            .with_signer(factory.signer.clone())
            .send_to(&network)
            .await?
            .assert_success();

        let coin: AccountId = format!("{COIN_TICKER}.{}", factory.id).parse()?;
        let launch = send(
            &network,
            &creator,
            &factory.id,
            "launch",
            json!({
                "name": "Walk Coin",
                "ticker": COIN_TICKER,
                "icon": COIN_ICON,
                "description": COIN_DESCRIPTION,
                "opening_buy": null,
            }),
            NearToken::from_near(5),
        )
        .await?;
        Ok(Self {
            sandbox,
            network,
            factory,
            creator,
            alice,
            coin,
            coin_code_hash,
            launch,
        })
    }

    pub fn rpc_url(&self) -> &str {
        &self.sandbox.rpc_addr
    }

    /// Sign and send one function call; fails if any receipt failed.
    pub async fn call(
        &self,
        who: &Account,
        contract: &AccountId,
        method: &str,
        args: Value,
        deposit: NearToken,
    ) -> Result<TransactionResult> {
        send(&self.network, who, contract, method, args, deposit).await
    }

    /// Sign and send one function call that must fail; returns the failure text.
    pub async fn call_refused(
        &self,
        who: &Account,
        contract: &AccountId,
        method: &str,
        args: Value,
        deposit: NearToken,
    ) -> Result<String> {
        let result = Contract(contract.clone())
            .call_function(method, args)
            .transaction()
            .deposit(deposit)
            .with_signer(who.id.clone(), who.signer.clone())
            .send_to(&self.network)
            .await;
        match result {
            Ok(r) if r.is_success() => {
                bail!("{method} on {contract} succeeded; a refusal was expected")
            }
            Ok(r) => Ok(format!("{r:?}")),
            Err(e) => Ok(e.to_string()),
        }
    }

    pub async fn view(&self, contract: &AccountId, method: &str, args: Value) -> Result<Value> {
        let data: Data<Value> = Contract(contract.clone())
            .call_function(method, args)
            .read_only()
            .fetch_from(&self.network)
            .await?;
        Ok(data.data)
    }
}

/// Every `EVENT_JSON` log with this standard and event name in the transaction's receipts.
pub fn events(result: &TransactionResult, standard: &str, name: &str) -> Vec<LoggedEvent> {
    let TransactionResult::Full(full) = result else {
        return vec![];
    };
    full.receipt_outcomes()
        .iter()
        .filter(|o| o.is_success())
        .flat_map(|o| {
            o.logs.iter().filter_map(move |line| {
                let event: Value = serde_json::from_str(line.strip_prefix("EVENT_JSON:")?).ok()?;
                (event["standard"] == standard && event["event"] == name).then(|| LoggedEvent {
                    receipt_id: o.transaction_hash.to_string(),
                    executor: o.executor_id.to_string(),
                    event,
                })
            })
        })
        .collect()
}

/// Field names of `fc_sell` in INTERFACE.md (unchanged since 1.0.0-rc.4).
pub const FC_SELL_FIELDS: [&str; 16] = [
    "account",
    "class",
    "tokens_asked",
    "tokens_sold",
    "marked_sold",
    "near_out",
    "fee_near",
    "creator_fee_near",
    "penalty_near",
    "fill",
    "limit_reason",
    "allowance_used",
    "allowance_left",
    "cap_used",
    "cap_left",
    "window_end_ns",
];

/// The sorted field names of one event's data object.
pub fn field_names(event: &Value) -> Vec<String> {
    let mut names: Vec<String> = event["data"][0]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// Sign and send one function call; fails if any receipt failed.
async fn send(
    network: &NetworkConfig,
    who: &Account,
    contract: &AccountId,
    method: &str,
    args: Value,
    deposit: NearToken,
) -> Result<TransactionResult> {
    let result = Contract(contract.clone())
        .call_function(method, args)
        .transaction()
        .deposit(deposit)
        .with_signer(who.id.clone(), who.signer.clone())
        .send_to(network)
        .await?;
    if !result.is_success() {
        bail!("{method} on {contract} failed: {result:?}");
    }
    Ok(result)
}

impl World {
    /// Waits until the final head reaches the last block of the transaction's receipts, so a
    /// reader that scans to the final head sees all of them. Returns that block height.
    pub async fn wait_final(&self, result: &TransactionResult) -> Result<u64> {
        let TransactionResult::Full(full) = result else {
            bail!("the transaction has no execution result yet")
        };
        let mut last = 0;
        for outcome in full.receipt_outcomes() {
            let block = near_api::Chain::block()
                .at(near_api::Reference::AtBlockHash(outcome.block_hash))
                .fetch_from(&self.network)
                .await?;
            last = last.max(block.header.height);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let head = near_api::Chain::block_number()
                .at(near_api::Reference::Final)
                .fetch_from(&self.network)
                .await?;
            if head >= last {
                return Ok(last);
            }
            if std::time::Instant::now() > deadline {
                bail!("final head {head} did not reach block {last} in 60 s");
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }
}
