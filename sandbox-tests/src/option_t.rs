//! Option T: one creator launches a coin, 20 buyer accounts buy in blocks 0 to N, then all of
//! them try to sell. Their sells share one early-supply allowance, and the real NEAR cap fills.
//!
//! The network is a parameter. Sandbox is the default. Testnet signs real transactions, so it
//! runs only when the builder sets [`TESTNET_YES_FLAG`] to `yes` for that run.
//!
//! The creator and the buyers are sub-accounts of a funder, and the funder's public key is their
//! one full-access key, so the funder's signer signs for all of them and no other key exists. The
//! separate delete step ([`delete_buyers`]) uses the same key to delete the buyers later.

use std::time::Instant;

use anyhow::{bail, Context, Result};
use near_api::types::transaction::result::TransactionResult;
use near_api::types::{AccountId, NearGas, NearToken};
use near_api::NetworkConfig;
use serde_json::json;
use tokio::task::JoinSet;

use crate::amount_text::{amount, near, tokens};
use crate::pass_list::PassList;
use crate::rate_limit::{with_rate_limit_backoff, Waits};
use crate::spend_guard::{check_funder, limits_from, SpendLimits, GAS_MARGIN};
use crate::{
    account_view, batch_once, events, launch_args, send_once, transact_once, transaction_hash,
    view_on, wait_for_final_height, Account, FunctionCall, World, LAUNCH_DEPOSIT,
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
    pub delete_buyers: bool,
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
            "--buy-millinear" => parsed.buy_millinear = number()?,
            "--buyer-extra-millinear" => parsed.buyer_extra_millinear = number()?,
            "--buyer-tgas" => parsed.buyer_tgas = number()?,
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
        }
    }

    fn coin(&self) -> Result<AccountId> {
        coin_account_id(&self.ticker, &self.factory)
    }

    /// NEAR the run moves from the funder into the new accounts: the creator's launch deposit and
    /// gas, and each buyer's buy and extra. Gas the funder burns itself is not in it.
    pub fn funding(&self) -> NearToken {
        funding(
            self.launch_deposit,
            self.buy_near,
            self.buyer_extra,
            self.buyers,
        )
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
    let keys =
        with_rate_limit_backoff(&format!("the key list of {id}"), Waits::for_reads(), || {
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
    let head = with_rate_limit_backoff("the head block", Waits::for_reads(), || {
        near_api::Chain::block_number()
            .at(near_api::Reference::Optimistic)
            .fetch_from(&setup.network)
    })
    .await?;
    wait_for_final_height(&setup.network, head).await?;
    let creator = accounts.remove(0);
    Ok((creator, accounts))
}

/// The delete step: deletes each buyer account that exists, with its NEAR going to the funder,
/// and returns the deleted accounts. It never touches the creator, the coin or the factory: it
/// builds only the buyer names from [`buyer_account_ids`]. Check [`choose_buyer_deletion`] first.
pub async fn delete_buyers(setup: &OptionTSetup) -> Result<Vec<AccountId>> {
    let mut deleted = Vec::new();
    for id in buyer_account_ids(&setup.ticker, &setup.funder.id, setup.buyers)? {
        match near_api::Account(id.clone())
            .view()
            .fetch_from(&setup.network)
            .await
        {
            Ok(_) => {}
            // Only the node's own answer that the account does not exist counts as absent.
            Err(e) if format!("{e:?}").contains("UnknownAccount") => {
                println!("option-t: {id} does not exist; nothing to delete");
                continue;
            }
            Err(e) => bail!("unknown result reading {id}: {e}; nothing more deleted"),
        }
        let result = near_api::Account(id.clone())
            .delete_account_with_beneficiary(setup.funder.id.clone())
            .with_signer(setup.funder.signer.clone())
            .send_to(&setup.network)
            .await
            .with_context(|| format!("unknown result deleting {id}; nothing more deleted"))?;
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

/// Option T, start to end. Prints one line per buy and per sell, adds one PASS line per check
/// to `list`, and returns the early-buy timing and the funder's cost.
pub async fn run(setup: &OptionTSetup, list: &mut PassList) -> Result<OptionTReport> {
    let say = |text: &str| println!("option-t: {text}");
    let coin = setup.coin()?;
    let funder_before = balance(&setup.network, &setup.funder.id).await?;

    say(&format!(
        "make the creator and {} buyer accounts under {}",
        setup.buyers, setup.funder.id
    ));
    let (creator, buyers) = make_accounts(setup).await?;

    if account_view(&setup.network, &coin).await?.is_some() {
        bail!("{coin} exists already; a rerun resumes only before the launch. Nothing more was signed");
    }
    say(&format!("{} launches {coin}", creator.id));
    check_funder(
        setup.limits.as_ref(),
        &setup.network,
        &setup.funder.id,
        NearToken::from_yoctonear(0),
        "the launch",
    )
    .await?;
    let launched = transact_once(
        &setup.network,
        &setup.send_network,
        &creator,
        &setup.factory,
        "launch",
        launch_args(
            &setup.name,
            &setup.ticker,
            None,
            Some(&setup.description),
            None,
        ),
        setup.launch_deposit,
        near_api::types::NearGas::from_tgas(300),
    )
    .await?;
    if events(&launched, "fair_chance_factory", "fc_launch").len() != 1 {
        bail!("the launch logged no fc_launch: {launched:?}");
    }
    say(&format!("launch tx {}", transaction_hash(&launched)?));
    let storage = view_on(&setup.network, &coin, "storage_balance_bounds", json!({})).await?;
    let storage_min: u128 = storage["min"].as_str().context("storage min")?.parse()?;

    say(&format!(
        "{} buyers each pay storage and buy {} at once",
        setup.buyers,
        near(setup.buy_near.as_yoctonear())
    ));
    // Buyers sign their own buys from NEAR they already hold, so the funder balance cannot change
    // during the batch: one spend check before it covers every buy, with no reads in the batch.
    check_funder(
        setup.limits.as_ref(),
        &setup.network,
        &setup.funder.id,
        NearToken::from_yoctonear(0),
        &format!("the {} buys", setup.buyers),
    )
    .await?;
    let started = Instant::now();
    let mut tasks = JoinSet::new();
    for (index, buyer) in buyers.iter().cloned().enumerate() {
        let network = setup.network.clone();
        let send_network = setup.send_network.clone();
        let coin = coin.clone();
        let buy_near = setup.buy_near;
        let gas = setup.buyer_gas;
        tasks.spawn(async move {
            let calls = vec![
                FunctionCall::new(
                    "storage_deposit",
                    json!({}),
                    NearToken::from_yoctonear(storage_min),
                ),
                FunctionCall::new("buy", json!({ "min_tokens_out": "0" }), buy_near),
            ];
            (
                index,
                batch_once(&network, &send_network, &buyer, &coin, calls, gas).await,
            )
        });
    }
    let mut bought: Vec<Option<TransactionResult>> = vec![None; buyers.len()];
    while let Some(joined) = tasks.join_next().await {
        let (index, result) = joined?;
        bought[index] = Some(result?);
    }
    let seconds = started.elapsed().as_secs_f64();
    let bought: Vec<TransactionResult> = bought
        .into_iter()
        .map(|b| b.expect("every task reports"))
        .collect();

    let rules = view_on(&setup.network, &coin, "fc_rules", json!({})).await?;
    let launch_block = rules["launch_block"]
        .as_u64()
        .context("fc_rules.launch_block")?;
    let early_blocks = rules["early_blocks"]
        .as_u64()
        .context("fc_rules.early_blocks")?;

    let mut buy_blocks = Vec::new();
    let mut buy_events = Vec::new();
    for (buyer, result) in buyers.iter().zip(&bought) {
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
    for account in std::iter::once(&creator).chain(&buyers) {
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
