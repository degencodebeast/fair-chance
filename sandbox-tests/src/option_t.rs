//! Option T: one creator launches a coin, 20 buyer accounts buy in blocks 0 to N, then all of
//! them try to sell. Their sells share one early-supply allowance, and the real NEAR cap fills.
//! Each buy, and one early sell per buyer, is signed before the launch and sent the moment the
//! launch result is back ([`crate::burst`]); with a later buyer set, one ordinary account buys
//! after the early window and sells every token after the dump.
//!
//! The network is a parameter. Sandbox is the default. Testnet signs real transactions, so it
//! runs only when the builder sets [`TESTNET_YES_FLAG`] to `yes` for that run.
//!
//! The creator and the buyers are sub-accounts of a funder, and the funder's public key is their
//! one full-access key, so the funder's signer signs for all of them and no other key exists. The
//! separate delete step ([`delete_buyers`]) uses the same key to delete the buyers later.

use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use near_api::types::transaction::result::TransactionResult;
use near_api::types::{AccountId, NearGas, NearToken};
use near_api::NetworkConfig;
use serde_json::json;
use tokio::task::JoinSet;

use crate::amount_text::{amount, near, tokens};
use crate::burst::{
    check_burst_timing, check_launch_answer, send_burst, BurstSettings, BurstTiming, SniperTxs,
};
use crate::pass_list::PassList;
use crate::rate_limit::{run_budget, with_rate_limit_backoff};
use crate::spend_guard::{check_funder, limits_from, SpendLimits, GAS_MARGIN};
use crate::{
    account_view, batch_once, events, function_call_actions, launch_args, send_once, transact_once,
    transaction_hash, view_on, wait_for_final_height, Account, FunctionCall, World, LAUNCH_DEPOSIT,
};

/// The environment variable the builder sets to `yes` to let Option T sign on testnet.
pub const TESTNET_YES_FLAG: &str = "FC_BUILDER_SAID_YES_TO_TESTNET_SIGNING";
/// The environment variable the builder sets to `yes` to let the delete step remove the testnet
/// buyer accounts (after the evidence file and the video).
pub const DELETE_BUYERS_YES_FLAG: &str = "FC_BUILDER_SAID_YES_TO_DELETING_OPTION_T_BUYERS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoNetwork {
    Sandbox,
    Testnet,
}

/// The command-line arguments of Option T. `None` means the default.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct OptionTArguments {
    pub network: Option<String>,
    pub ticker: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub start_balance: Option<String>,
    pub spend_cap: Option<String>,
    pub keep: Option<String>,
    pub buy_millinear: Option<u64>,
    pub buyer_extra_millinear: Option<u64>,
    pub buyer_tgas: Option<u64>,
    pub launch_deposit_millinear: Option<u64>,
    /// Snipers: the buyer accounts that buy in the early window.
    pub buyers: Option<u64>,
    /// NEAR the one ordinary buyer pays after the early window, in milliNEAR.
    pub later_buy_millinear: Option<u64>,
    /// Sandbox only: milliseconds per block, close to testnet for a rehearsal.
    pub sandbox_block_ms: Option<u64>,
    /// Sandbox only: a wait before each burst send, standing in for the slow testnet RPC.
    pub sandbox_send_delay_ms: Option<u64>,
    pub delete_buyers: bool,
    /// The BAG sells-only step ([`crate::sells_only`]); needs `--sell-plan` and `--ticker`.
    pub sells_only: bool,
    pub sell_plan: Option<String>,
}

/// Parses the arguments after the program name. It refuses any argument it does not know, a flag
/// without its value, `--flag=value`, a number that does not parse and a repeated flag, so a typo
/// never falls back to the default run or skips a guard.
pub fn parse_arguments(args: &[String]) -> Result<OptionTArguments> {
    let mut parsed = OptionTArguments::default();
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        if flag == "--delete-buyers" {
            if parsed.delete_buyers {
                bail!("{flag} is given twice");
            }
            parsed.delete_buyers = true;
            continue;
        }
        if flag == "--sells-only" {
            if parsed.sells_only {
                bail!("{flag} is given twice");
            }
            parsed.sells_only = true;
            continue;
        }
        let taken = match flag.as_str() {
            "--network" => parsed.network.is_some(),
            "--ticker" => parsed.ticker.is_some(),
            "--name" => parsed.name.is_some(),
            "--description" => parsed.description.is_some(),
            "--start-balance" => parsed.start_balance.is_some(),
            "--spend-cap" => parsed.spend_cap.is_some(),
            "--keep" => parsed.keep.is_some(),
            "--buy-millinear" => parsed.buy_millinear.is_some(),
            "--buyer-extra-millinear" => parsed.buyer_extra_millinear.is_some(),
            "--buyer-tgas" => parsed.buyer_tgas.is_some(),
            "--launch-deposit-millinear" => parsed.launch_deposit_millinear.is_some(),
            "--buyers" => parsed.buyers.is_some(),
            "--later-buy-millinear" => parsed.later_buy_millinear.is_some(),
            "--sandbox-block-ms" => parsed.sandbox_block_ms.is_some(),
            "--sandbox-send-delay-ms" => parsed.sandbox_send_delay_ms.is_some(),
            "--sell-plan" => parsed.sell_plan.is_some(),
            _ => bail!("unknown argument {flag}; nothing was run"),
        };
        if taken {
            bail!("{flag} is given twice");
        }
        let value = match args.next() {
            Some(value) if !value.starts_with("--") => value,
            _ => bail!("{flag} needs a value"),
        };
        let number = || {
            value
                .parse::<u64>()
                .map(Some)
                .with_context(|| format!("{flag} takes a whole number, not {value}"))
        };
        match flag.as_str() {
            "--network" => parsed.network = Some(value.clone()),
            "--ticker" => parsed.ticker = Some(value.clone()),
            "--name" => parsed.name = Some(value.clone()),
            "--description" => parsed.description = Some(value.clone()),
            "--start-balance" => parsed.start_balance = Some(value.clone()),
            "--spend-cap" => parsed.spend_cap = Some(value.clone()),
            "--keep" => parsed.keep = Some(value.clone()),
            "--sell-plan" => parsed.sell_plan = Some(value.clone()),
            "--buy-millinear" => parsed.buy_millinear = number()?,
            "--buyer-extra-millinear" => parsed.buyer_extra_millinear = number()?,
            "--buyer-tgas" => parsed.buyer_tgas = number()?,
            "--buyers" => parsed.buyers = number()?,
            "--later-buy-millinear" => parsed.later_buy_millinear = number()?,
            "--sandbox-block-ms" => parsed.sandbox_block_ms = number()?,
            "--sandbox-send-delay-ms" => parsed.sandbox_send_delay_ms = number()?,
            _ => parsed.launch_deposit_millinear = number()?,
        }
    }
    Ok(parsed)
}

/// The network from the `--network` argument (none means sandbox) and the value of
/// [`TESTNET_YES_FLAG`]. Testnet needs the flag to be exactly `yes`; any other network is refused.
pub fn choose_network(
    network: Option<&str>,
    testnet_yes_flag: Option<&str>,
) -> Result<DemoNetwork> {
    match network {
        None | Some("sandbox") => Ok(DemoNetwork::Sandbox),
        Some("testnet") if testnet_yes_flag == Some("yes") => Ok(DemoNetwork::Testnet),
        Some("testnet") => bail!(
            "Option T signs real transactions on testnet; it runs there only when the builder sets {TESTNET_YES_FLAG}=yes for that run"
        ),
        Some(other) => bail!("unknown network {other:?}: use sandbox or testnet"),
    }
}

/// The rehearsal flags change the sandbox only; on testnet they are refused, so a testnet run
/// never waits on purpose before a send.
pub fn check_sandbox_only_flags(args: &OptionTArguments, network: DemoNetwork) -> Result<()> {
    if network == DemoNetwork::Testnet {
        for (flag, given) in [
            ("--sandbox-block-ms", args.sandbox_block_ms.is_some()),
            (
                "--sandbox-send-delay-ms",
                args.sandbox_send_delay_ms.is_some(),
            ),
        ] {
            if given {
                bail!("{flag} is for a sandbox rehearsal only; nothing was run");
            }
        }
    }
    Ok(())
}

/// The spend limits of the run: testnet needs all three of `--start-balance`, `--spend-cap` and
/// `--keep`; sandbox takes them when given and runs without them otherwise.
pub fn choose_spend_limits(
    args: &OptionTArguments,
    network: DemoNetwork,
) -> Result<Option<SpendLimits>> {
    let limits = limits_from(
        args.start_balance.as_deref(),
        args.spend_cap.as_deref(),
        args.keep.as_deref(),
    )?;
    if network == DemoNetwork::Testnet && limits.is_none() {
        bail!(
            "Option T on testnet needs --start-balance, --spend-cap and --keep; nothing was signed"
        );
    }
    Ok(limits)
}

/// Whether the delete step may run: on testnet only with [`DELETE_BUYERS_YES_FLAG`] exactly
/// `yes`; on sandbox always (its accounts vanish with the sandbox).
pub fn choose_buyer_deletion(network: DemoNetwork, delete_yes_flag: Option<&str>) -> Result<()> {
    match network {
        DemoNetwork::Sandbox => Ok(()),
        DemoNetwork::Testnet if delete_yes_flag == Some("yes") => Ok(()),
        DemoNetwork::Testnet => bail!(
            "the delete step removes the testnet buyer accounts; it runs only when the builder sets {DELETE_BUYERS_YES_FLAG}=yes, after the evidence file and the video"
        ),
    }
}

/// What one Option T run needs. The same values work on sandbox and on testnet.
pub struct OptionTSetup {
    /// Reads: FastNEAR then the public RPC on testnet ([`crate::rpcs::testnet_rpcs`]).
    pub network: NetworkConfig,
    /// Signed transactions: one RPC with one try; each goes out once ([`crate::send_once`]).
    pub send_network: NetworkConfig,
    /// Pays for the creator and the buyers and signs for all of them; their accounts are its
    /// sub-accounts, so the buyers are siblings of the creator and not in its creator group.
    pub funder: Account,
    /// The factory that launches the coin.
    pub factory: AccountId,
    /// Ticker of the coin, kept as given for the symbol; the coin account is
    /// `<lowercase ticker>.<factory>`, as the factory makes it.
    pub ticker: String,
    /// Name and description of the coin (`ft_metadata.name`, `fc_rules.description`).
    pub name: String,
    pub description: String,
    pub buyers: usize,
    /// NEAR each buyer pays in its one buy.
    pub buy_near: NearToken,
    /// NEAR each buyer gets on top of its buy: the storage deposit and the prepaid gas, which the
    /// chain holds before each call and refunds when unused.
    pub buyer_extra: NearToken,
    /// Prepaid gas of each buyer transaction (storage deposit and buy; sell).
    pub buyer_gas: NearGas,
    /// NEAR the creator attaches to `launch` for the coin's storage.
    pub launch_deposit: NearToken,
    /// Checked against the funder balance before each step that signs; none on a plain sandbox run.
    pub limits: Option<SpendLimits>,
    /// NEAR the one ordinary buyer pays after the early window; zero means no later buyer.
    pub later_buy: NearToken,
    /// Sandbox rehearsal only: a wait before each burst send that stands in for the slow public
    /// testnet RPC. Zero on testnet.
    pub send_delay: Duration,
}

/// NEAR the creator gets on top of the launch deposit, for the gas of `launch` (300 TGas).
pub const CREATOR_EXTRA: NearToken = NearToken::from_millinear(500);
/// NEAR an Option T run moves from the funder into the new accounts: the creator's launch
/// deposit and [`CREATOR_EXTRA`], and each buyer's buy and extra.
pub fn funding(
    launch_deposit: NearToken,
    buy_near: NearToken,
    buyer_extra: NearToken,
    buyers: usize,
) -> NearToken {
    let buyer = buy_near.saturating_add(buyer_extra);
    launch_deposit
        .saturating_add(CREATOR_EXTRA)
        .saturating_add(NearToken::from_yoctonear(
            buyer.as_yoctonear() * buyers as u128,
        ))
}

/// The coin name and description when the run gets no `--name` or `--description`.
pub const DEFAULT_NAME: &str = "Option T";
pub const DEFAULT_DESCRIPTION: &str = "Option T demo coin.";

impl OptionTSetup {
    /// The default run on a sandbox World: funder bob, the World's factory, 1 NEAR buys.
    pub fn sandbox_defaults(world: &World, ticker: &str, buyers: usize) -> Self {
        Self {
            network: world.network.clone(),
            send_network: world.network.clone(),
            funder: world.bob.clone(),
            factory: world.factory.id.clone(),
            ticker: ticker.to_string(),
            name: DEFAULT_NAME.to_string(),
            description: DEFAULT_DESCRIPTION.to_string(),
            buyers,
            buy_near: NearToken::from_near(1),
            buyer_extra: NearToken::from_millinear(500),
            buyer_gas: NearGas::from_tgas(300),
            launch_deposit: LAUNCH_DEPOSIT,
            limits: None,
            later_buy: NearToken::from_yoctonear(0),
            send_delay: Duration::ZERO,
        }
    }

    fn coin(&self) -> Result<AccountId> {
        coin_account_id(&self.ticker, &self.factory)
    }

    /// NEAR the run moves from the funder into the new accounts: the creator's launch deposit and
    /// gas, and each buyer's buy and extra. Gas the funder burns itself is not in it.
    pub fn funding(&self) -> NearToken {
        let later = if self.later_buy.is_zero() {
            NearToken::from_yoctonear(0)
        } else {
            self.later_buy.saturating_add(self.buyer_extra)
        };
        funding(
            self.launch_deposit,
            self.buy_near,
            self.buyer_extra,
            self.buyers,
        )
        .saturating_add(later)
    }
}

/// The Option T report: early-buy timing and what the run cost the funder.
#[derive(Debug, Clone, Copy)]
pub struct OptionTReport {
    pub timing: EarlyBuyTiming,
    /// The funder's NEAR before the run minus after it: everything the run moved or burnt.
    pub funder_paid: NearToken,
    /// NEAR still held by the creator and buyer accounts at the end (the delete step returns the
    /// buyers' part to the funder).
    pub left_in_new_accounts: NearToken,
    /// The NEAR the coin locks for its storage (`storage_usage` x 10^19 yoctoNEAR per byte).
    pub coin_storage_lock: NearToken,
}

/// The timing of the early buys, for the testnet plan: the early window is blocks 0 to N.
#[derive(Debug, Clone, Copy)]
pub struct EarlyBuyTiming {
    pub seconds: f64,
    /// Lowest and highest block of the buys, counted from the launch block (block 0).
    pub first_block: u64,
    pub last_block: u64,
}

fn sub_account(name: &str, parent: &AccountId) -> Result<AccountId> {
    Ok(format!("{name}.{parent}").parse()?)
}

/// The coin account the factory makes: `<lowercase ticker>.<factory>`.
pub fn coin_account_id(ticker: &str, factory: &AccountId) -> Result<AccountId> {
    sub_account(&ticker.to_lowercase(), factory)
}

/// The creator account: `<lowercase ticker>-creator.<funder>`.
pub fn creator_account_id(ticker: &str, funder: &AccountId) -> Result<AccountId> {
    sub_account(&format!("{}-creator", ticker.to_lowercase()), funder)
}

/// The buyer accounts: `<lowercase ticker>-buyer-01.<funder>` to `...-buyer-<n>.<funder>`.
pub fn buyer_account_ids(ticker: &str, funder: &AccountId, n: usize) -> Result<Vec<AccountId>> {
    let ticker = ticker.to_lowercase();
    (1..=n)
        .map(|i| sub_account(&format!("{ticker}-buyer-{i:02}"), funder))
        .collect()
}

/// The one ordinary buyer that buys after the early window: `<lowercase ticker>-later-buyer.<funder>`.
pub fn later_buyer_account_id(ticker: &str, funder: &AccountId) -> Result<AccountId> {
    sub_account(&format!("{}-later-buyer", ticker.to_lowercase()), funder)
}

/// Loads the funder's signer from the system keychain (the near CLI keychain entry for the
/// account on `network`), then signs in memory, so it can sign for the funder's sub-accounts that
/// hold the funder's public key. No key file is read or written. Only the testnet path calls it.
/// Built only with the `testnet-keychain` feature.
#[cfg(feature = "testnet-keychain")]
pub async fn funder_signer_from_keychain(
    funder: &AccountId,
    network: &NetworkConfig,
) -> Result<std::sync::Arc<near_api::Signer>> {
    use near_api::signer::SignerTrait;
    let keychain =
        near_api::signer::keystore::KeystoreSigner::search_for_keys(funder.clone(), network)
            .await
            .with_context(|| format!("no full-access key of {funder} in the keychain"))?;
    let public_key = keychain.get_public_key()?;
    let secret = SignerTrait::get_secret_key(&keychain, funder, public_key).await?;
    Ok(near_api::Signer::from_secret_key(secret)?)
}

/// Makes `<id>` with the funder's public key as its one full-access key, funded with `amount`.
/// What a run does with one creator or buyer account it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountPlan {
    /// The account does not exist: create and fund it.
    Create,
    /// It exists with the funder's key and at least the planned NEAR: use it as it is.
    Reuse,
}

/// The facts about an existing account that decide whether a rerun may use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExistingAccount {
    pub balance: NearToken,
    /// The funder's public key is one of its keys, with full access.
    pub funder_key_full_access: bool,
}

/// Create a missing account; reuse an existing one only when it holds the funder's key with full
/// access and at least `planned`; stop otherwise. An existing account is never funded again.
pub fn decide_account(
    existing: Option<ExistingAccount>,
    planned: NearToken,
) -> Result<AccountPlan> {
    let Some(account) = existing else {
        return Ok(AccountPlan::Create);
    };
    if !account.funder_key_full_access {
        bail!("it exists without the funder's public key as a full-access key; stopped, nothing was signed");
    }
    if account.balance < planned {
        bail!(
            "it exists with {}, less than the planned {}; stopped, nothing was signed",
            account.balance.exact_amount_display(),
            planned.exact_amount_display()
        );
    }
    Ok(AccountPlan::Reuse)
}

/// The facts of an account, or `None` when the node answers that it does not exist. Reads back off
/// on a 429; any other error stops the run.
async fn existing_account(setup: &OptionTSetup, id: &AccountId) -> Result<Option<ExistingAccount>> {
    let Some(view) = account_view(&setup.network, id).await? else {
        return Ok(None);
    };
    let funder_key = setup.funder.signer.get_public_key().await?;
    let keys = with_rate_limit_backoff(&format!("the key list of {id}"), run_budget(), || {
        near_api::Account(id.clone())
            .list_keys()
            .fetch_from(&setup.network)
    })
    .await?;
    let funder_key_full_access = keys.data.iter().any(|(key, access)| {
        *key == funder_key && access.permission == near_api::types::AccessKeyPermission::FullAccess
    });
    Ok(Some(ExistingAccount {
        balance: view.amount,
        funder_key_full_access,
    }))
}

/// Creates and funds one account the survey planned to create, with a spend check first.
async fn create_sub_account(setup: &OptionTSetup, id: &AccountId, amount: NearToken) -> Result<()> {
    check_funder(
        setup.limits.as_ref(),
        &setup.network,
        &setup.funder.id,
        amount.saturating_add(GAS_MARGIN),
        &format!("funding {id}"),
    )
    .await?;
    let public_key = setup.funder.signer.get_public_key().await?;
    let transaction = near_api::Account::create_account(id.clone())
        .fund_myself(setup.funder.id.clone(), amount)
        .with_public_key(public_key)
        .with_signer(setup.funder.signer.clone());
    let made = send_once(
        transaction,
        &setup.funder.id,
        &setup.network,
        &setup.send_network,
    )
    .await?;
    if !made.is_success() {
        bail!("creating {id} failed: {made:?}");
    }
    println!(
        "option-t: created {id} with {}",
        amount.exact_amount_display()
    );
    Ok(())
}

/// One account the run needs: its name, the NEAR it is planned to hold, and what the survey found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurveyEntry {
    pub id: AccountId,
    pub planned: NearToken,
    pub existing: Option<ExistingAccount>,
}

/// Decides every account at once: create the missing ones, reuse the good existing ones. Any bad
/// existing account stops the whole run, with every bad account named, before anything is signed.
pub fn plan_accounts(entries: Vec<SurveyEntry>) -> Result<Vec<(SurveyEntry, AccountPlan)>> {
    let mut planned = Vec::new();
    let mut refused = Vec::new();
    for entry in entries {
        match decide_account(entry.existing, entry.planned) {
            Ok(plan) => planned.push((entry, plan)),
            Err(e) => refused.push(format!("{}: {e}", entry.id)),
        }
    }
    if !refused.is_empty() {
        bail!(
            "the account survey stopped the run before anything was signed: {}",
            refused.join("; ")
        );
    }
    Ok(planned)
}

/// Time between the survey's reads, so one pass over 21 accounts stays well under a rate limit.
const SURVEY_SPACING: std::time::Duration = std::time::Duration::from_millis(500);

/// Reads the creator and every buyer once, spaced, and prints one line per account.
async fn survey(setup: &OptionTSetup) -> Result<Vec<(SurveyEntry, AccountPlan)>> {
    let mut names = vec![(
        creator_account_id(&setup.ticker, &setup.funder.id)?,
        setup.launch_deposit.saturating_add(CREATOR_EXTRA),
    )];
    for id in buyer_account_ids(&setup.ticker, &setup.funder.id, setup.buyers)? {
        names.push((id, setup.buy_near.saturating_add(setup.buyer_extra)));
    }
    if !setup.later_buy.is_zero() {
        names.push((
            later_buyer_account_id(&setup.ticker, &setup.funder.id)?,
            setup.later_buy.saturating_add(setup.buyer_extra),
        ));
    }
    let mut entries = Vec::new();
    for (i, (id, planned)) in names.into_iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(SURVEY_SPACING).await;
        }
        let existing = existing_account(setup, &id).await?;
        match existing {
            None => println!(
                "option-t: survey {id}: missing; plan: create with {}",
                planned.exact_amount_display()
            ),
            Some(e) => println!(
                "option-t: survey {id}: exists with {}, funder full-access key {}; planned {}",
                e.balance.exact_amount_display(),
                if e.funder_key_full_access {
                    "yes"
                } else {
                    "no"
                },
                planned.exact_amount_display()
            ),
        }
        entries.push(SurveyEntry {
            id,
            planned,
            existing,
        });
    }
    plan_accounts(entries)
}

/// Makes the creator and the buyer accounts after one survey of all of them: a good existing
/// account is reused and never funded again, a missing one is created, and any bad one stops the
/// run before anything is signed. Then it waits until the accounts are final (the signer reads
/// each key at the final block).
pub async fn make_accounts(setup: &OptionTSetup) -> Result<(Account, Vec<Account>)> {
    let (creator, buyers, _later) = make_all_accounts(setup).await?;
    Ok((creator, buyers))
}

/// As [`make_accounts`], with the later buyer too when [`OptionTSetup::later_buy`] is not zero.
pub async fn make_all_accounts(
    setup: &OptionTSetup,
) -> Result<(Account, Vec<Account>, Option<Account>)> {
    let plan = survey(setup).await?;
    let mut accounts = Vec::new();
    for (entry, decision) in &plan {
        match decision {
            AccountPlan::Reuse => println!(
                "option-t: reused {}: it holds {}, planned {}, funder key with full access; not funded again",
                entry.id,
                entry.existing.map(|e| e.balance).unwrap_or_default().exact_amount_display(),
                entry.planned.exact_amount_display()
            ),
            AccountPlan::Create => create_sub_account(setup, &entry.id, entry.planned).await?,
        }
        accounts.push(Account {
            id: entry.id.clone(),
            signer: setup.funder.signer.clone(),
        });
    }
    let head = with_rate_limit_backoff("the head block", run_budget(), || {
        near_api::Chain::block_number()
            .at(near_api::Reference::Optimistic)
            .fetch_from(&setup.network)
    })
    .await?;
    wait_for_final_height(&setup.network, head).await?;
    let creator = accounts.remove(0);
    let later = if setup.later_buy.is_zero() {
        None
    } else {
        accounts.pop()
    };
    Ok((creator, accounts, later))
}

/// The delete step: deletes each buyer account that exists, and the later buyer, with its NEAR
/// going to the funder, and returns the deleted accounts. It never touches the creator, the coin
/// or the factory: it builds only the names from [`buyer_account_ids`] and
/// [`later_buyer_account_id`]. Check [`choose_buyer_deletion`] first.
pub async fn delete_buyers(setup: &OptionTSetup) -> Result<Vec<AccountId>> {
    let mut deleted = Vec::new();
    let mut ids = buyer_account_ids(&setup.ticker, &setup.funder.id, setup.buyers)?;
    ids.push(later_buyer_account_id(&setup.ticker, &setup.funder.id)?);
    for id in ids {
        // Only the node's own answer that the account does not exist counts as absent; any other
        // error stops the step.
        if account_view(&setup.network, &id)
            .await
            .with_context(|| format!("unknown result reading {id}; nothing more deleted"))?
            .is_none()
        {
            println!("option-t: {id} does not exist; nothing to delete");
            continue;
        }
        let transaction = near_api::Account(id.clone())
            .delete_account_with_beneficiary(setup.funder.id.clone())
            .with_signer(setup.funder.signer.clone());
        let result = send_once(
            transaction,
            &setup.funder.id,
            &setup.network,
            &setup.send_network,
        )
        .await
        .with_context(|| format!("deleting {id}; nothing more deleted"))?;
        if !result.is_success() {
            bail!("deleting {id} failed: {result:?}");
        }
        println!(
            "option-t: deleted {id}, its NEAR went to {}",
            setup.funder.id
        );
        deleted.push(id);
    }
    Ok(deleted)
}

async fn balance(network: &NetworkConfig, account: &AccountId) -> Result<NearToken> {
    Ok(account_view(network, account)
        .await?
        .with_context(|| format!("{account} does not exist"))?
        .amount)
}

/// NEAR each buyer attaches to `storage_deposit` in its pre-signed buy. The coin's
/// `storage_balance_bounds.min` is 0.0032 NEAR (read from the testnet BAG coin on 14 Nov; one
/// record has one fixed size); the coin refunds the rest. Fixed before the launch, so the buy can
/// be signed with no read of the new coin.
pub const BUY_STORAGE_DEPOSIT: NearToken = NearToken::from_millinear(5);
/// The tokens each sniper's pre-signed early sell asks for: 1 token, so the balance check passes
/// and the early-window rule is the one that refuses it.
pub const EARLY_SELL_TOKENS: u128 = 1_000_000_000_000_000_000;
/// Prepaid gas of each pre-signed early sell. The coin refuses it early, so it burns little; a
/// small value keeps the NEAR the chain holds for its gas low, after the buy took most of the
/// sniper's NEAR.
pub const EARLY_SELL_GAS: NearGas = NearGas::from_tgas(50);
/// Blocks from the coin's launch block to the launch transaction's last receipt (the factory's
/// callbacks), counted on the sandbox rehearsal; the burst guard uses it.
pub const LAUNCH_TAIL_BLOCKS: u64 = 3;
/// Time from sending the buys to sending the early sells: about two testnet blocks, so each buy is
/// in a block before the early sell with the next nonce of the same key reaches the node.
pub const EARLY_SELL_GAP: Duration = Duration::from_millis(1200);
/// Blocks back from the head over which the run measures seconds per block before the launch.
const BLOCK_TIME_SPAN: u64 = 100;

/// Seconds per block over the last [`BLOCK_TIME_SPAN`] final blocks. Two reads, before the launch.
pub async fn measured_block_s(network: &NetworkConfig) -> Result<f64> {
    let head = with_rate_limit_backoff("the final block", run_budget(), || {
        near_api::Chain::block()
            .at(near_api::Reference::Final)
            .fetch_from(network)
    })
    .await?;
    let from = head.header.height.saturating_sub(BLOCK_TIME_SPAN);
    let earlier = with_rate_limit_backoff("an earlier block", run_budget(), || {
        near_api::Chain::block()
            .at(near_api::Reference::AtBlock(from))
            .fetch_from(network)
    })
    .await?;
    let blocks = head.header.height - earlier.header.height;
    if blocks == 0 {
        bail!("no blocks to measure the block time; nothing was signed");
    }
    Ok((head.header.timestamp - earlier.header.timestamp) as f64 / 1e9 / blocks as f64)
}

/// Signs each sniper's buy (storage deposit and buy in one transaction) and its early sell (the
/// next nonce) before the launch, from one nonce read per sniper on the send RPC. Returns them
/// with the longest time one of those reads took: the send RPC's time per call.
pub async fn presign_snipers(
    setup: &OptionTSetup,
    coin: &AccountId,
    buyers: &[Account],
) -> Result<(Vec<SniperTxs>, f64)> {
    let public_key = setup.funder.signer.get_public_key().await?;
    let mut snipers = Vec::new();
    let mut send_s: f64 = 0.0;
    for (i, buyer) in buyers.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(SURVEY_SPACING).await;
        }
        let started = Instant::now();
        let (nonce, block_hash, _) =
            with_rate_limit_backoff(&format!("the nonce of {}", buyer.id), run_budget(), || {
                buyer
                    .signer
                    .fetch_tx_nonce(buyer.id.clone(), public_key, &setup.send_network)
            })
            .await?;
        send_s = send_s.max(started.elapsed().as_secs_f64());
        let calls = vec![
            FunctionCall::new("storage_deposit", json!({}), BUY_STORAGE_DEPOSIT),
            FunctionCall::new("buy", json!({ "min_tokens_out": "0" }), setup.buy_near),
        ];
        let buy = near_api::Transaction::construct(buyer.id.clone(), coin.clone())
            .add_actions(function_call_actions(calls, setup.buyer_gas)?)
            .with_signer(buyer.signer.clone())
            .presign_offline(public_key, block_hash, nonce)
            .await?;
        let early_sell = near_api::Contract(coin.clone())
            .call_function(
                "sell",
                json!({ "amount": EARLY_SELL_TOKENS.to_string(), "min_near_out": "0" }),
            )
            .transaction()
            .deposit(NearToken::from_yoctonear(1))
            .gas(EARLY_SELL_GAS)
            .with_signer(buyer.id.clone(), buyer.signer.clone())
            .presign_offline(public_key, block_hash, nonce + 1)
            .await?;
        snipers.push(SniperTxs {
            buyer: buyer.id.clone(),
            buy,
            early_sell,
        });
    }
    Ok((snipers, send_s))
}

/// Seconds one read of the funder's access key takes on the send RPC: the send RPC's time per
/// call, before any account is made.
async fn send_call_s(setup: &OptionTSetup) -> Result<f64> {
    let public_key = setup.funder.signer.get_public_key().await?;
    let started = Instant::now();
    with_rate_limit_backoff("the funder's access key", run_budget(), || {
        near_api::Account(setup.funder.id.clone())
            .access_key(public_key)
            .fetch_from(&setup.send_network)
    })
    .await?;
    Ok(started.elapsed().as_secs_f64())
}

/// The burst guard of [`check_burst_timing`] for this run, with `send_s` measured on the send RPC.
fn burst_guard(setup: &OptionTSetup, block_s: f64, send_s: f64) -> Result<(BurstTiming, u64)> {
    let timing = BurstTiming {
        block_s,
        send_s: send_s + setup.send_delay.as_secs_f64(),
        launch_tail_blocks: LAUNCH_TAIL_BLOCKS,
        early_sell_gap_s: EARLY_SELL_GAP.as_secs_f64(),
    };
    let last = check_burst_timing(&timing, rule_engine::FixedValues::fixed().early_blocks)?;
    Ok((timing, last))
}

/// The height of a block by its hash.
async fn height_of(network: &NetworkConfig, hash: near_api::types::CryptoHash) -> Result<u64> {
    Ok(
        with_rate_limit_backoff("a block by its hash", run_budget(), || {
            near_api::Chain::block()
                .at(near_api::Reference::AtBlockHash(hash))
                .fetch_from(network)
        })
        .await?
        .header
        .height,
    )
}

/// The block of the coin's receipt in a transaction, counted from the launch block, and the
/// transaction's refusal code when it failed.
async fn coin_receipt(
    network: &NetworkConfig,
    coin: &AccountId,
    launch_block: u64,
    result: &TransactionResult,
) -> Result<(u64, Option<String>)> {
    let TransactionResult::Full(full) = result else {
        bail!("the transaction has no execution result yet")
    };
    let outcome = full
        .receipt_outcomes()
        .iter()
        .find(|o| &o.executor_id == coin)
        .context("no receipt ran on the coin")?;
    let block = height_of(network, outcome.block_hash).await?;
    let code = (!result.is_success())
        .then(|| crate::sells_only::refusal_code(&format!("{result:?}")))
        .flatten();
    Ok((block.saturating_sub(launch_block), code))
}

/// Option T, start to end. Before the launch it signs every sniper buy and early sell and checks
/// that the burst fits the early window; after the launch result it sends them with no read in
/// between. Prints one line per buy and per sell, adds one PASS line per check to `list`, and
/// returns the early-buy timing and the funder's cost.
pub async fn run(setup: &OptionTSetup, list: &mut PassList) -> Result<OptionTReport> {
    let say = |text: &str| println!("option-t: {text}");
    let coin = setup.coin()?;
    let funder_before = balance(&setup.network, &setup.funder.id).await?;

    say(&format!(
        "make the creator, {} buyer accounts{} under {}",
        setup.buyers,
        if setup.later_buy.is_zero() {
            ""
        } else {
            " and one later buyer"
        },
        setup.funder.id
    ));
    // The guard first, before any account is made: the block time and one timed read on the
    // send RPC. It runs again on the nonce reads just before the launch.
    let block_s = measured_block_s(&setup.network).await?;
    let (_, first_estimate) = burst_guard(setup, block_s, send_call_s(setup).await?)?;
    say(&format!(
        "burst guard before any signature: {block_s:.4} s per block; the last early sell lands by block {first_estimate} (estimate)"
    ));
    let (creator, buyers, later) = make_all_accounts(setup).await?;

    if account_view(&setup.network, &coin).await?.is_some() {
        bail!("{coin} exists already; a rerun resumes only before the launch. Nothing more was signed");
    }
    check_funder(
        setup.limits.as_ref(),
        &setup.network,
        &setup.funder.id,
        NearToken::from_yoctonear(0),
        "the launch",
    )
    .await?;

    // Everything the burst needs, before the launch: the block time, one nonce per sniper and a
    // recent block hash, all buys and early sells signed, and the guard.
    let early_blocks = rule_engine::FixedValues::fixed().early_blocks;
    let (snipers, send_s) = presign_snipers(setup, &coin, &buyers).await?;
    let (burst_timing, estimate) = burst_guard(setup, block_s, send_s)?;
    say(&format!(
        "signed {} buys and {} early sells before the launch; {block_s:.4} s per block, {:.2} s per send call; the last early sell lands by block {estimate} of 0 to {early_blocks} (estimate)",
        snipers.len(),
        snipers.len(),
        burst_timing.send_s
    ));

    say(&format!("{} launches {coin}", creator.id));
    let launch = near_api::Contract(setup.factory.clone())
        .call_function(
            "launch",
            launch_args(
                &setup.name,
                &setup.ticker,
                None,
                Some(&setup.description),
                None,
            ),
        )
        .transaction()
        .deposit(setup.launch_deposit)
        .gas(NearGas::from_tgas(300))
        .with_signer(creator.id.clone(), creator.signer.clone())
        // Back as soon as every receipt ran, with no finality wait: the coin exists then.
        .wait_until(near_api::types::TxExecutionStatus::ExecutedOptimistic)
        // Signed now, so the timed send below holds no nonce read.
        .presign_with(&setup.send_network)
        .await?;
    let launch_sent = Instant::now();
    let launched = send_once(launch, &creator.id, &setup.network, &setup.send_network).await?;
    let launch_answer_s = launch_sent.elapsed().as_secs_f64();
    if !launched.is_success() || events(&launched, "fair_chance_factory", "fc_launch").len() != 1 {
        bail!("the launch logged no fc_launch; nothing more was signed: {launched:?}");
    }
    check_launch_answer(&burst_timing, early_blocks, launch_answer_s)?;
    let started = Instant::now();
    let burst = send_burst(
        &setup.network,
        &setup.send_network,
        snipers,
        BurstSettings {
            early_sell_gap: EARLY_SELL_GAP,
            send_delay: setup.send_delay,
        },
    )
    .await?;
    let seconds = started.elapsed().as_secs_f64();
    say(&format!(
        "launch tx {}; its answer took {launch_answer_s:.1} s",
        transaction_hash(&launched)?
    ));

    let rules = view_on(&setup.network, &coin, "fc_rules", json!({})).await?;
    let launch_block = rules["launch_block"]
        .as_u64()
        .context("fc_rules.launch_block")?;
    if rules["early_blocks"].as_u64() != Some(early_blocks) {
        bail!(
            "fc_rules.early_blocks is {}, not {early_blocks}",
            rules["early_blocks"]
        );
    }
    if let TransactionResult::Full(full) = &launched {
        let mut last = 0;
        for outcome in full.receipt_outcomes() {
            last = last.max(height_of(&setup.network, outcome.block_hash).await?);
        }
        say(&format!(
            "the launch's last receipt ran in block {} after the launch block {launch_block}",
            last.saturating_sub(launch_block)
        ));
    }

    let mut buy_blocks = Vec::new();
    let mut buy_events = Vec::new();
    for (buyer, result) in buyers.iter().zip(&burst.buys) {
        let buy = events(result, "fair_chance", "fc_buy")
            .pop()
            .with_context(|| format!("{} did not buy: {result:?}", buyer.id))?;
        let data = buy.event["data"][0].clone();
        let block = data["block"].as_u64().context("fc_buy.block")? - launch_block;
        println!(
            "option-t:   buy  {} block {block:>2}: {} tokens, {} marked, tx {}",
            buyer.id,
            tokens(amount(&data["tokens_out"])?),
            tokens(amount(&data["marked_tokens"])?),
            transaction_hash(result)?
        );
        buy_blocks.push(block);
        buy_events.push(data);
    }
    let timing = EarlyBuyTiming {
        seconds,
        first_block: *buy_blocks.iter().min().context("no buys")?,
        last_block: *buy_blocks.iter().max().context("no buys")?,
    };
    say(&format!(
        "the {} early buys took {:.1} s and landed in blocks {} to {} after the launch",
        setup.buyers, timing.seconds, timing.first_block, timing.last_block
    ));

    let mut early_sells = Vec::new();
    for (buyer, result) in buyers.iter().zip(&burst.early_sells) {
        let (block, code) = coin_receipt(&setup.network, &coin, launch_block, result).await?;
        println!(
            "option-t:   early sell {} block {block:>2}: {}, tx {}",
            buyer.id,
            code.as_deref().unwrap_or("not refused"),
            transaction_hash(result)?
        );
        early_sells.push((buyer.id.clone(), block, code));
    }

    list.check(
        &format!(
            "{} buyers bought in blocks 0 to {early_blocks}",
            number_word(setup.buyers)
        ),
        (|| {
            if timing.last_block > early_blocks {
                bail!(
                    "a buy landed in block {}, after the early window",
                    timing.last_block
                );
            }
            Ok(format!(
                "{} buys of {} landed in blocks {} to {} after the launch, {:.1} s from the launch result",
                setup.buyers, near(setup.buy_near.as_yoctonear()), timing.first_block, timing.last_block, timing.seconds
            ))
        })(),
    )?;

    list.check(
        "every early sell is refused with FC_EARLY_WINDOW",
        (|| {
            for (id, block, code) in &early_sells {
                if code.as_deref() != Some("FC_EARLY_WINDOW") {
                    bail!("the early sell of {id} in block {block} gave {code:?}");
                }
                if *block > early_blocks {
                    bail!("the early sell of {id} ran in block {block}, after the early window");
                }
            }
            let blocks: Vec<String> = early_sells.iter().map(|(_, b, _)| b.to_string()).collect();
            Ok(format!(
                "{} early sells of 1 token each were refused in blocks {} after the launch",
                early_sells.len(),
                blocks.join(", ")
            ))
        })(),
    )?;

    let early = view_on(&setup.network, &coin, "fc_early_window", json!({})).await?;
    list.check(
        "every buy is marked",
        (|| {
            let mut total = 0u128;
            for data in &buy_events {
                if data["marked_tokens"] != data["tokens_out"] {
                    bail!(
                        "{} has {} marked of {}",
                        data["account"],
                        data["marked_tokens"],
                        data["tokens_out"]
                    );
                }
                total += amount(&data["tokens_out"])?;
            }
            if amount(&early["early_supply_total"])? != total {
                bail!(
                    "early_supply_total {} is not the sum {total}",
                    early["early_supply_total"]
                );
            }
            Ok(format!(
                "all {} early-supply tokens of the {} buys are marked (fc_early_window.early_supply_total)",
                tokens(total),
                setup.buyers
            ))
        })(),
    )?;

    say("wait for the end of the early window");
    wait_for_final_height(&setup.network, launch_block + early_blocks + 1).await?;

    // The ordinary later buyer buys after the early window, so none of its tokens are marked.
    let mut later_bought = None;
    if let Some(later) = &later {
        check_funder(
            setup.limits.as_ref(),
            &setup.network,
            &setup.funder.id,
            NearToken::from_yoctonear(0),
            "the later buy",
        )
        .await?;
        let calls = vec![
            FunctionCall::new("storage_deposit", json!({}), BUY_STORAGE_DEPOSIT),
            FunctionCall::new("buy", json!({ "min_tokens_out": "0" }), setup.later_buy),
        ];
        let result = batch_once(
            &setup.network,
            &setup.send_network,
            later,
            &coin,
            calls,
            setup.buyer_gas,
        )
        .await?;
        let data = events(&result, "fair_chance", "fc_buy")
            .pop()
            .with_context(|| format!("{} did not buy: {result:?}", later.id))?
            .event["data"][0]
            .clone();
        println!(
            "option-t:   later buy {} block {}: {} tokens for {}, {} marked, tx {}",
            later.id,
            data["block"]
                .as_u64()
                .unwrap_or(0)
                .saturating_sub(launch_block),
            tokens(amount(&data["tokens_out"])?),
            near(setup.later_buy.as_yoctonear()),
            tokens(amount(&data["marked_tokens"])?),
            transaction_hash(&result)?
        );
        later_bought = Some(data);
    }

    say("every buyer tries to sell every token at once");
    check_funder(
        setup.limits.as_ref(),
        &setup.network,
        &setup.funder.id,
        NearToken::from_yoctonear(0),
        &format!("the {} sells", setup.buyers),
    )
    .await?;
    let mut tasks = JoinSet::new();
    for (index, (buyer, data)) in buyers.iter().cloned().zip(&buy_events).enumerate() {
        let network = setup.network.clone();
        let send_network = setup.send_network.clone();
        let coin = coin.clone();
        let tokens = data["tokens_out"]
            .as_str()
            .context("tokens_out")?
            .to_string();
        let gas = setup.buyer_gas;
        tasks.spawn(async move {
            let sold = transact_once(
                &network,
                &send_network,
                &buyer,
                &coin,
                "sell",
                json!({ "amount": tokens, "min_near_out": "0" }),
                NearToken::from_yoctonear(1),
                gas,
            )
            .await;
            (index, sold)
        });
    }
    let mut sold: Vec<Option<TransactionResult>> = vec![None; buyers.len()];
    while let Some(joined) = tasks.join_next().await {
        let (index, result) = joined?;
        sold[index] = Some(result?);
    }
    let mut sells = Vec::new();
    let mut cap_filled = Vec::new();
    for (buyer, result) in buyers.iter().zip(sold.iter().flatten()) {
        let sell = events(result, "fair_chance", "fc_sell")
            .pop()
            .with_context(|| format!("{} logged no fc_sell: {result:?}", buyer.id))?;
        let data = sell.event["data"][0].clone();
        println!(
            "option-t:   sell {} tx {} receipt {}: asked {}, sold {} (marked {}) for {}, fill {}, limit_reason {}",
            buyer.id,
            transaction_hash(result)?,
            sell.receipt_id,
            tokens(amount(&data["tokens_asked"])?),
            tokens(amount(&data["tokens_sold"])?),
            tokens(amount(&data["marked_sold"])?),
            near(amount(&data["near_out"])?),
            data["fill"].as_str().unwrap_or("?"),
            data["limit_reason"].as_str().unwrap_or("none"),
        );
        sells.push(data);
        cap_filled.extend(events(result, "fair_chance", "fc_cap_filled"));
    }

    // After the dump, the later buyer sells every token it bought.
    let mut later_sold = None;
    if let (Some(later), Some(bought)) = (&later, &later_bought) {
        let result = transact_once(
            &setup.network,
            &setup.send_network,
            later,
            &coin,
            "sell",
            json!({ "amount": bought["tokens_out"], "min_near_out": "0" }),
            NearToken::from_yoctonear(1),
            setup.buyer_gas,
        )
        .await?;
        let sell = events(&result, "fair_chance", "fc_sell")
            .pop()
            .with_context(|| format!("{} logged no fc_sell: {result:?}", later.id))?;
        let data = sell.event["data"][0].clone();
        println!(
            "option-t:   later sell {} tx {}: asked {}, sold {} (marked {}) for {}, class {}, fill {}, limit_reason {}",
            later.id,
            transaction_hash(&result)?,
            tokens(amount(&data["tokens_asked"])?),
            tokens(amount(&data["tokens_sold"])?),
            tokens(amount(&data["marked_sold"])?),
            near(amount(&data["near_out"])?),
            data["class"].as_str().unwrap_or("?"),
            data["fill"].as_str().unwrap_or("?"),
            data["limit_reason"].as_str().unwrap_or("none"),
        );
        later_sold = Some(data);
    }

    let early = view_on(&setup.network, &coin, "fc_early_window", json!({})).await?;
    list.check(
        "one shared allowance",
        (|| {
            let shared = amount(&early["shared_allowance"])?;
            let used = amount(&early["shared_allowance_used"])?;
            let mut marked_sold = 0u128;
            for data in &sells {
                marked_sold += amount(&data["marked_sold"])?;
            }
            if used != marked_sold {
                bail!("shared_allowance_used {used} is not the marked tokens sold {marked_sold}");
            }
            if used > shared {
                bail!("the sells used {used}, more than the shared allowance {shared}");
            }
            let held_back = sells.iter().filter(|s| s["fill"] != "full").count();
            Ok(format!(
                "{} sells took {} of one shared early-supply allowance of {} tokens (fc_early_window.shared_allowance_used = the marked tokens sold); {held_back} filled in part or not at all",
                sells.len(),
                tokens(used),
                tokens(shared)
            ))
        })(),
    )?;

    list.check(
        "most of the dump is held back",
        (|| {
            let (mut asked, mut sold) = (0u128, 0u128);
            for data in &sells {
                asked += amount(&data["tokens_asked"])?;
                sold += amount(&data["tokens_sold"])?;
            }
            if sold.saturating_mul(2) >= asked {
                bail!("the buyers sold {sold} of {asked} tokens, not less than half");
            }
            Ok(format!(
                "the {} buyers asked to sell {} tokens and sold {} ({:.1}%); {} stay in their wallets",
                sells.len(),
                tokens(asked),
                tokens(sold),
                sold as f64 * 100.0 / asked as f64,
                tokens(asked - sold)
            ))
        })(),
    )?;

    if let (Some(bought), Some(sold)) = (&later_bought, &later_sold) {
        list.check(
            "the later buyer sells all of its tokens",
            (|| {
                if amount(&bought["marked_tokens"])? != 0 {
                    bail!("the later buy marked {} tokens", bought["marked_tokens"]);
                }
                if sold["fill"] != "full" || sold["tokens_sold"] != bought["tokens_out"] {
                    bail!(
                        "the later sell sold {} of {}, fill {}",
                        sold["tokens_sold"],
                        bought["tokens_out"],
                        sold["fill"]
                    );
                }
                Ok(format!(
                    "{} bought after the window with none marked and sold all {} tokens for {} after the dump (class {}, fill full)",
                    sold["account"].as_str().unwrap_or("?"),
                    tokens(amount(&sold["tokens_sold"])?),
                    near(amount(&sold["near_out"])?),
                    sold["class"].as_str().unwrap_or("?")
                ))
            })(),
        )?;
    }

    list.check(
        "cap full",
        (|| {
            let filled = cap_filled.first().context("no sell logged fc_cap_filled")?;
            let cap = &filled.event["data"][0];
            let held = sells.iter().filter(|s| s["limit_reason"] == "cap_full").count();
            if held == 0 {
                bail!("no sell has limit_reason cap_full");
            }
            Ok(format!(
                "fc_cap_filled by {} in receipt {} with cap_used {} of cap_total {}; limit_reason per sell: {}",
                cap["by_account"].as_str().unwrap_or("?"),
                filled.receipt_id,
                near(amount(&cap["cap_used"])?),
                near(amount(&cap["cap_total"])?),
                reasons(&sells)
            ))
        })(),
    )?;
    let funder_after = balance(&setup.network, &setup.funder.id).await?;
    let mut left = NearToken::from_yoctonear(0);
    for account in std::iter::once(&creator)
        .chain(&buyers)
        .chain(later.as_ref())
    {
        left = left.saturating_add(balance(&setup.network, &account.id).await?);
    }
    let coin_state = account_view(&setup.network, &coin)
        .await?
        .with_context(|| format!("{coin} does not exist"))?;
    let report = OptionTReport {
        timing,
        funder_paid: funder_before.saturating_sub(funder_after),
        left_in_new_accounts: left,
        coin_storage_lock: NearToken::from_yoctonear(
            u128::from(coin_state.storage_usage) * 10_000_000_000_000_000_000,
        ),
    };
    say(&format!(
        "the funder paid {}; {} is still in the creator and buyer accounts; the coin locks {} for its storage",
        near(report.funder_paid.as_yoctonear()),
        near(report.left_in_new_accounts.as_yoctonear()),
        near(report.coin_storage_lock.as_yoctonear())
    ));
    Ok(report)
}

fn number_word(n: usize) -> String {
    match n {
        20 => "twenty".to_string(),
        other => other.to_string(),
    }
}

/// How many sells have each `limit_reason`, for example `allowance 15, cap_full 1, floor 4`.
fn reasons(sells: &[serde_json::Value]) -> String {
    let mut counts = std::collections::BTreeMap::new();
    for sell in sells {
        let reason = sell["limit_reason"].as_str().unwrap_or("none (full fill)");
        *counts.entry(reason.to_string()).or_insert(0) += 1;
    }
    counts
        .iter()
        .map(|(reason, n)| format!("{reason} {n}"))
        .collect::<Vec<_>>()
        .join(", ")
}
