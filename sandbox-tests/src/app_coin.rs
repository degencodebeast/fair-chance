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
