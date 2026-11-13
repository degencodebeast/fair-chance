//! The spend limits of a testnet run. Before each step that signs or spends, the run reads the
//! funder balance and refuses the step when the spend since the start balance plus the step's
//! cost would go above the cap, or when the balance after the step would fall below the amount to
//! keep. The start balance is the builder's reading before the first signing of the session, so
//! the deploy and Option T share one cap.

use anyhow::{bail, Result};
use near_api::types::{AccountId, NearToken};
use near_api::NetworkConfig;

/// NEAR set aside for the gas of one transaction the funder signs. Well above what any of them
/// burns (the global deploy burns about 0.01 NEAR of gas).
pub const GAS_MARGIN: NearToken = NearToken::from_millinear(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpendLimits {
    /// The funder balance before the first signing of the session.
    pub start: NearToken,
    /// The most the session may spend from `start`.
    pub cap: NearToken,
    /// The funder balance must stay at or above this after each step.
    pub keep: NearToken,
}

impl SpendLimits {
    /// Refuses `step` when `balance` (read now) minus `cost` breaks a limit. A balance above the
    /// start (a top-up) counts as nothing spent.
    pub fn check(&self, balance: NearToken, cost: NearToken, step: &str) -> Result<()> {
        let spent = self.start.saturating_sub(balance);
        if spent.saturating_add(cost) > self.cap {
            bail!(
                "refused before {step}: {} spent since the start balance {}, this step up to {}, cap {}; nothing more was signed",
                spent.exact_amount_display(),
                self.start.exact_amount_display(),
                cost.exact_amount_display(),
                self.cap.exact_amount_display()
            );
        }
        if balance < cost.saturating_add(self.keep) {
            bail!(
                "refused before {step}: the balance {} minus this step's {} would fall below {}; nothing more was signed",
                balance.exact_amount_display(),
                cost.exact_amount_display(),
                self.keep.exact_amount_display()
            );
        }
        Ok(())
    }
}

/// Reads the funder balance and checks `step` against `limits`; prints one line when it passes.
/// With no limits (a sandbox run) it reads nothing.
pub async fn check_funder(
    limits: Option<&SpendLimits>,
    network: &NetworkConfig,
    funder: &AccountId,
    cost: NearToken,
    step: &str,
) -> Result<()> {
    let Some(limits) = limits else {
        return Ok(());
    };
    let balance = crate::account_view(network, funder)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "reading the balance of {funder} before {step}: {e}; nothing more was signed"
            )
        })?
        .ok_or_else(|| anyhow::anyhow!("{funder} does not exist; nothing more was signed"))?
        .amount;
    limits.check(balance, cost, step)?;
    println!(
        "spend check before {step}: balance {}, spent {} of the cap {}, this step up to {}",
        balance.exact_amount_display(),
        limits.start.saturating_sub(balance).exact_amount_display(),
        limits.cap.exact_amount_display(),
        cost.exact_amount_display()
    );
    Ok(())
}

/// Parses a NEAR amount written as digits with an optional fraction of at most 24 digits, for
/// example `69.89859448325`. Anything else is refused.
pub fn parse_near(text: &str) -> Result<NearToken> {
    let (whole, fraction) = match text.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (text, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || fraction.is_some_and(|f| !digits(f) || f.len() > 24) {
        bail!("{text:?} is not a NEAR amount such as 69.89859448325");
    }
    let fraction = fraction.unwrap_or("");
    let yocto = format!("{whole}{fraction:0<24}")
        .parse::<u128>()
        .map_err(|_| anyhow::anyhow!("{text:?} is too large"))?;
    Ok(NearToken::from_yoctonear(yocto))
}

/// The limits from three optional values: all three or none.
pub fn limits_from(
    start: Option<&str>,
    cap: Option<&str>,
    keep: Option<&str>,
) -> Result<Option<SpendLimits>> {
    match (start, cap, keep) {
        (None, None, None) => Ok(None),
        (Some(start), Some(cap), Some(keep)) => Ok(Some(SpendLimits {
            start: parse_near(start)?,
            cap: parse_near(cap)?,
            keep: parse_near(keep)?,
        })),
        _ => bail!("give all three of --start-balance, --spend-cap and --keep, or none"),
    }
}
