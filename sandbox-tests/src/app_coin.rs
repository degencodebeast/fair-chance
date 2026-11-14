//! A run on a coin the app launches: the runner does not launch. It makes the buyer accounts,
//! signs every sniper buy and early sell, then waits for the coin to appear on chain and sends the
//! burst at once. The launch transaction and the creator come from the chain, never from the
//! runner, and the run refuses a coin its own funder launched or a coin with unknown code.

use anyhow::{bail, Result};
use near_api::types::AccountId;

use crate::option_t::OptionTArguments;

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
