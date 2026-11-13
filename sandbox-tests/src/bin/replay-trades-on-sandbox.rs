//! Plays a list of trades against the coin on a fresh sandbox, after the early window, and writes
//! what each trade did. It is the sandbox side of the replay's equivalence test
//! (`analysis/smdd-replay/tests/test_limits_on_replay.py`): on a real chain the runner cannot pick a
//! trade's block, so it plays only trades after block N, where the block does not change a fee.
//!
//! replay-trades-on-sandbox play <plan.json> <out.json>
//!
//! The plan is the in-process runner's plan (`contracts/coin/examples/replay-trades-in-process.rs`):
//! `{"trades": [{"account", "side", "buy_sol_lamports", "buy_venue_sol_depth" (buy),
//! "sell_numerator", "sell_denominator" (sell)}]}`; block and time fields are not used. A buy pays
//! floor(pool NEAR depth x buy_sol_lamports / buy_venue_sol_depth), the depth read from `fc_pool`
//! just before it. A sell sells floor(balance x numerator / denominator) of the live balance, or
//! all of it when the numerator is not below the denominator. Each plan account is a new
//! sub-account of alice. The out file holds `fc_rules`, `total_supply`, `start_pool` (the pool
//! before the first trade) and one result per trade with the in-process runner's fields.
//! Needs NEAR_SANDBOX_BIN_PATH (near-sandbox 2.13.4).

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use fair_chance_sandbox_tests::{events, transact, Account, World};
use near_api::types::{AccountId, NearToken};
use near_api::{Contract, Reference};
use serde_json::{json, Value};

/// NEAR each trading account starts with: more than any fixture buy plus gas and storage.
const ACCOUNT_NEAR: u128 = 50;

fn amount(value: &Value, field: &str) -> Result<u128> {
    value[field]
        .as_str()
        .with_context(|| format!("{field} must be a decimal string"))?
        .parse()
        .with_context(|| format!("{field} must be a decimal string"))
}

/// floor(a x numerator / denominator), as the in-process runner computes it.
fn mul_div_floor(a: u128, numerator: u128, denominator: u128) -> u128 {
    (a / denominator) * numerator + (a % denominator) * numerator / denominator
}

/// The sell size: floor(balance x numerator / denominator), or the whole balance.
fn share_of(balance: u128, numerator: u128, denominator: u128) -> u128 {
    if numerator >= denominator {
        return balance;
    }
    mul_div_floor(balance, numerator, denominator)
}

/// A view at the optimistic head, so it sees the transaction that just ran.
async fn view(world: &World, method: &str, args: Value) -> Result<Value> {
    Ok(Contract(world.coin.clone())
        .call_function(method, args)
        .read_only::<Value>()
        .at(Reference::Optimistic)
        .fetch_from(&world.network)
        .await?
        .data)
}

async fn trading_account(world: &World, index: usize) -> Result<Account> {
    let id: AccountId = format!("trader{index}.{}", world.alice.id).parse()?;
    let made = near_api::Account::create_account(id.clone())
        .fund_myself(world.alice.id.clone(), NearToken::from_near(ACCOUNT_NEAR))
        .with_public_key(world.alice.signer.get_public_key().await?)
        .with_signer(world.alice.signer.clone())
        .send_to(&world.network)
        .await?;
    if !made.is_success() {
        bail!("creating {id} failed: {made:?}");
    }
    let account = Account {
        id,
        signer: world.alice.signer.clone(),
    };
    // register fails the run on a failed receipt; its result is not needed.
    let _registered = world.register(&account, &world.coin).await?;
    Ok(account)
}

async fn play(world: &World, who: &Account, trade: &Value) -> Result<Value> {
    let side = trade["side"].as_str().context("side")?;
    let mut result = json!({ "side": side });
    let (method, args, deposit) = if side == "buy" {
        let pool = view(world, "fc_pool", json!({})).await?;
        let near_in = mul_div_floor(
            amount(&pool, "near_reserve")?,
            amount(trade, "buy_sol_lamports")?,
            amount(trade, "buy_venue_sol_depth")?,
        );
        (
            "buy",
            json!({ "min_tokens_out": "0" }),
            NearToken::from_yoctonear(near_in),
        )
    } else {
        let balance: u128 = view(world, "ft_balance_of", json!({ "account_id": who.id }))
            .await?
            .as_str()
            .context("balance")?
            .parse()?;
        let tokens = share_of(
            balance,
            amount(trade, "sell_numerator")?,
            amount(trade, "sell_denominator")?,
        );
        result["tokens_asked"] = json!(tokens.to_string());
        (
            "sell",
            json!({ "amount": tokens.to_string(), "min_near_out": "0" }),
            NearToken::from_yoctonear(1),
        )
    };
    let done = transact(&world.network, who, &world.coin, method, args, deposit).await?;
    if !done.is_success() {
        result["refusal"] = json!(format!("{done:?}"));
        result["near"] = json!("0");
        result["tokens"] = json!("0");
    } else if side == "buy" {
        let buy = events(&done, "fair_chance", "fc_buy")
            .first()
            .context("fc_buy")?
            .event["data"][0]
            .clone();
        result["near"] = buy["near_in"].clone();
        result["tokens"] = buy["tokens_out"].clone();
        result["tokens_asked"] = buy["tokens_out"].clone();
        result["fee_near"] = buy["fee_near"].clone();
        result["class"] = buy["class_after"].clone();
    } else {
        let sell = events(&done, "fair_chance", "fc_sell")
            .first()
            .context("fc_sell")?
            .event["data"][0]
            .clone();
        result["near"] = sell["near_out"].clone();
        result["tokens"] = sell["tokens_sold"].clone();
        result["fee_near"] = sell["fee_near"].clone();
        result["class"] = sell["class"].clone();
        result["fill"] = sell["fill"].clone();
        result["limit_reason"] = sell["limit_reason"].clone();
    }
    let pool = view(world, "fc_pool", json!({})).await?;
    result["token_reserve"] = pool["token_reserve"].clone();
    result["near_reserve"] = pool["near_reserve"].clone();
    Ok(result)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 || args[1] != "play" {
        eprintln!("usage: replay-trades-on-sandbox play <plan.json> <out.json>");
        std::process::exit(2);
    }
    let plan: Value = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
    let trades = plan["trades"].as_array().context("trades")?;

    // World::launch waits until the head is past the coin's early window.
    let world = World::launch().await?;
    let mut accounts: BTreeMap<String, Account> = BTreeMap::new();
    for trade in trades {
        let name = trade["account"].as_str().context("account")?.to_string();
        if !accounts.contains_key(&name) {
            let account = trading_account(&world, accounts.len() + 1).await?;
            accounts.insert(name, account);
        }
    }
    let rules = view(&world, "fc_rules", json!({})).await?;
    let total_supply = view(&world, "ft_total_supply", json!({})).await?;
    let start_pool = view(&world, "fc_pool", json!({})).await?;
    let mut results = Vec::new();
    for trade in trades {
        let who = &accounts[trade["account"].as_str().context("account")?];
        results.push(play(&world, who, trade).await?);
    }
    let out = json!({
        "fc_rules": rules,
        "total_supply": total_supply,
        "start_pool": start_pool,
        "results": results,
    });
    std::fs::write(&args[3], serde_json::to_string(&out)?)?;
    Ok(())
}
