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
pub(crate) fn refusal_code(text: &str) -> Option<String> {
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
