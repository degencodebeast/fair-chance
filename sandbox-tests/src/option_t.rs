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
use crate::{
    batch_on_with_gas, events, launch_args, transact, transact_with_gas, view_on,
    wait_for_final_height, Account, FunctionCall, World, LAUNCH_DEPOSIT,
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
    pub network: NetworkConfig,
    /// Pays for the creator and the buyers and signs for all of them; their accounts are its
    /// sub-accounts, so the buyers are siblings of the creator and not in its creator group.
    pub funder: Account,
    /// The factory that launches the coin.
    pub factory: AccountId,
    /// Ticker of the coin; the coin account is `<ticker>.<factory>`.
    pub ticker: String,
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
}

/// NEAR the creator gets on top of the launch deposit, for the gas of `launch` (300 TGas).
const CREATOR_EXTRA: NearToken = NearToken::from_millinear(500);

impl OptionTSetup {
    /// The default run on a sandbox World: funder bob, the World's factory, 1 NEAR buys.
    pub fn sandbox_defaults(world: &World, ticker: &str, buyers: usize) -> Self {
        Self {
            network: world.network.clone(),
            funder: world.bob.clone(),
            factory: world.factory.id.clone(),
            ticker: ticker.to_string(),
            buyers,
            buy_near: NearToken::from_near(1),
            buyer_extra: NearToken::from_millinear(500),
            buyer_gas: NearGas::from_tgas(300),
            launch_deposit: LAUNCH_DEPOSIT,
        }
    }

    fn coin(&self) -> Result<AccountId> {
        sub_account(&self.ticker, &self.factory)
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

/// The creator account: `<ticker>-creator.<funder>`.
pub fn creator_account_id(ticker: &str, funder: &AccountId) -> Result<AccountId> {
    sub_account(&format!("{ticker}-creator"), funder)
}

/// The buyer accounts: `<ticker>-buyer-01.<funder>` to `<ticker>-buyer-<n>.<funder>`.
pub fn buyer_account_ids(ticker: &str, funder: &AccountId, n: usize) -> Result<Vec<AccountId>> {
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
async fn new_sub_account(
    setup: &OptionTSetup,
    id: AccountId,
    amount: NearToken,
) -> Result<Account> {
    let public_key = setup.funder.signer.get_public_key().await?;
    let made = near_api::Account::create_account(id.clone())
        .fund_myself(setup.funder.id.clone(), amount)
        .with_public_key(public_key)
        .with_signer(setup.funder.signer.clone())
        .send_to(&setup.network)
        .await?;
    if !made.is_success() {
        bail!("creating {id} failed: {made:?}");
    }
    Ok(Account {
        id,
        signer: setup.funder.signer.clone(),
    })
}

/// Makes the creator and the buyer accounts, and waits until they are final (the signer reads
/// each key at the final block).
pub async fn make_accounts(setup: &OptionTSetup) -> Result<(Account, Vec<Account>)> {
    let creator = new_sub_account(
        setup,
        creator_account_id(&setup.ticker, &setup.funder.id)?,
        setup.launch_deposit.saturating_add(CREATOR_EXTRA),
    )
    .await?;
    let mut buyers = Vec::new();
    for id in buyer_account_ids(&setup.ticker, &setup.funder.id, setup.buyers)? {
        buyers.push(
            new_sub_account(setup, id, setup.buy_near.saturating_add(setup.buyer_extra)).await?,
        );
    }
    let head = near_api::Chain::block_number()
        .at(near_api::Reference::Optimistic)
        .fetch_from(&setup.network)
        .await?;
    wait_for_final_height(&setup.network, head).await?;
    Ok((creator, buyers))
}

/// The delete step: deletes each buyer account that exists, with its NEAR going to the funder,
/// and returns the deleted accounts. It never touches the creator, the coin or the factory: it
/// builds only the buyer names from [`buyer_account_ids`]. Check [`choose_buyer_deletion`] first.
pub async fn delete_buyers(setup: &OptionTSetup) -> Result<Vec<AccountId>> {
    let mut deleted = Vec::new();
    for id in buyer_account_ids(&setup.ticker, &setup.funder.id, setup.buyers)? {
        let exists = near_api::Account(id.clone())
            .view()
            .fetch_from(&setup.network)
            .await
            .is_ok();
        if !exists {
            println!("option-t: {id} does not exist; nothing to delete");
            continue;
        }
        let result = near_api::Account(id.clone())
            .delete_account_with_beneficiary(setup.funder.id.clone())
            .with_signer(setup.funder.signer.clone())
            .send_to(&setup.network)
            .await?;
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
    Ok(near_api::Account(account.clone())
        .view()
        .fetch_from(network)
        .await?
        .data
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

    say(&format!("{} launches {coin}", creator.id));
    let launched = transact(
        &setup.network,
        &creator,
        &setup.factory,
        "launch",
        launch_args(
            "Option T",
            &setup.ticker,
            None,
            Some("Option T demo coin."),
            None,
        ),
        setup.launch_deposit,
    )
    .await?;
    if events(&launched, "fair_chance_factory", "fc_launch").len() != 1 {
        bail!("the launch logged no fc_launch: {launched:?}");
    }
    let storage = view_on(&setup.network, &coin, "storage_balance_bounds", json!({})).await?;
    let storage_min: u128 = storage["min"].as_str().context("storage min")?.parse()?;

    say(&format!(
        "{} buyers each pay storage and buy {} at once",
        setup.buyers,
        near(setup.buy_near.as_yoctonear())
    ));
    let started = Instant::now();
    let mut tasks = JoinSet::new();
    for (index, buyer) in buyers.iter().cloned().enumerate() {
        let network = setup.network.clone();
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
                batch_on_with_gas(&network, &buyer, &coin, calls, gas).await,
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
            "option-t:   buy  {} block {block:>2}: {} tokens, {} marked",
            buyer.id,
            tokens(amount(&data["tokens_out"])?),
            tokens(amount(&data["marked_tokens"])?)
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
    let mut tasks = JoinSet::new();
    for (index, (buyer, data)) in buyers.iter().cloned().zip(&buy_events).enumerate() {
        let network = setup.network.clone();
        let coin = coin.clone();
        let tokens = data["tokens_out"]
            .as_str()
            .context("tokens_out")?
            .to_string();
        let gas = setup.buyer_gas;
        tasks.spawn(async move {
            let sold = transact_with_gas(
                &network,
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
            "option-t:   sell {} receipt {}: asked {}, sold {} (marked {}) for {}, fill {}, limit_reason {}",
            buyer.id,
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
    let coin_state = near_api::Account(coin.clone())
        .view()
        .fetch_from(&setup.network)
        .await?
        .data;
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
