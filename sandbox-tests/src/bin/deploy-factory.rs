//! The factory deploy step (see `fair_chance_sandbox_tests::factory_deploy`).
//!
//!   deploy-factory --dry-run            print the plan and its cost from the wasm files; reads no
//!                                       chain, loads no key, signs nothing
//!   deploy-factory --network testnet --start-balance <NEAR> --spend-cap <NEAR> --keep <NEAR>
//!                                       deploy on testnet; refused unless the builder sets
//!                                       FC_BUILDER_SAID_YES_TO_TESTNET_SIGNING=yes for that run.
//!                                       Before each signing it reads the funder balance and
//!                                       refuses a step that would spend more than the cap since
//!                                       the start balance or leave less than --keep
//!
//! Both read FC_TESTNET_FUNDER. Testnet also reads FC_TESTNET_RPC_URL and loads the funder key
//! from the system keychain (build with `--features testnet-keychain`; scripts/deploy-factory.sh
//! does this for `--network testnet`). Testnet deploys only the coin wasm from
//! scripts/build-coin-reproducible.sh, built from HEAD of a clean tree.

use anyhow::{bail, Context, Result};
use fair_chance_sandbox_tests::factory_deploy::{parse_deploy_arguments, DeployMode, DeployPlan};
use fair_chance_sandbox_tests::option_t::{funding, TESTNET_YES_FLAG};
use fair_chance_sandbox_tests::{
    reproducible_coin_build, wasm, LAUNCH_DEPOSIT, REPRODUCIBLE_COIN_DIR,
};
use near_api::types::{AccountId, CryptoHash, NearToken};

/// The default Option T set: 20 buyers, 1 NEAR buys, 0.5 NEAR extra each.
const OPTION_T_BUYERS: usize = 20;

#[tokio::main]
async fn main() {
    if let Err(e) = start().await {
        eprintln!("deploy-factory: {e:#}");
        std::process::exit(1);
    }
}

async fn start() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = parse_deploy_arguments(&args, std::env::var(TESTNET_YES_FLAG).ok().as_deref())?;
    let funder: AccountId = std::env::var("FC_TESTNET_FUNDER")
        .context("set FC_TESTNET_FUNDER")?
        .parse()?;
    if !funder.as_str().ends_with(".testnet") {
        bail!("{funder} is not a .testnet account");
    }
    let (coin_code, source) = coin_code(mode)?;
    let plan = DeployPlan::new(funder, coin_code, wasm("fair_chance_factory")?)?;
    println!("deploy-factory: coin wasm from {source}");
    for line in plan.describe() {
        println!("deploy-factory: {line}");
    }
    let option_t = funding(
        LAUNCH_DEPOSIT,
        NearToken::from_near(1),
        NearToken::from_millinear(500),
        OPTION_T_BUYERS,
    );
    println!(
        "deploy-factory: the default Option T set then moves {} from the funder (creator 5.5 NEAR, {OPTION_T_BUYERS} buyers x 1.5 NEAR), gas not included",
        option_t.exact_amount_display()
    );
    println!(
        "deploy-factory: deploy plus default Option T: {} plus gas",
        plan.total().saturating_add(option_t).exact_amount_display()
    );
    match mode {
        DeployMode::DryRun => {
            println!("deploy-factory: dry run; nothing was read from a chain or signed");
            Ok(())
        }
        DeployMode::Testnet(limits) => testnet(&plan, &limits).await,
    }
}

/// The coin wasm and where it came from. Testnet takes only the reproducible build of HEAD of a
/// clean tree, with the checksum cargo-near printed. A dry run falls back to the local build
/// when the reproducible output is missing, and says so.
fn coin_code(mode: DeployMode) -> Result<(Vec<u8>, String)> {
    match (reproducible_coin_build(), mode) {
        (Ok(build), _) => {
            let hash = CryptoHash::hash(&build.wasm).to_string();
            if hash != build.cargo_near_sha256_base58 {
                bail!(
                    "the reproducible wasm hashes to {hash}, cargo-near printed {}",
                    build.cargo_near_sha256_base58
                );
            }
            if matches!(mode, DeployMode::Testnet(_)) {
                let head = git(&["rev-parse", "HEAD"])?;
                if head != build.commit {
                    bail!("the reproducible wasm is from {}, HEAD is {head}; build again", build.commit);
                }
                if !git(&["status", "--porcelain"])?.is_empty() {
                    bail!("the tree has changes; commit them and build again");
                }
            }
            Ok((build.wasm, format!("{REPRODUCIBLE_COIN_DIR} (commit {})", build.commit)))
        }
        (Err(e), DeployMode::Testnet(_)) => Err(e),
        (Err(_), DeployMode::DryRun) => Ok((
            wasm("fair_chance_coin")?,
            "target/near/fair_chance_coin (local non-reproducible build: the reproducible build has not run, so its size can differ)".to_string(),
        )),
    }
}

fn git(args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("git").args(args).output()?;
    if !out.status.success() {
        bail!("git {args:?} failed");
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

#[cfg(feature = "testnet-keychain")]
async fn testnet(
    plan: &DeployPlan,
    limits: &fair_chance_sandbox_tests::spend_guard::SpendLimits,
) -> Result<()> {
    use fair_chance_sandbox_tests::factory_deploy::{check_deployed, deploy};
    use fair_chance_sandbox_tests::option_t::funder_signer_from_keychain;
    let rpc_url = std::env::var("FC_TESTNET_RPC_URL").context("set FC_TESTNET_RPC_URL")?;
    if rpc_url.contains("mainnet") {
        bail!("FC_TESTNET_RPC_URL names mainnet");
    }
    let network = near_api::NetworkConfig::from_rpc_url("testnet", rpc_url.parse()?);
    let signer = funder_signer_from_keychain(&plan.funder, &network).await?;
    let funder = fair_chance_sandbox_tests::Account {
        id: plan.funder.clone(),
        signer,
    };
    let report = deploy(plan, &network, &funder, Some(limits)).await?;
    match report.global_deploy_tx {
        Some(tx) => println!("deploy-factory: global deploy tx {tx}"),
        None => println!("deploy-factory: global code was already on chain; no global deploy tx"),
    }
    println!("deploy-factory: factory tx {}", report.factory_tx);
    for line in check_deployed(plan, &network).await? {
        println!("deploy-factory: checked {line}");
    }
    Ok(())
}

#[cfg(not(feature = "testnet-keychain"))]
async fn testnet(
    _plan: &DeployPlan,
    _limits: &fair_chance_sandbox_tests::spend_guard::SpendLimits,
) -> Result<()> {
    bail!(
        "the testnet path loads the funder key from the keychain; build with --features testnet-keychain (scripts/deploy-factory.sh does this for --network testnet). Nothing was signed"
    )
}
