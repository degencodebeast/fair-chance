//! A fresh near-sandbox with the factory deployed and one coin launched.
//!
//! Each [`World`] starts its own sandbox: the near-sandbox crate picks a free RPC and network
//! port and makes a new temporary data folder, so runs in parallel do not share either.
//! The binary comes from `NEAR_SANDBOX_BIN_PATH`.

pub mod amount_text;
pub mod app_coin;
pub mod burst;
pub mod buyer_funding;
pub mod factory_deploy;
pub mod option_t;
pub mod page_and_watcher_reads;
pub mod pass_list;
pub mod rate_limit;
pub mod refusal_texts;
pub mod rpcs;
pub mod sells_only;
pub mod spend_guard;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
pub use near_api::types::account::ContractState;
use near_api::types::transaction::result::TransactionResult;
use near_api::types::{AccountId, CryptoHash, Data, NearToken};
use near_api::{Contract, NetworkConfig, Signer};
use near_sandbox::{GenesisAccount, Sandbox, SandboxConfig};
use serde_json::{json, Value};

/// Sandbox `config.json` patch for tests that fast-forward a day: nearcore adds (min + max block
/// production delay) / 2 of block time per fast-forwarded block, so a 200 s max delay makes one
/// such block about 90 s (93 s measured), and 1,000 blocks pass 24 h. A single validator still
/// makes normal blocks at the min delay.
pub fn fast_forward_config() -> Value {
    json!({
        "gc_num_epochs_to_keep": 10_000,
        "consensus": { "max_block_production_delay": { "secs": 200, "nanos": 0 } },
    })
}

/// Sandbox `config.json` patch for a block about every `block_ms` milliseconds: a single
/// validator makes each block at the min block production delay. The max delay is twice that, and
/// all blocks are kept, as in [`World::launch_with_coin_code`].
pub fn block_time_config(block_ms: u64) -> Value {
    let delay = |ms: u64| json!({ "secs": ms / 1000, "nanos": (ms % 1000) * 1_000_000 });
    json!({
        "gc_num_epochs_to_keep": 10_000,
        "consensus": {
            "min_block_production_delay": delay(block_ms),
            "max_block_production_delay": delay(2 * block_ms),
        },
    })
}

pub const COIN_TICKER: &str = "walk";
/// The walking-path coin's icon: a 96 x 96 WebP data URL under 8 KB.
pub const COIN_ICON: &str = include_str!("../../testdata/coin-icon-96px-webp-data-url.txt");
pub const COIN_DESCRIPTION: &str = "The walking-path coin.";
/// The NEAR a creator attaches to `launch` for the coin's storage (no opening buy).
pub const LAUNCH_DEPOSIT: NearToken = NearToken::from_near(5);

/// The JSON arguments of the factory `launch` call. `opening_buy` is yoctoNEAR.
pub fn launch_args(
    name: &str,
    ticker: &str,
    icon: Option<&str>,
    description: Option<&str>,
    opening_buy: Option<NearToken>,
) -> Value {
    json!({
        "name": name,
        "ticker": ticker,
        "icon": icon,
        "description": description,
        "opening_buy": opening_buy.map(|n| n.as_yoctonear().to_string()),
    })
}

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

#[derive(Clone)]
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
    pub bob: Account,
    pub coin: AccountId,
    pub coin_code_hash: CryptoHash,
    pub launch: TransactionResult,
}

/// One `EVENT_JSON` log from a successful receipt outcome.
#[derive(Debug, Clone)]
pub struct LoggedEvent {
    pub receipt_id: String,
    pub executor: String,
    /// The block that holds the receipt outcome.
    pub block_hash: CryptoHash,
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

/// The coin wasm from the reproducible Docker build, with the commit it was built from and the
/// sha256 checksum cargo-near printed for it (`scripts/build-coin-reproducible.sh` writes all three).
pub struct ReproducibleCoinBuild {
    pub wasm: Vec<u8>,
    pub commit: String,
    pub cargo_near_sha256_base58: String,
}

/// The folder where `scripts/build-coin-reproducible.sh` puts its output.
pub const REPRODUCIBLE_COIN_DIR: &str = "target/near-reproducible/fair_chance_coin";

/// Reads the reproducible coin build output. An error when the build script has not run.
pub fn reproducible_coin_build() -> Result<ReproducibleCoinBuild> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(REPRODUCIBLE_COIN_DIR);
    let read = |file: &str| -> Result<String> {
        let path = dir.join(file);
        Ok(std::fs::read_to_string(&path)
            .with_context(|| {
                format!(
                    "read {}; run scripts/build-coin-reproducible.sh first",
                    path.display()
                )
            })?
            .trim()
            .to_string())
    };
    let wasm_path = dir.join("fair_chance_coin.wasm");
    Ok(ReproducibleCoinBuild {
        wasm: std::fs::read(&wasm_path).with_context(|| {
            format!(
                "read {}; run scripts/build-coin-reproducible.sh first",
                wasm_path.display()
            )
        })?,
        commit: read("built-commit.txt")?,
        cargo_near_sha256_base58: read("cargo-near-sha256-base58.txt")?,
    })
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
        Self::launch_with_coin_code(wasm("fair_chance_coin")?).await
    }

    /// As [`World::launch`], on a sandbox where one fast-forwarded block adds about 90 s of block
    /// time ([`fast_forward_config`]), so a test can pass 24 h in about 1,000 blocks.
    pub async fn launch_for_fast_forward() -> Result<Self> {
        Self::launch_on(wasm("fair_chance_coin")?, fast_forward_config()).await
    }

    /// As [`World::launch`], with the given coin wasm as the global code (for example the
    /// reproducible build from [`reproducible_coin_build`]).
    pub async fn launch_with_coin_code(coin_code: Vec<u8>) -> Result<Self> {
        // The sandbox makes blocks fast and by default drops blocks older than a few epochs of 60
        // blocks. The block scan reads from the launch block, so keep all.
        Self::launch_on(coin_code, json!({ "gc_num_epochs_to_keep": 10_000 })).await
    }

    /// As [`World::launch`], on a sandbox that makes one block about every `block_ms`
    /// milliseconds ([`block_time_config`]), for a rehearsal close to testnet (0.58 s per block).
    pub async fn launch_with_block_ms(block_ms: u64) -> Result<Self> {
        Self::launch_on(wasm("fair_chance_coin")?, block_time_config(block_ms)).await
    }

    async fn launch_on(coin_code: Vec<u8>, sandbox_config: Value) -> Result<Self> {
        check_sandbox_binary()?;
        let factory_code = wasm("fair_chance_factory")?;
        let (factory_g, factory) = genesis("factory.sandbox")?;
        let (creator_g, creator) = genesis("creator.sandbox")?;
        let (alice_g, alice) = genesis("alice.sandbox")?;
        let (bob_g, bob) = genesis("bob.sandbox")?;
        let sandbox = Sandbox::start_sandbox_with_config(SandboxConfig {
            additional_accounts: vec![factory_g, creator_g, alice_g, bob_g],
            additional_config: Some(sandbox_config),
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
            launch_args(
                "Walk Coin",
                COIN_TICKER,
                Some(COIN_ICON),
                Some(COIN_DESCRIPTION),
                None,
            ),
            LAUNCH_DEPOSIT,
        )
        .await?;
        let world = Self {
            sandbox,
            network,
            factory,
            creator,
            alice,
            bob,
            coin,
            coin_code_hash,
            launch,
        };
        // Buys in blocks 0 to N are marked and cannot be sold before block N + 1. The tests on
        // the walk coin trade unmarked tokens, so they start after the early window.
        world.wait_until_after_the_early_window(&world.coin).await?;
        Ok(world)
    }

    /// Waits until the final head is past the coin's early window (blocks 0 to N after its
    /// launch block), so the next transaction lands after it.
    pub async fn wait_until_after_the_early_window(&self, coin: &AccountId) -> Result<()> {
        let rules = self.view(coin, "fc_rules", json!({})).await?;
        let launch_block = rules["launch_block"]
            .as_u64()
            .context("fc_rules.launch_block")?;
        let early_blocks = rules["early_blocks"]
            .as_u64()
            .context("fc_rules.early_blocks")?;
        self.wait_for_height(launch_block + early_blocks + 1).await
    }

    pub fn rpc_url(&self) -> &str {
        &self.sandbox.rpc_addr
    }

    /// The coin account the factory makes for a ticker: `<ticker>.<factory>`.
    pub fn coin_for(&self, ticker: &str) -> Result<AccountId> {
        Ok(format!("{ticker}.{}", self.factory.id).parse()?)
    }

    /// Send one `launch` call to the factory and return the result, success or not.
    pub async fn try_launch(
        &self,
        creator: &Account,
        args: Value,
        deposit: NearToken,
    ) -> Result<TransactionResult> {
        transact(
            &self.network,
            creator,
            &self.factory.id,
            "launch",
            args,
            deposit,
        )
        .await
    }

    /// The account's contract state as the chain reports it (`view_account`), or `None` when
    /// no account exists.
    pub async fn contract_state(&self, account: &AccountId) -> Result<Option<ContractState>> {
        match near_api::Account(account.clone())
            .view()
            .fetch_from(&self.network)
            .await
        {
            Ok(data) => Ok(Some(data.data.contract_state)),
            Err(e) if format!("{e:?}").contains("UnknownAccount") => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// The account's NEAR balance (`view_account` amount).
    pub async fn balance(&self, account: &AccountId) -> Result<NearToken> {
        Ok(near_api::Account(account.clone())
            .view()
            .fetch_from(&self.network)
            .await?
            .data
            .amount)
    }

    /// Number of access keys on the account (`view_access_key_list`).
    pub async fn access_key_count(&self, account: &AccountId) -> Result<usize> {
        Ok(near_api::Account(account.clone())
            .list_keys()
            .fetch_from(&self.network)
            .await?
            .data
            .len())
    }

    /// Height of the block that holds a receipt outcome's block hash.
    pub async fn block_height_of(&self, block_hash: CryptoHash) -> Result<u64> {
        Ok(near_api::Chain::block()
            .at(near_api::Reference::AtBlockHash(block_hash))
            .fetch_from(&self.network)
            .await?
            .header
            .height)
    }

    /// Waits until the final head is at `height` or above.
    pub async fn wait_for_height(&self, height: u64) -> Result<()> {
        wait_for_final_height(&self.network, height).await
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
        match transact(&self.network, who, contract, method, args, deposit).await {
            Ok(r) if r.is_success() => {
                bail!("{method} on {contract} succeeded; a refusal was expected")
            }
            Ok(r) => Ok(format!("{r:?}")),
            Err(e) => Ok(e.to_string()),
        }
    }

    /// NEP-145: pays the storage minimum for `who`'s holder record on `coin`.
    pub async fn register(&self, who: &Account, coin: &AccountId) -> Result<TransactionResult> {
        let bounds = self.view(coin, "storage_balance_bounds", json!({})).await?;
        let min: u128 = bounds["min"].as_str().context("storage min")?.parse()?;
        self.call(
            who,
            coin,
            "storage_deposit",
            json!({}),
            NearToken::from_yoctonear(min),
        )
        .await
    }

    /// NEAR protocol storage price is 10^19 yoctoNEAR per byte. The coin's NEAR balance minus
    /// its storage lock, minus the pool's real NEAR: NEAR that belongs to neither the pool nor
    /// the storage lock. Holder records must never take it below its launch value.
    pub async fn free_near_beyond_pool(&self, coin: &AccountId) -> Result<i128> {
        let account = near_api::Account(coin.clone())
            .view()
            .fetch_from(&self.network)
            .await?
            .data;
        let lock = u128::from(account.storage_usage) * 10_000_000_000_000_000_000;
        let pool = self.view(coin, "fc_pool", json!({})).await?;
        let real_near: u128 = pool["real_near"].as_str().context("real_near")?.parse()?;
        Ok(account.amount.as_yoctonear() as i128 - lock as i128 - real_near as i128)
    }

    pub async fn view(&self, contract: &AccountId, method: &str, args: Value) -> Result<Value> {
        view_on(&self.network, contract, method, args).await
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
                    block_hash: o.block_hash,
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
    let result = transact(network, who, contract, method, args, deposit).await?;
    if !result.is_success() {
        bail!("{method} on {contract} failed: {result:?}");
    }
    Ok(result)
}

/// The gas every test transaction attaches. The factory `launch` chain needs more than the
/// 100 TGas default; unused gas is refunded.
const TRANSACTION_GAS: near_api::types::NearGas = near_api::types::NearGas::from_tgas(300);

/// Sign and send one function call on any network and return its result, success or not.
pub async fn transact(
    network: &NetworkConfig,
    who: &Account,
    contract: &AccountId,
    method: &str,
    args: Value,
    deposit: NearToken,
) -> Result<TransactionResult> {
    transact_with_gas(
        network,
        who,
        contract,
        method,
        args,
        deposit,
        TRANSACTION_GAS,
    )
    .await
}

/// As [`transact`], with the given prepaid gas. The chain holds gas x gas price of the signer's
/// NEAR until the call ends, so an account with little NEAR needs a smaller value.
pub async fn transact_with_gas(
    network: &NetworkConfig,
    who: &Account,
    contract: &AccountId,
    method: &str,
    args: Value,
    deposit: NearToken,
    gas: near_api::types::NearGas,
) -> Result<TransactionResult> {
    Ok(Contract(contract.clone())
        .call_function(method, args)
        .transaction()
        .deposit(deposit)
        .gas(gas)
        .with_signer(who.id.clone(), who.signer.clone())
        .send_to(network)
        .await?)
}

/// One function-call action of a [`World::batch`] transaction.
pub struct FunctionCall {
    pub method: String,
    pub args: Value,
    pub deposit: NearToken,
}

impl FunctionCall {
    pub fn new(method: &str, args: Value, deposit: NearToken) -> Self {
        Self {
            method: method.to_string(),
            args,
            deposit,
        }
    }
}

impl World {
    /// Sign and send one transaction that holds every call as a function-call action to
    /// `contract`, as a wallet does; returns its result, success or not. The actions run in one
    /// receipt, so one failed action rolls back all of them. The gas is split evenly.
    pub async fn batch(
        &self,
        who: &Account,
        contract: &AccountId,
        calls: Vec<FunctionCall>,
    ) -> Result<TransactionResult> {
        batch_on(&self.network, who, contract, calls).await
    }

    /// The NEAR the transaction burnt for gas, over its own outcome and every receipt outcome.
    pub fn tokens_burnt(result: &TransactionResult) -> Result<u128> {
        let TransactionResult::Full(full) = result else {
            bail!("the transaction has no execution result yet")
        };
        Ok(full
            .outcomes()
            .iter()
            .map(|o| o.tokens_burnt.as_yoctonear())
            .sum())
    }

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

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` (UTC) for a time in milliseconds since the Unix epoch.
pub fn stamp_at(unix_ms: u128) -> String {
    let ms = unix_ms % 1000;
    let secs = unix_ms / 1000;
    let (days, day_secs) = ((secs / 86_400) as i64, secs % 86_400);
    // Days since 1970-01-01 to a civil date (H. Hinnant, "chrono-compatible low-level date
    // algorithms").
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        day_secs / 3600,
        day_secs / 60 % 60,
        day_secs % 60
    )
}

/// The current UTC time for a step line, as [`stamp_at`] writes it.
pub fn stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    stamp_at(now.as_millis())
}

/// One view call on any network.
pub async fn view_on(
    network: &NetworkConfig,
    contract: &AccountId,
    method: &str,
    args: Value,
) -> Result<Value> {
    let data: Data<Value> = rate_limit::with_rate_limit_backoff(
        &format!("the view {method} on {contract}"),
        rate_limit::run_budget(),
        || {
            Contract(contract.clone())
                .call_function(method, args.clone())
                .read_only()
                .fetch_from(network)
        },
    )
    .await?;
    Ok(data.data)
}

/// Sign and send one transaction on any network that holds every call as a function-call action
/// to `contract`, as a wallet does (see [`World::batch`]).
pub async fn batch_on(
    network: &NetworkConfig,
    who: &Account,
    contract: &AccountId,
    calls: Vec<FunctionCall>,
) -> Result<TransactionResult> {
    batch_on_with_gas(network, who, contract, calls, TRANSACTION_GAS).await
}

/// As [`batch_on`], with the given prepaid gas for the whole transaction, split evenly.
pub async fn batch_on_with_gas(
    network: &NetworkConfig,
    who: &Account,
    contract: &AccountId,
    calls: Vec<FunctionCall>,
    total_gas: near_api::types::NearGas,
) -> Result<TransactionResult> {
    Ok(
        near_api::Transaction::construct(who.id.clone(), contract.clone())
            .add_actions(function_call_actions(calls, total_gas)?)
            .with_signer(who.signer.clone())
            .send_to(network)
            .await?,
    )
}

/// One function-call action per call, with `total_gas` split evenly.
pub(crate) fn function_call_actions(
    calls: Vec<FunctionCall>,
    total_gas: near_api::types::NearGas,
) -> Result<Vec<near_api::types::Action>> {
    let gas = near_api::types::NearGas::from_gas(total_gas.as_gas() / calls.len().max(1) as u64);
    calls
        .into_iter()
        .map(|call| {
            Ok(near_api::types::Action::FunctionCall(Box::new(
                near_api::types::transaction::actions::FunctionCallAction {
                    method_name: call.method,
                    args: serde_json::to_vec(&call.args)?,
                    gas,
                    deposit: call.deposit,
                },
            )))
        })
        .collect()
}

/// As [`batch_on_with_gas`], sent once through [`send_once`].
pub async fn batch_once(
    read: &NetworkConfig,
    send: &NetworkConfig,
    who: &Account,
    contract: &AccountId,
    calls: Vec<FunctionCall>,
    total_gas: near_api::types::NearGas,
) -> Result<TransactionResult> {
    let transaction = near_api::Transaction::construct(who.id.clone(), contract.clone())
        .add_actions(function_call_actions(calls, total_gas)?)
        .with_signer(who.signer.clone());
    send_once(transaction, &who.id, read, send).await
}

/// As [`transact_with_gas`], sent once through [`send_once`].
#[allow(clippy::too_many_arguments)]
pub async fn transact_once(
    read: &NetworkConfig,
    send: &NetworkConfig,
    who: &Account,
    contract: &AccountId,
    method: &str,
    args: Value,
    deposit: NearToken,
    gas: near_api::types::NearGas,
) -> Result<TransactionResult> {
    let transaction = Contract(contract.clone())
        .call_function(method, args)
        .transaction()
        .deposit(deposit)
        .gas(gas)
        .with_signer(who.id.clone(), who.signer.clone());
    send_once(transaction, &who.id, read, send).await
}

/// The account that signs `transaction`.
fn transaction_signer(
    transaction: &near_api::advanced::ExecuteSignedTransaction,
) -> Result<AccountId> {
    use near_api::advanced::TransactionableOrSigned;
    match &transaction.transaction {
        TransactionableOrSigned::Transactionable(t) => Ok(t
            .prepopulated()
            .map_err(|e| anyhow::anyhow!("the transaction is not valid: {e}; nothing was sent"))?
            .signer_id),
        TransactionableOrSigned::Signed((signed, _)) => Ok(signed.transaction.signer_id().clone()),
    }
}

/// Signs `transaction` without sending it. The nonce and a final block hash come from `send`; when
/// that read fails, nothing went out, so they come from one read on the first endpoint of `read`
/// (FastNEAR on testnet) instead. When that fails too, it stops with "nothing was sent". No error
/// text carries an RPC URL.
pub async fn presign_once(
    transaction: near_api::advanced::ExecuteSignedTransaction,
    read: &NetworkConfig,
    send: &NetworkConfig,
) -> Result<near_api::advanced::ExecuteSignedTransaction> {
    // The nonce belongs to the account that signs, so it comes from the transaction itself, as in
    // near-api's presign_with; no caller can name another account.
    let signer_id = &transaction_signer(&transaction)?;
    let failed = |what: &str, e: &dyn std::fmt::Display| {
        anyhow::anyhow!(
            "signing for {signer_id} failed before any send: {what}: {}; nothing was sent",
            rate_limit::without_urls(&format!("{e}"))
        )
    };
    let key = transaction
        .signer
        .get_public_key()
        .await
        .map_err(|e| failed("the public key", &e))?;
    let (nonce, block_hash, _) = match transaction
        .signer
        .fetch_tx_nonce(signer_id.clone(), key, send)
        .await
    {
        Ok(found) => found,
        Err(first) => {
            println!(
                "send: the nonce read for {signer_id} on the send RPC failed ({}); nothing was sent; reading it once on the read RPC",
                rate_limit::without_urls(&format!("{first}"))
            );
            let mut fallback = read.clone();
            fallback.rpc_endpoints.truncate(1);
            transaction
                .signer
                .fetch_tx_nonce(signer_id.clone(), key, &fallback)
                .await
                .map_err(|e| failed("the nonce read on both RPCs", &e))?
        }
    };
    transaction
        .presign_offline(key, block_hash, nonce)
        .await
        .map_err(|e| failed("signing", &e))
}

/// Signs `transaction` ([`presign_once`]: the nonce from `send`, or once from `read` when that
/// read fails), sends it once to `send`, and returns its result. near-api tries a network's endpoints in turn, so `send` must hold one endpoint with one
/// try ([`rpcs::testnet_rpcs`]). When the send gives no answer or an unknown result, it looks up the
/// transaction by its hash on `read` and returns what the chain holds; it never sends it again, on
/// either RPC. A hash the chain does not know stops the run. A failure before the send (signing,
/// the nonce read) means nothing went out. No error text carries an RPC URL.
pub async fn send_once(
    transaction: near_api::advanced::ExecuteSignedTransaction,
    signer_id: &AccountId,
    read: &NetworkConfig,
    send: &NetworkConfig,
) -> Result<TransactionResult> {
    send_once_and_flag(transaction, signer_id, read, send, None).await
}

/// As [`send_once`]; when the send gives no success answer (any error, or a transaction that
/// failed), it sets `unclear` at once, before any lookup or wait, so a caller can stop other sends.
pub async fn send_once_and_flag(
    transaction: near_api::advanced::ExecuteSignedTransaction,
    signer_id: &AccountId,
    read: &NetworkConfig,
    send: &NetworkConfig,
    unclear: Option<&std::sync::atomic::AtomicBool>,
) -> Result<TransactionResult> {
    let flag = || {
        if let Some(unclear) = unclear {
            unclear.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    };
    let signs = transaction_signer(&transaction)?;
    if &signs != signer_id {
        bail!("the transaction is signed by {signs}, not {signer_id}; nothing was sent");
    }
    // A transaction signed before (the pre-signed burst) goes out as it is, with no nonce read
    // before its send; any other is signed here with one nonce read.
    let signed = match &transaction.transaction {
        near_api::advanced::TransactionableOrSigned::Signed(_) => transaction,
        near_api::advanced::TransactionableOrSigned::Transactionable(_) => {
            presign_once(transaction, read, send).await?
        }
    };
    let hash = match &signed.transaction {
        near_api::advanced::TransactionableOrSigned::Signed((tx, _)) => tx.get_hash(),
        near_api::advanced::TransactionableOrSigned::Transactionable(_) => {
            bail!("the transaction of {signer_id} is not signed; nothing was sent")
        }
    };
    let sent = signed.send_to(send).await;
    let error = match sent {
        Ok(result) => {
            if !result.is_success() {
                flag();
            }
            return Ok(result);
        }
        Err(e) => {
            flag();
            rate_limit::without_urls(&format!("{e:?}"))
        }
    };
    println!("send: tx {hash} of {signer_id} gave no clear answer; looking it up by hash, not sending it again");
    // First a lookup that does not wait: a node answers UNKNOWN_TRANSACTION at once for a hash it
    // never saw. A lookup that waits for Final instead hangs until the node times out (HTTP 408),
    // which is an unknown result, not "not found".
    let seen = rate_limit::with_rate_limit_backoff(
        &format!("the status of tx {hash}"),
        rate_limit::run_budget(),
        || async {
            match near_api::Transaction::status_with_options(
                signer_id.clone(),
                hash,
                near_api::types::TxExecutionStatus::None,
            )
            .fetch_from(read)
            .await
            {
                Ok(result) => Ok(Lookup::Done(Box::new(result))),
                Err(e) => {
                    let text = format!("{e:?}");
                    if text.contains("UnknownTransaction") || text.contains("UNKNOWN_TRANSACTION") {
                        Ok(Lookup::Missing)
                    } else if text.contains("ConversionError") {
                        // The node knows the hash, but the outcome is not complete yet.
                        Ok(Lookup::Pending)
                    } else {
                        Err(e)
                    }
                }
            }
        },
    )
    .await?;
    let result = match seen {
        Lookup::Missing => bail!(
            "tx {hash} of {signer_id} was not found on chain after a send with no clear answer ({error}); stopped, not sent again"
        ),
        Lookup::Done(result) => *result,
        Lookup::Pending => {
            println!("send: tx {hash} is on chain but not done; waiting for its final outcome");
            rate_limit::with_rate_limit_backoff(
                &format!("the final status of tx {hash}"),
                rate_limit::run_budget(),
                || {
                    near_api::Transaction::status_with_options(
                        signer_id.clone(),
                        hash,
                        near_api::types::TxExecutionStatus::Final,
                    )
                    .fetch_from(read)
                },
            )
            .await?
        }
    };
    println!("send: tx {hash} found on chain by its hash");
    Ok(TransactionResult::Full(Box::new(result)))
}

/// What a lookup by hash found after a send with no clear answer.
enum Lookup {
    Missing,
    Pending,
    Done(Box<near_api::types::transaction::result::ExecutionFinalResult>),
}

/// The account view, or `None` when the node answers that the account does not exist. Reads back
/// off on a 429; any other error is an unknown result.
pub async fn account_view(
    network: &NetworkConfig,
    id: &AccountId,
) -> Result<Option<near_api::types::account::Account>> {
    rate_limit::with_rate_limit_backoff(
        &format!("the account {id}"),
        rate_limit::run_budget(),
        || async {
            match near_api::Account(id.clone())
                .view()
                .fetch_from(network)
                .await
            {
                Ok(data) => Ok(Some(data.data)),
                Err(e) if format!("{e:?}").contains("UnknownAccount") => Ok(None),
                Err(e) => Err(e),
            }
        },
    )
    .await
}

/// Waits until the final head of any network is at `height` or above (60 s at most). It reads the
/// head every 500 ms and backs off on a 429.
pub async fn wait_for_final_height(network: &NetworkConfig, height: u64) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let head =
            rate_limit::with_rate_limit_backoff("the final head", rate_limit::run_budget(), || {
                near_api::Chain::block_number()
                    .at(near_api::Reference::Final)
                    .fetch_from(network)
            })
            .await?;
        if head >= height {
            return Ok(());
        }
        if std::time::Instant::now() > deadline {
            bail!("final head {head} did not reach block {height} in 60 s");
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// How many blocks before `launch_block` the watcher and the page start a coin's block scan
/// (`LaunchScanMarginBlocks` in watcher/internal/eventsource/source.go, `LAUNCH_SCAN_MARGIN_BLOCKS`
/// in web/src/chain.ts). The launch transaction must sit inside this margin.
pub const LAUNCH_SCAN_MARGIN_BLOCKS: u64 = 10;

/// The hash of the transaction (from its first outcome).
pub fn transaction_hash(result: &TransactionResult) -> Result<CryptoHash> {
    let TransactionResult::Full(full) = result else {
        bail!("the transaction has no execution result yet")
    };
    Ok(full
        .outcomes()
        .first()
        .context("the transaction has no outcome")?
        .transaction_hash)
}

/// A fresh sandbox with these genesis accounts (each with its own generated key), for tests that
/// deploy their own contracts. The binary comes from `NEAR_SANDBOX_BIN_PATH`.
pub async fn sandbox_with_accounts(
    names: &[&str],
) -> Result<(Sandbox, NetworkConfig, Vec<Account>)> {
    check_sandbox_binary()?;
    let mut genesis_accounts = Vec::new();
    let mut accounts = Vec::new();
    for name in names {
        let (g, a) = genesis(name)?;
        genesis_accounts.push(g);
        accounts.push(a);
    }
    let sandbox = Sandbox::start_sandbox_with_config(SandboxConfig {
        additional_accounts: genesis_accounts,
        additional_config: Some(json!({ "gc_num_epochs_to_keep": 10_000 })),
        ..Default::default()
    })
    .await?;
    let network = NetworkConfig::from_rpc_url("sandbox", sandbox.rpc_addr.parse()?);
    Ok((sandbox, network, accounts))
}

/// The hash of the block that included the transaction itself (its first outcome).
pub fn transaction_block_hash(result: &TransactionResult) -> Result<CryptoHash> {
    let TransactionResult::Full(full) = result else {
        bail!("the transaction has no execution result yet")
    };
    Ok(full
        .outcomes()
        .first()
        .context("the transaction has no outcome")?
        .block_hash)
}
