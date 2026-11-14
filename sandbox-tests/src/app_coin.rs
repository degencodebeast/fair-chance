//! A run on a coin the app launches: the runner does not launch. It makes the buyer accounts,
//! signs every sniper buy and early sell, then waits for the coin to appear on chain and sends the
//! burst at once. The launch transaction and the creator come from the chain, never from the
//! runner, and the run refuses a coin its own funder launched or a coin with unknown code.

use std::future::Future;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use near_api::types::AccountId;

use crate::option_t::OptionTArguments;
use crate::rate_limit::{without_urls, Budget};

/// What `--coin` asks for: the coin account and its label, the first part of its name, which
/// names the runner's accounts (`<label>-buyer-01`, `<label>-later-buyer`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCoinRequest {
    pub coin: AccountId,
    pub label: String,
}

/// `None` without `--coin`. With it: the coin must be `<label>.<factory>`, and no flag of a run
/// that launches, or of a BAG step, may come with it.
pub fn app_coin_request(
    args: &OptionTArguments,
    factory: &AccountId,
) -> Result<Option<AppCoinRequest>> {
    let Some(coin) = args.coin.as_deref() else {
        return Ok(None);
    };
    for (flag, given) in [
        ("--ticker", args.ticker.is_some()),
        ("--name", args.name.is_some()),
        ("--description", args.description.is_some()),
        (
            "--launch-deposit-millinear",
            args.launch_deposit_millinear.is_some(),
        ),
        ("--sells-only", args.sells_only),
        ("--delete-buyers", args.delete_buyers),
        ("--continue", args.continue_sells),
        ("--return-to-funder", args.return_to_funder),
        ("--sell-plan", args.sell_plan.is_some()),
    ] {
        if given {
            bail!("{flag} does not go with --coin: the app launches that coin; nothing was run");
        }
    }
    let refuse = || {
        anyhow::anyhow!(
            "--coin takes a coin of the factory {factory} (<label>.{factory}), not {coin:?}; nothing was run"
        )
    };
    let parsed: AccountId = coin.parse().map_err(|_| refuse())?;
    let label = parsed
        .as_str()
        .strip_suffix(&format!(".{factory}"))
        .filter(|label| !label.is_empty() && !label.contains('.'))
        .ok_or_else(refuse)?
        .to_string();
    Ok(Some(AppCoinRequest {
        coin: parsed,
        label,
    }))
}

/// What the chain says about the coin when it appears: the code hash of the account, and from
/// `fc_rules` the code hash the coin recorded, its creator and its early window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCoinFacts {
    pub account_code_hash: String,
    pub rules_code_hash: String,
    pub creator: String,
    pub early_blocks: u64,
}

/// The checks before any buy is sent: the known code on the account and in `fc_rules`, the fixed
/// early window, and a creator that is neither the funder nor one of its sub-accounts (the runner
/// signs for those, so such a coin could be the runner's own launch).
pub fn check_app_coin(
    facts: &AppCoinFacts,
    funder: &AccountId,
    known_code_hash: &str,
    early_blocks: u64,
) -> Result<()> {
    for (what, hash) in [
        ("the coin account's code hash", &facts.account_code_hash),
        ("fc_rules.code_hash", &facts.rules_code_hash),
    ] {
        if hash != known_code_hash {
            bail!("{what} is {hash}, not the known {known_code_hash}; no buy was sent");
        }
    }
    if facts.early_blocks != early_blocks {
        bail!(
            "fc_rules.early_blocks is {}, not {early_blocks}; no buy was sent",
            facts.early_blocks
        );
    }
    let creator = facts.creator.as_str();
    if creator == funder.as_str() || creator.ends_with(&format!(".{funder}")) {
        bail!(
            "the coin's creator {creator} is the funder {funder} or one of its sub-accounts, so the runner could have launched it; the app must launch it from another account; no buy was sent"
        );
    }
    Ok(())
}

/// Checks the blocks from the coin's launch block to the read that found it against the plan of
/// [`crate::burst::check_burst_timing`]: the planned detect blocks plus the spare blocks of the
/// estimate. A later find means the burst would land later than estimated, so no buy is sent.
pub fn check_detection(
    timing: &crate::burst::BurstTiming,
    early_blocks: u64,
    lag_blocks: u64,
) -> Result<()> {
    let last = crate::burst::check_burst_timing(timing, early_blocks)?;
    let allowed = timing.launch_tail_blocks + early_blocks.saturating_sub(last);
    if lag_blocks > allowed {
        bail!(
            "the coin was found {lag_blocks} blocks after its launch block, more than the {allowed} the burst plan allows; no buy was sent"
        );
    }
    Ok(())
}

/// One read of the coin: not there yet, or there with what the read found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe<T> {
    Missing,
    Found(T),
}

/// `Some(retry_after)` for an HTTP 429 or 5xx (the RPC is busy or failed for a moment), `None`
/// for anything else.
fn busy_wait(error: &str) -> Option<Option<Duration>> {
    if let Some(wait) = crate::rate_limit::rate_limit_wait(error) {
        return Some(wait);
    }
    let after = error.split("status: 5").nth(1)?;
    let code = after.as_bytes();
    let three_digits = code.len() >= 2
        && code[..2].iter().all(u8::is_ascii_digit)
        && !code.get(2).is_some_and(u8::is_ascii_digit);
    three_digits.then_some(None)
}

/// Reads the coin with `probe` every `interval` until it is found, for `deadline` at most. A 429 or
/// a 5xx waits as `budget` allows (the shared rule: 2 s doubling, at most two waits in the run); a
/// 403, any other answer, or a busy answer past the budget stops it with no retry. The error
/// texts carry no URL.
pub async fn wait_for_app_coin<T, F, Fut>(
    mut probe: F,
    interval: Duration,
    budget: &Budget,
    deadline: Duration,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<Probe<T>, String>>,
{
    let started = Instant::now();
    loop {
        match probe().await {
            Ok(Probe::Found(found)) => return Ok(found),
            Ok(Probe::Missing) => {}
            Err(error) => {
                let error = without_urls(&error);
                let Some(retry_after) = busy_wait(&error) else {
                    bail!("unknown result of the read of the coin: {error}; stopped, no retry");
                };
                let Some(wait) = budget.next(retry_after) else {
                    bail!(
                        "the read of the coin got a 429 or 5xx past the run's wait limit; stopped: {error}"
                    );
                };
                println!(
                    "app-coin: the read of the coin got a 429 or 5xx; waiting {} s",
                    wait.as_secs_f64()
                );
                tokio::time::sleep(wait).await;
                continue;
            }
        }
        if started.elapsed() >= deadline {
            bail!(
                "the coin did not appear within {} s; stopped, no buy was sent",
                deadline.as_secs()
            );
        }
        tokio::time::sleep(interval).await;
    }
}
