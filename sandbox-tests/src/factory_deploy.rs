//! The factory deploy step: the coin code goes into global storage by its code hash (immutable,
//! signed by the funder), then one transaction from the funder makes `factory.<funder>`, funds
//! it, gives it the funder's public key, deploys the factory code and calls `new` with the coin
//! code hash. Either the whole factory transaction lands or none of it does.
//!
//! [`DeployPlan`] works out the actions and costs from the real wasm bytes before anything is
//! signed, so a dry run shows the same numbers the deploy uses. The network is a parameter: the
//! sandbox test and the testnet run take the same code path.

use anyhow::{bail, Context, Result};
use near_api::types::transaction::actions::{
    AddKeyAction, CreateAccountAction, DeployContractAction, FunctionCallAction, TransferAction,
};
use near_api::types::{
    AccessKey, AccessKeyPermission, AccountId, Action, CryptoHash, NearGas, NearToken,
};
use near_api::{Contract, NetworkConfig};
use serde_json::json;

use crate::option_t::TESTNET_YES_FLAG;
use crate::{transaction_hash, view_on, Account, ContractState};

/// NEAR burnt per byte of code put into global storage: nearcore parameters.yaml,
/// `global_contract_storage_amount_per_byte: 0.0001 N`, ten times normal storage.
pub const GLOBAL_CONTRACT_YOCTO_PER_BYTE: u128 = 100_000_000_000_000_000_000;
/// NEAR locked per byte of account storage (`storage_amount_per_byte`, 10^19 yoctoNEAR).
pub const STORAGE_YOCTO_PER_BYTE: u128 = 10_000_000_000_000_000_000;
/// Bytes the factory account holds besides its code: the account record, its one key and the
/// contract state (the 32-byte coin code hash). Rounded up.
pub const FACTORY_STATE_MARGIN_BYTES: u128 = 1_000;
/// NEAR the factory gets on top of its storage.
pub const FACTORY_EXTRA: NearToken = NearToken::from_millinear(500);
/// The factory account is `<this>.<funder>`.
pub const FACTORY_SUB_ACCOUNT: &str = "factory";
/// Gas for the factory `new` call inside the factory transaction.
const NEW_GAS: NearGas = NearGas::from_tgas(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployMode {
    /// Print the plan from the wasm bytes; read no chain, load no key, sign nothing.
    DryRun,
    /// Sign on testnet with the funder key from the keychain.
    Testnet,
}

/// Parses the arguments after the program name: exactly `--dry-run`, or exactly
/// `--network testnet` with [`TESTNET_YES_FLAG`] set to `yes`. Anything else is refused.
pub fn parse_deploy_arguments(
    args: &[String],
    testnet_yes_flag: Option<&str>,
) -> Result<DeployMode> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["--dry-run"] => Ok(DeployMode::DryRun),
        ["--network", "testnet"] if testnet_yes_flag == Some("yes") => Ok(DeployMode::Testnet),
        ["--network", "testnet"] => bail!(
            "the deploy signs real transactions on testnet; it runs there only when the builder sets {TESTNET_YES_FLAG}=yes for that run"
        ),
        _ => bail!(
            "use exactly --dry-run or --network testnet, not {args:?}; nothing was run"
        ),
    }
}

/// The deploy, worked out from the real wasm bytes before anything is signed.
#[derive(Debug, Clone)]
pub struct DeployPlan {
    pub funder: AccountId,
    pub factory: AccountId,
    pub coin_code: Vec<u8>,
    pub coin_code_hash: CryptoHash,
    pub factory_code: Vec<u8>,
    pub factory_code_hash: CryptoHash,
    /// NEAR the global deploy burns (not locked: nobody gets it back).
    pub global_burn: NearToken,
    /// NEAR the funder moves into the factory account.
    pub factory_funding: NearToken,
}

impl DeployPlan {
    pub fn new(funder: AccountId, coin_code: Vec<u8>, factory_code: Vec<u8>) -> Result<Self> {
        if coin_code.is_empty() || factory_code.is_empty() {
            bail!(
                "a wasm is empty: coin {} bytes, factory {} bytes",
                coin_code.len(),
                factory_code.len()
            );
        }
        let factory: AccountId = format!("{FACTORY_SUB_ACCOUNT}.{funder}").parse()?;
        let global_burn =
            NearToken::from_yoctonear(coin_code.len() as u128 * GLOBAL_CONTRACT_YOCTO_PER_BYTE);
        let factory_storage =
            (factory_code.len() as u128 + FACTORY_STATE_MARGIN_BYTES) * STORAGE_YOCTO_PER_BYTE;
        Ok(Self {
            coin_code_hash: CryptoHash::hash(&coin_code),
            factory_code_hash: CryptoHash::hash(&factory_code),
            funder,
            factory,
            coin_code,
            factory_code,
            global_burn,
            factory_funding: NearToken::from_yoctonear(factory_storage)
                .saturating_add(FACTORY_EXTRA),
        })
    }

    /// The NEAR the deploy takes from the funder, gas not included (gas is well under 0.1 NEAR).
    pub fn total(&self) -> NearToken {
        self.global_burn.saturating_add(self.factory_funding)
    }

    /// One line per planned action and cost.
    pub fn describe(&self) -> Vec<String> {
        vec![
            format!("signer {} (funder)", self.funder),
            format!(
                "1. deploy the coin code ({} bytes) as a global contract by code hash {} (immutable); burns {}",
                self.coin_code.len(),
                self.coin_code_hash,
                self.global_burn.exact_amount_display()
            ),
            format!(
                "2. one transaction to {}: create the account, transfer {}, add the funder's public key as its one full-access key, deploy the factory code ({} bytes, code hash {}), call new with coin_code_hash {}",
                self.factory,
                self.factory_funding.exact_amount_display(),
                self.factory_code.len(),
                self.factory_code_hash,
                self.coin_code_hash
            ),
            format!(
                "total from the funder: {} plus gas (under 0.1 NEAR)",
                self.total().exact_amount_display()
            ),
        ]
    }
}

/// The transactions the deploy sent. `global_deploy_tx` is `None` when the coin code was already
/// in global storage under the same hash, so nothing was burnt again.
#[derive(Debug, Clone)]
pub struct DeployReport {
    pub global_deploy_tx: Option<CryptoHash>,
    pub factory_tx: CryptoHash,
}

/// Whether an account exists. Only the node's own answer that it does not exist counts as absent;
/// any other error is an unknown result.
async fn account_exists(network: &NetworkConfig, id: &AccountId) -> Result<bool> {
    match near_api::Account(id.clone())
        .view()
        .fetch_from(network)
        .await
    {
        Ok(_) => Ok(true),
        Err(e) if format!("{e:?}").contains("UnknownAccount") => Ok(false),
        Err(e) => bail!("unknown result reading {id}: {e}; nothing was signed"),
    }
}

/// Whether the coin code is already in global storage under its hash. Only the node's answer
/// that no such code exists counts as absent.
async fn global_code_exists(network: &NetworkConfig, hash: CryptoHash) -> Result<bool> {
    match Contract::global_wasm()
        .by_hash(hash)
        .fetch_from(network)
        .await
    {
        Ok(_) => Ok(true),
        Err(e) if is_no_global_code(&format!("{e:?}"), hash) => Ok(false),
        Err(e) => bail!("unknown result reading global code {hash}: {e}; stop and report"),
    }
}

/// Whether an error is the node's answer that no global code exists under `hash`. near-api 0.8.7
/// does not parse that answer: it comes back as a transport error that holds the raw JSON, with
/// the error name `NO_GLOBAL_CONTRACT_CODE` and the hash (seen on near-sandbox 2.13.4). A parsed
/// `NoGlobalContractCode` counts too. Anything else is an unknown result.
pub fn is_no_global_code(error: &str, hash: CryptoHash) -> bool {
    let named = error.contains("NoGlobalContractCode")
        || error.contains(r#"\"name\":\"NO_GLOBAL_CONTRACT_CODE\""#)
        || error.contains(r#""name":"NO_GLOBAL_CONTRACT_CODE""#);
    named && error.contains(&hash.to_string())
}

/// Runs the plan. It refuses before signing when the factory account exists already (no
/// redeploy). After the global deploy it stops on any failure or unknown result and never
/// retries: the caller reports and asks.
pub async fn deploy(
    plan: &DeployPlan,
    network: &NetworkConfig,
    funder: &Account,
) -> Result<DeployReport> {
    if funder.id != plan.funder {
        bail!(
            "the signer is {}, the plan's funder is {}",
            funder.id,
            plan.funder
        );
    }
    if account_exists(network, &plan.factory).await? {
        bail!(
            "{} exists already; this step never redeploys. Nothing was signed",
            plan.factory
        );
    }

    let global_deploy_tx = if global_code_exists(network, plan.coin_code_hash).await? {
        println!(
            "deploy: the coin code {} is already in global storage; nothing burnt",
            plan.coin_code_hash
        );
        None
    } else {
        let result = Contract::deploy_global_contract_code(plan.coin_code.clone())
            .as_hash()
            .with_signer(funder.id.clone(), funder.signer.clone())
            .send_to(network)
            .await
            .context("unknown result of the global deploy; stop and report, do not retry")?;
        let hash = transaction_hash(&result)?;
        if !result.is_success() {
            bail!("the global deploy failed in tx {hash}: {result:?}");
        }
        println!(
            "deploy: global coin code {} in tx {hash}",
            plan.coin_code_hash
        );
        Some(hash)
    };

    let public_key = funder.signer.get_public_key().await?;
    let actions = vec![
        Action::CreateAccount(CreateAccountAction {}),
        Action::Transfer(TransferAction {
            deposit: plan.factory_funding,
        }),
        Action::AddKey(Box::new(AddKeyAction {
            public_key,
            access_key: AccessKey {
                nonce: 0.into(),
                permission: AccessKeyPermission::FullAccess,
            },
        })),
        Action::DeployContract(DeployContractAction {
            code: plan.factory_code.clone(),
        }),
        Action::FunctionCall(Box::new(FunctionCallAction {
            method_name: "new".to_string(),
            args: serde_json::to_vec(
                &json!({ "coin_code_hash": plan.coin_code_hash.to_string() }),
            )?,
            gas: NEW_GAS,
            deposit: NearToken::from_yoctonear(0),
        })),
    ];
    let result = near_api::Transaction::construct(funder.id.clone(), plan.factory.clone())
        .add_actions(actions)
        .with_signer(funder.signer.clone())
        .send_to(network)
        .await
        .context("unknown result of the factory transaction; stop and report, do not retry")?;
    let factory_tx = transaction_hash(&result)?;
    if !result.is_success() {
        bail!("the factory transaction {factory_tx} failed: {result:?}");
    }
    println!(
        "deploy: {} made, deployed and started in tx {factory_tx}",
        plan.factory
    );
    Ok(DeployReport {
        global_deploy_tx,
        factory_tx,
    })
}

/// Reads the chain after the deploy: the factory runs the planned code, it records the planned
/// coin code hash, and the coin code is in global storage under that hash.
pub async fn check_deployed(plan: &DeployPlan, network: &NetworkConfig) -> Result<Vec<String>> {
    let factory = near_api::Account(plan.factory.clone())
        .view()
        .fetch_from(network)
        .await?
        .data;
    if factory.contract_state != ContractState::LocalHash(plan.factory_code_hash) {
        bail!(
            "{} runs {:?}, not the factory code {}",
            plan.factory,
            factory.contract_state,
            plan.factory_code_hash
        );
    }
    let recorded = view_on(network, &plan.factory, "coin_code_hash", json!({})).await?;
    if recorded != json!(plan.coin_code_hash.to_string()) {
        bail!(
            "the factory records coin code hash {recorded}, not {}",
            plan.coin_code_hash
        );
    }
    if !global_code_exists(network, plan.coin_code_hash).await? {
        bail!("no global code under {}", plan.coin_code_hash);
    }
    Ok(vec![
        format!(
            "{} code hash {} (view_account)",
            plan.factory, plan.factory_code_hash
        ),
        format!(
            "{}.coin_code_hash() = {}",
            plan.factory, plan.coin_code_hash
        ),
        format!(
            "global code {} exists (view_global_contract_code)",
            plan.coin_code_hash
        ),
    ])
}
