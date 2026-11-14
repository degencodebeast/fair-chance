//! The BAG sells-only step (builder decision 14 Nov 00:25 Lagos). After the launch and the 20 buys,
//! bag-buyer-01 to bag-buyer-18 each sell their full balance, one after another in that order,
//! with 1 yoctoNEAR and `min_near_out` 0. bag-buyer-19 and bag-buyer-20 never sell.
//!
//! Before any signature the step reads the 18 balances from the chain, writes the plan file (a
//! file that exists already stops it) and reads the file back. Before each sell it reads the
//! seller's balance again and refuses any account or amount that is not the next step of the plan
//! read back from the file, then
//! runs the spend check. Each sell goes out once ([`crate::transact_once`]). Every step line starts
//! with a UTC timestamp ([`crate::stamp`]).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use near_api::types::{AccountId, NearToken};
use serde_json::{json, Value};

use crate::option_t::{buyer_account_ids, coin_account_id, OptionTArguments, OptionTSetup};
use crate::pass_list::PassList;
use crate::spend_guard::check_funder;
use crate::{account_view, events, stamp, transact_once, transaction_hash, view_on, Account};

/// How many buyers sell: bag-buyer-01 to bag-buyer-18.
pub const SELLERS: usize = 18;
/// How many buyers there are; the last two keep their tokens.
const BUYERS: usize = 20;

/// The one ticker, factory and coin the step runs on. Any other is refused before any read.
pub const SELLS_ONLY_TICKER: &str = "BAG";
pub const SELLS_ONLY_FACTORY: &str = "factory.fairchance.testnet";
pub const SELLS_ONLY_COIN: &str = "bag.factory.fairchance.testnet";

/// The fixed rule, written into the plan file.
pub const RULE: &str = "bag-buyer-01 to bag-buyer-18 each sell their full balance, read from the chain just before the run, one after another in that order, with 1 yoctoNEAR and min_near_out 0. bag-buyer-19 and bag-buyer-20 never sell.";

/// Time between the reads of the first pass, so it stays under a rate limit.
const READ_SPACING: std::time::Duration = std::time::Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SellStep {
    pub account: AccountId,
    pub amount: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SellPlan {
    pub coin: AccountId,
    pub steps: Vec<SellStep>,
    pub keepers: Vec<AccountId>,
}

/// What one sell did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SellOutcome {
    pub account: AccountId,
    pub tx_hash: String,
    pub tokens_asked: u128,
    pub tokens_sold: u128,
    pub near_out: u128,
    /// `full`, `partial` or `none` from fc_sell; `refused` when the coin refused the sell.
    pub fill: String,
    pub limit_reason: Option<String>,
    pub refusal_code: Option<String>,
    pub cap_filled: bool,
}

/// The sellers (buyer 01 to 18, in order) and the keepers (buyer 19 and 20).
pub fn fixed_accounts(
    ticker: &str,
    funder: &AccountId,
) -> Result<(Vec<AccountId>, Vec<AccountId>)> {
    let mut all = buyer_account_ids(ticker, funder, BUYERS)?;
    let keepers = all.split_off(SELLERS);
    Ok((all, keepers))
}

/// The plan from the balances read before the run. It refuses a list that is not exactly the 18
/// sellers in order, and a seller with no tokens.
pub fn plan_from_balances(
    coin: &AccountId,
    ticker: &str,
    funder: &AccountId,
    balances: &[(AccountId, u128)],
) -> Result<SellPlan> {
    let (sellers, keepers) = fixed_accounts(ticker, funder)?;
    let given: Vec<&AccountId> = balances.iter().map(|(a, _)| a).collect();
    if given != sellers.iter().collect::<Vec<_>>() {
        bail!(
            "the balances are not bag-buyer-01 to bag-buyer-18 in that order; nothing was signed"
        );
    }
    if let Some((a, _)) = balances.iter().find(|(_, amount)| *amount == 0) {
        bail!("{a} holds no tokens, so the fixed rule cannot run; nothing was signed");
    }
    Ok(SellPlan {
        coin: coin.clone(),
        steps: balances
            .iter()
            .map(|(account, amount)| SellStep {
                account: account.clone(),
                amount: *amount,
            })
            .collect(),
        keepers,
    })
}

/// The plan file: the coin, the rule, each sell with its order, account and amount, and the
/// accounts that keep their tokens.
pub fn plan_json(plan: &SellPlan) -> Value {
    json!({
        "coin": plan.coin,
        "rule": RULE,
        "sells": plan.steps.iter().enumerate().map(|(i, s)| json!({
            "order": i + 1,
            "account": s.account,
            "amount": s.amount.to_string(),
            "deposit_yocto": "1",
            "min_near_out": "0",
        })).collect::<Vec<_>>(),
        "keep": plan.keepers,
    })
}

/// Refuses a plan file that is not the plan.
pub fn check_plan_file(text: &str, plan: &SellPlan) -> Result<()> {
    let read: Value =
        serde_json::from_str(text).context("the plan file is not JSON; nothing was signed")?;
    if read != plan_json(plan) {
        bail!("the plan file is not the plan the run made; nothing was signed");
    }
    Ok(())
}

/// The plan in a plan file, as [`plan_json`] writes it. It refuses a file that is not JSON, an
/// amount that is not a whole number and a sell out of order.
pub fn plan_from_file(text: &str) -> Result<SellPlan> {
    let file: Value =
        serde_json::from_str(text).context("the plan file is not JSON; nothing was signed")?;
    let bad = || {
        anyhow::anyhow!("the plan file has a field that is missing or wrong; nothing was signed")
    };
    let account =
        |v: &Value| -> Result<AccountId> { v.as_str().ok_or_else(bad)?.parse().map_err(|_| bad()) };
    let coin = account(&file["coin"])?;
    let mut steps = Vec::new();
    for (i, sell) in file["sells"].as_array().ok_or_else(bad)?.iter().enumerate() {
        if sell["order"].as_u64() != Some(i as u64 + 1) {
            bail!(
                "sell {} of the plan file is out of order; nothing was signed",
                i + 1
            );
        }
        steps.push(SellStep {
            account: account(&sell["account"])?,
            amount: sell["amount"]
                .as_str()
                .ok_or_else(bad)?
                .parse()
                .map_err(|_| bad())?,
        });
    }
    let keepers = file["keep"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(account)
        .collect::<Result<_>>()?;
    Ok(SellPlan {
        coin,
        steps,
        keepers,
    })
}

/// Refuses a sell that is not the plan's step `index`: another account, another amount, or a
/// chain balance that is not the planned amount.
pub fn check_step(
    plan: &SellPlan,
    index: usize,
    account: &AccountId,
    amount: u128,
    on_chain: u128,
) -> Result<()> {
    let Some(step) = plan.steps.get(index) else {
        bail!(
            "sell {} is past the plan's {} sells; nothing more was signed",
            index + 1,
            plan.steps.len()
        );
    };
    if &step.account != account {
        bail!(
            "sell {} is planned for {}, not {account}; nothing more was signed",
            index + 1,
            step.account
        );
    }
    if amount != step.amount {
        bail!(
            "sell {} of {account} is planned for {}, not {amount}; nothing more was signed",
            index + 1,
            step.amount
        );
    }
    if on_chain != step.amount {
        bail!(
            "{account} holds {on_chain} now, not the planned {}; nothing more was signed",
            step.amount
        );
    }
    Ok(())
}

/// The plan file path when the arguments ask for the sells-only step. `--sells-only` needs
/// `--sell-plan`, `--network testnet` and `--ticker BAG` exactly, and does not mix with
/// `--delete-buyers`.
pub fn sells_only_request(args: &OptionTArguments) -> Result<Option<PathBuf>> {
    match (args.sells_only, &args.sell_plan) {
        (false, None) => Ok(None),
        (false, Some(_)) => bail!("--sell-plan needs --sells-only; nothing was run"),
        (true, None) => bail!("--sells-only needs --sell-plan <file>; nothing was run"),
        (true, Some(path)) => {
            if args.ticker.as_deref() != Some(SELLS_ONLY_TICKER) {
                bail!("--sells-only runs only with --ticker {SELLS_ONLY_TICKER}; nothing was run");
            }
            if args.network.as_deref() != Some("testnet") {
                bail!("--sells-only runs only with --network testnet; nothing was run");
            }
            if args.delete_buyers {
                bail!("--sells-only does not mix with --delete-buyers; nothing was run");
            }
            Ok(Some(PathBuf::from(path)))
        }
    }
}

/// What each seller keeps after the return step: 0.05 NEAR. It pays for the account's own storage
/// (about 0.002 NEAR for a plain account) and the fee of a later delete, with a wide margin.
pub const RETURN_KEEP_YOCTO: u128 = 50_000_000_000_000_000_000_000;
/// The storage price: 10^19 yoctoNEAR per byte.
const YOCTO_PER_STORAGE_BYTE: u128 = 10_000_000_000_000_000_000;

/// One seller as read just before the return step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SellerState {
    pub account: AccountId,
    /// BAG it holds now.
    pub bag: u128,
    /// NEAR it holds now (view_account amount), in yoctoNEAR.
    pub near: u128,
    pub storage_bytes: u64,
}

/// One transfer of the return step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnStep {
    pub account: AccountId,
    /// yoctoNEAR sent to the funder: `near_before` minus [`RETURN_KEEP_YOCTO`].
    pub amount: u128,
    /// The NEAR read before the step; the transfer goes out only while the account still holds it.
    pub near_before: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnPlan {
    pub steps: Vec<ReturnStep>,
    /// Sellers that hold the keep or less, with the reason: nothing to send.
    pub skipped: Vec<(AccountId, String)>,
}

/// The return step's transfers, from the sells plan (read back from its file) and the sellers'
/// state now. It refuses, before any signature, a list that is not bag-buyer-01 to -18 of the plan
/// in order (so buyers 19 and 20 are never in it), a seller that still holds its whole planned
/// BAG (its sell did not run), and a seller whose storage costs more than half the keep. A seller
/// with the keep or less is skipped. A partly filled sell counts as sold: the seller holds less
/// BAG than planned.
pub fn plan_returns(
    plan: &SellPlan,
    funder: &AccountId,
    states: &[SellerState],
) -> Result<ReturnPlan> {
    let (sellers, keepers) = fixed_accounts(SELLS_ONLY_TICKER, funder)?;
    let planned: Vec<&AccountId> = plan.steps.iter().map(|s| &s.account).collect();
    let given: Vec<&AccountId> = states.iter().map(|s| &s.account).collect();
    if planned != sellers.iter().collect::<Vec<_>>() || plan.keepers != keepers {
        bail!("the plan file is not bag-buyer-01 to -18 selling and 19 and 20 keeping; nothing was signed");
    }
    if given != planned {
        bail!("the accounts read are not bag-buyer-01 to -18 of the plan in order; nothing was signed");
    }
    let mut out = ReturnPlan {
        steps: Vec::new(),
        skipped: Vec::new(),
    };
    for (state, step) in states.iter().zip(&plan.steps) {
        let account = &state.account;
        if state.bag >= step.amount {
            bail!(
                "{account} holds {} BAG, not less than its planned sell of {}: its sell did not run; nothing was signed",
                state.bag,
                step.amount
            );
        }
        let storage = u128::from(state.storage_bytes) * YOCTO_PER_STORAGE_BYTE;
        if storage > RETURN_KEEP_YOCTO / 2 {
            bail!(
                "{account} uses {} bytes of storage, more than the keep covers with margin; nothing was signed",
                state.storage_bytes
            );
        }
        if state.near <= RETURN_KEEP_YOCTO {
            out.skipped.push((
                account.clone(),
                format!("holds {} yoctoNEAR, not more than the keep", state.near),
            ));
            continue;
        }
        out.steps.push(ReturnStep {
            account: account.clone(),
            amount: state.near - RETURN_KEEP_YOCTO,
            near_before: state.near,
        });
    }
    Ok(out)
}

/// Refuses a transfer when the seller no longer holds exactly the NEAR read before the step.
pub fn check_return_step(step: &ReturnStep, near_now: u128) -> Result<()> {
    if near_now != step.near_before {
        bail!(
            "{} holds {near_now} yoctoNEAR now, not the {} read before the step; nothing more was signed",
            step.account,
            step.near_before
        );
    }
    Ok(())
}

/// Whether the arguments ask for the return step. `--return-to-funder` runs only with the
/// sells-only step, so it has all of that step's conditions (testnet, `--ticker BAG`, a plan file).
pub fn return_request(args: &OptionTArguments) -> Result<bool> {
    if !args.return_to_funder {
        return Ok(false);
    }
    if !args.sells_only {
        bail!("--return-to-funder needs --sells-only; nothing was run");
    }
    sells_only_request(args)?;
    Ok(true)
}

/// The coin of the sells-only step, from the FC_TESTNET_FACTORY value. It refuses any factory that
/// is not exactly [`SELLS_ONLY_FACTORY`], before any read or signature.
pub fn check_sells_only_factory(factory: Option<&str>) -> Result<AccountId> {
    if factory != Some(SELLS_ONLY_FACTORY) {
        bail!("--sells-only runs only on the factory {SELLS_ONLY_FACTORY}; set FC_TESTNET_FACTORY to it; nothing was run");
    }
    let coin = coin_account_id(SELLS_ONLY_TICKER, &SELLS_ONLY_FACTORY.parse()?)?;
    if coin.as_str() != SELLS_ONLY_COIN {
        bail!("the coin is {coin}, not {SELLS_ONLY_COIN}; nothing was run");
    }
    Ok(coin)
}

fn say(text: &str) {
    println!("{} sells-only: {text}", stamp());
}

async fn token_balance(setup: &OptionTSetup, coin: &AccountId, who: &AccountId) -> Result<u128> {
    let value = view_on(
        &setup.network,
        coin,
        "ft_balance_of",
        json!({ "account_id": who }),
    )
    .await?;
    value
        .as_str()
        .context("ft_balance_of is not a string")?
        .parse()
        .context("ft_balance_of is not a number")
}

fn amount(value: &Value) -> Result<u128> {
    value
        .as_str()
        .context("an amount is not a string")?
        .parse()
        .context("an amount is not a number")
}

/// The first `FC_...` code in a failure text.
fn refusal_code(text: &str) -> Option<String> {
    let start = text.find("FC_")?;
    let code: String = text[start..]
        .chars()
        .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
        .collect();
    Some(code)
}

/// The sells-only step. It returns one outcome per sell, in order, and adds PASS lines to `list`.
pub async fn run_sells(
    setup: &OptionTSetup,
    plan_path: &Path,
    list: &mut PassList,
) -> Result<Vec<SellOutcome>> {
    let coin = coin_account_id(&setup.ticker, &setup.factory)?;
    say(&format!("coin {coin}; the rule: {RULE}"));
    if plan_path.exists() {
        bail!(
            "the plan file {} exists already; a sells-only run writes a new one; nothing was signed",
            plan_path.display()
        );
    }
    if account_view(&setup.network, &coin).await?.is_none() {
        bail!("{coin} does not exist; nothing was signed");
    }

    let (sellers, keepers) = fixed_accounts(&setup.ticker, &setup.funder.id)?;
    let mut balances = Vec::new();
    for (i, account) in sellers.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(READ_SPACING).await;
        }
        let held = token_balance(setup, &coin, account).await?;
        say(&format!(
            "read {account}: {held} tokens (planned sell {} of {SELLERS})",
            i + 1
        ));
        balances.push((account.clone(), held));
    }
    let mut kept = Vec::new();
    for account in &keepers {
        tokio::time::sleep(READ_SPACING).await;
        let held = token_balance(setup, &coin, account).await?;
        say(&format!(
            "read {account}: {held} tokens (keeps them, never sells)"
        ));
        kept.push(held);
    }
    let plan = plan_from_balances(&coin, &setup.ticker, &setup.funder.id, &balances)?;
    let text = serde_json::to_string_pretty(&plan_json(&plan))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(plan_path)
        .with_context(|| {
            format!(
                "writing the plan file {}; nothing was signed",
                plan_path.display()
            )
        })?;
    std::io::Write::write_all(&mut file, format!("{text}\n").as_bytes()).with_context(|| {
        format!(
            "writing the plan file {}; nothing was signed",
            plan_path.display()
        )
    })?;
    drop(file);
    let written = std::fs::read_to_string(plan_path)?;
    check_plan_file(&written, &plan)?;
    // Each send is checked against this copy, read back from the file, not against `plan`.
    let filed = plan_from_file(&written)?;
    if filed.coin != coin {
        bail!(
            "the plan file names {}, not {coin}; nothing was signed",
            filed.coin
        );
    }
    say(&format!(
        "plan file written before any signature: {}",
        plan_path.display()
    ));

    let mut outcomes = Vec::new();
    for (index, step) in plan.steps.iter().enumerate() {
        let n = index + 1;
        say(&format!(
            "sell {n} of {SELLERS}: chain check of {}",
            step.account
        ));
        let on_chain = token_balance(setup, &coin, &step.account).await?;
        check_step(&filed, index, &step.account, step.amount, on_chain)?;
        check_funder(
            setup.limits.as_ref(),
            &setup.network,
            &setup.funder.id,
            NearToken::from_yoctonear(0),
            &format!("sell {n} of {SELLERS}"),
        )
        .await?;
        say(&format!(
            "sell {n} of {SELLERS}: {} sends {} tokens",
            step.account, step.amount
        ));
        let seller = Account {
            id: step.account.clone(),
            signer: setup.funder.signer.clone(),
        };
        let result = transact_once(
            &setup.network,
            &setup.send_network,
            &seller,
            &coin,
            "sell",
            json!({ "amount": step.amount.to_string(), "min_near_out": "0" }),
            NearToken::from_yoctonear(1),
            setup.buyer_gas,
        )
        .await
        .with_context(|| {
            format!(
                "sell {n} of {SELLERS} by {}; nothing more was signed",
                step.account
            )
        })?;
        let tx_hash = transaction_hash(&result)?.to_string();
        let cap_filled = !events(&result, "fair_chance", "fc_cap_filled").is_empty();
        let outcome = match events(&result, "fair_chance", "fc_sell").pop() {
            Some(sell) => {
                let data = &sell.event["data"][0];
                SellOutcome {
                    account: step.account.clone(),
                    tx_hash,
                    tokens_asked: amount(&data["tokens_asked"])?,
                    tokens_sold: amount(&data["tokens_sold"])?,
                    near_out: amount(&data["near_out"])?,
                    fill: data["fill"].as_str().unwrap_or("?").to_string(),
                    limit_reason: data["limit_reason"].as_str().map(str::to_string),
                    refusal_code: None,
                    cap_filled,
                }
            }
            None if !result.is_success() => {
                let text = crate::rate_limit::without_urls(&format!("{result:?}"));
                let Some(code) = refusal_code(&text) else {
                    bail!("sell {n} of {SELLERS} by {} failed with no refusal code, tx {tx_hash}; nothing more was signed", step.account);
                };
                SellOutcome {
                    account: step.account.clone(),
                    tx_hash,
                    tokens_asked: step.amount,
                    tokens_sold: 0,
                    near_out: 0,
                    fill: "refused".to_string(),
                    limit_reason: None,
                    refusal_code: Some(code),
                    cap_filled: false,
                }
            }
            None => bail!(
                "sell {n} of {SELLERS} by {} succeeded but logged no fc_sell, tx {tx_hash}; nothing more was signed",
                step.account
            ),
        };
        say(&format!(
            "sell {n} of {SELLERS}: {} tx {}: asked {}, sold {}, near_out {} yocto, fill {}, limit_reason {}, refusal {}{}",
            outcome.account,
            outcome.tx_hash,
            outcome.tokens_asked,
            outcome.tokens_sold,
            outcome.near_out,
            outcome.fill,
            outcome.limit_reason.as_deref().unwrap_or("none"),
            outcome.refusal_code.as_deref().unwrap_or("none"),
            if outcome.cap_filled { ", fc_cap_filled" } else { "" }
        ));
        outcomes.push(outcome);
    }

    for (account, before) in keepers.iter().zip(&kept) {
        let now = token_balance(setup, &coin, account).await?;
        if now != *before {
            bail!("{account} held {before} before the sells and {now} after; it must not sell");
        }
        say(&format!("{account} still holds {now} tokens"));
    }
    let filled = outcomes.iter().filter(|o| o.tokens_sold > 0).count();
    list.check(
        "eighteen sells in order",
        Ok(format!(
            "{} sells by bag-buyer-01 to -18 in order, {filled} sold tokens; bag-buyer-19 and -20 kept theirs",
            outcomes.len()
        )),
    )?;
    Ok(outcomes)
}

/// What one transfer of the return step did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnOutcome {
    pub account: AccountId,
    pub tx_hash: String,
    /// yoctoNEAR sent to the funder.
    pub amount: u128,
}

async fn near_and_storage(setup: &OptionTSetup, who: &AccountId) -> Result<(u128, u64)> {
    let view = account_view(&setup.network, who)
        .await?
        .with_context(|| format!("{who} does not exist; nothing more was signed"))?;
    Ok((view.amount.as_yoctonear(), view.storage_usage))
}

/// The return step, after the sells. It reads the existing plan file back, reads the BAG and NEAR
/// of bag-buyer-01 to -18 and of the two keepers, plans every transfer with [`plan_returns`], and
/// only then signs. Before each transfer it reads the seller again ([`check_return_step`]) and runs
/// the spend check; each transfer goes out once ([`crate::send_once`]). At the end it checks that
/// buyers 19 and 20 hold the same NEAR and BAG as before. It returns one outcome per transfer.
pub async fn run_returns(
    setup: &OptionTSetup,
    plan_path: &Path,
    list: &mut PassList,
) -> Result<Vec<ReturnOutcome>> {
    let coin = coin_account_id(&setup.ticker, &setup.factory)?;
    say(&format!(
        "return step on {coin}: bag-buyer-01 to -18 send their NEAR minus {RETURN_KEEP_YOCTO} yoctoNEAR to {}",
        setup.funder.id
    ));
    let text = std::fs::read_to_string(plan_path).with_context(|| {
        format!(
            "the return step reads the plan file {} of the sells; nothing was signed",
            plan_path.display()
        )
    })?;
    let plan = plan_from_file(&text)?;
    if plan.coin != coin {
        bail!(
            "the plan file names {}, not {coin}; nothing was signed",
            plan.coin
        );
    }
    if account_view(&setup.network, &coin).await?.is_none() {
        bail!("{coin} does not exist; nothing was signed");
    }

    let mut states = Vec::new();
    for (i, step) in plan.steps.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(READ_SPACING).await;
        }
        let bag = token_balance(setup, &coin, &step.account).await?;
        let (near, storage_bytes) = near_and_storage(setup, &step.account).await?;
        say(&format!(
            "read {}: {bag} BAG (planned sell {}), {near} yoctoNEAR, {storage_bytes} bytes",
            step.account, step.amount
        ));
        states.push(SellerState {
            account: step.account.clone(),
            bag,
            near,
            storage_bytes,
        });
    }
    let mut kept = Vec::new();
    for account in &plan.keepers {
        tokio::time::sleep(READ_SPACING).await;
        let held = (
            near_and_storage(setup, account).await?.0,
            token_balance(setup, &coin, account).await?,
        );
        say(&format!(
            "read {account}: {} yoctoNEAR, {} BAG (never touched)",
            held.0, held.1
        ));
        kept.push(held);
    }
    let returns = plan_returns(&plan, &setup.funder.id, &states)?;
    for (account, why) in &returns.skipped {
        say(&format!("{account}: skipped, {why}"));
    }
    let total: u128 = returns.steps.iter().map(|s| s.amount).sum();
    say(&format!(
        "plan: {} transfers, {total} yoctoNEAR in all, before any signature",
        returns.steps.len()
    ));

    let count = returns.steps.len();
    let mut outcomes = Vec::new();
    for (index, step) in returns.steps.iter().enumerate() {
        let n = index + 1;
        let (near_now, _) = near_and_storage(setup, &step.account).await?;
        check_return_step(step, near_now)?;
        check_funder(
            setup.limits.as_ref(),
            &setup.network,
            &setup.funder.id,
            NearToken::from_yoctonear(0),
            &format!("return {n} of {count}"),
        )
        .await?;
        say(&format!(
            "return {n} of {count}: {} sends {} yoctoNEAR",
            step.account, step.amount
        ));
        let transfer = near_api::Tokens::account(step.account.clone())
            .send_to(setup.funder.id.clone())
            .near(NearToken::from_yoctonear(step.amount))
            .with_signer(setup.funder.signer.clone());
        let result = crate::send_once(transfer, &step.account, &setup.network, &setup.send_network)
            .await
            .with_context(|| {
                format!(
                    "return {n} of {count} by {}; nothing more was signed",
                    step.account
                )
            })?;
        let tx_hash = transaction_hash(&result)?.to_string();
        if !result.is_success() {
            bail!(
                "return {n} of {count} by {} failed, tx {tx_hash}; nothing more was signed",
                step.account
            );
        }
        say(&format!(
            "return {n} of {count}: {} tx {tx_hash}",
            step.account
        ));
        outcomes.push(ReturnOutcome {
            account: step.account.clone(),
            tx_hash,
            amount: step.amount,
        });
    }

    for (account, before) in plan.keepers.iter().zip(&kept) {
        let now = (
            near_and_storage(setup, account).await?.0,
            token_balance(setup, &coin, account).await?,
        );
        if now != *before {
            bail!("{account} held {before:?} (NEAR, BAG) before the return step and {now:?} after; it must not be touched");
        }
        say(&format!(
            "{account} still holds {} yoctoNEAR and {} BAG",
            now.0, now.1
        ));
    }
    let sent: u128 = outcomes.iter().map(|o| o.amount).sum();
    list.check(
        "returns to the funder",
        Ok(format!(
            "{} transfers to {}, {sent} yoctoNEAR; {} skipped; bag-buyer-19 and -20 untouched",
            outcomes.len(),
            setup.funder.id,
            returns.skipped.len()
        )),
    )?;
    Ok(outcomes)
}
