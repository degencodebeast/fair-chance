//! NEP-145 `storage_unregister` and the Fair Chance rule on it (ADR 0001): a force-unregister is
//! refused while the account holds tokens or has used allowance in the current window, so a new
//! record never gives a fresh allowance.

use crate::allowance::unmarked_allowance;
use crate::coin_state::{At, CoinState, Holder};
use crate::refusals::{Refusal, RefusalCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unregistered {
    /// The account had no record: `storage_unregister` returns false.
    NotRegistered,
    /// The caller removes the record and sends `refund` yoctoNEAR (the storage deposit it paid)
    /// back to the account.
    Removed { refund: u128 },
}

/// The rule step for `storage_unregister(force)` on the account's record, if any.
pub fn unregister(
    state: &CoinState,
    holder: Option<&Holder>,
    force: bool,
    at: At,
) -> Result<Unregistered, Refusal> {
    let Some(holder) = holder else {
        return Ok(Unregistered::NotRegistered);
    };
    if holder.balance > 0 && !force {
        return Err(Refusal::new(RefusalCode::UnregisterWithoutForce));
    }
    let used = unmarked_allowance(state, Some(holder), at).used;
    if force && (holder.balance > 0 || used > 0) {
        return Err(Refusal::new(RefusalCode::UnregisterWithBalance));
    }
    Ok(Unregistered::Removed {
        refund: holder.storage_deposit,
    })
}
