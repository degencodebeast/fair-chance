//! NEP-145 `storage_unregister` and the Fair Chance rule on it (ADR 0001). The refusal follows
//! the account's state, with or without force:
//! - The account holds tokens: `FC_UNREGISTER_WITH_BALANCE`.
//! - No tokens, but it used allowance or has a real NEAR cap record in the current window:
//!   `FC_UNREGISTER_WINDOW_OPEN`. A new record would get a fresh allowance and a clean cap net
//!   in the same window. Unregister is allowed again once the window ends, so the storage
//!   deposit is never locked.

use crate::allowance::unmarked_allowance;
use crate::coin_state::{At, CoinState, Holder};
use crate::real_near_cap::cap_net_now;
use crate::refusals::{Refusal, RefusalCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unregistered {
    /// The account had no record: `storage_unregister` returns false.
    NotRegistered,
    /// The caller removes the record and sends `refund` yoctoNEAR (the storage deposit it paid)
    /// back to the account.
    Removed { refund: u128 },
}

/// The rule step for `storage_unregister(force)` on the account's record, if any. `force`
/// changes no outcome: the refusal codes follow the account's state.
pub fn unregister(
    state: &CoinState,
    holder: Option<&Holder>,
    force: bool,
    at: At,
) -> Result<Unregistered, Refusal> {
    let _ = force;
    let Some(holder) = holder else {
        return Ok(Unregistered::NotRegistered);
    };
    if holder.balance > 0 {
        return Err(Refusal::new(RefusalCode::UnregisterWithBalance));
    }
    let used = unmarked_allowance(state, Some(holder), at).used;
    if used > 0 || cap_net_now(state, Some(holder), at).is_open() {
        return Err(Refusal::new(RefusalCode::UnregisterWindowOpen));
    }
    Ok(Unregistered::Removed {
        refund: holder.storage_deposit,
    })
}
