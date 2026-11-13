//! The creator group (ADR 0001): the creator account, every account whose name ends with `.`
//! plus the creator's name, and every parent account of the creator up to but not including the
//! top-level account. The test runs on the dot boundary, so `malice.near` is not in the group of
//! `alice.near`. Its tokens are always marked, and it may hold at most `creator_cap_bps` of
//! supply on buys and receives.

use crate::coin_state::CoinState;
use crate::wide_integer_math::mul_div;

/// True when `account` is in the creator group of `creator`.
pub fn is_in_group(creator: &str, account: &str) -> bool {
    if account == creator {
        return true;
    }
    // A sub-account of the creator, at any depth.
    if let Some(prefix) = account.strip_suffix(creator) {
        if prefix.ends_with('.') && prefix.len() > 1 {
            return true;
        }
    }
    // A parent of the creator below the top-level account: the creator ends with `.` plus the
    // account, and the account itself has a dot.
    if let Some(prefix) = creator.strip_suffix(account) {
        if prefix.ends_with('.') && prefix.len() > 1 && account.contains('.') {
            return true;
        }
    }
    false
}

/// True when `account` is in this coin's creator group.
pub fn in_creator_group(state: &CoinState, account: &str) -> bool {
    is_in_group(&state.creator, account)
}

/// The most tokens the creator group may hold: `creator_cap_bps` of supply, rounded down.
pub fn creator_cap(state: &CoinState) -> u128 {
    mul_div(
        state.fixed.total_supply,
        state.fixed.creator_cap_bps as u128,
        10_000,
    )
}
