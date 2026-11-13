//! Holder class (ADR 0001): `small`, `band` or `large`, from the live balance at each action.
//! An account becomes large only by the buy that takes its balance to T (`large_threshold_bps` of
//! supply) or more, and it stays large until its balance is zero. A send never makes a receiver
//! large: the send rules refuse it.

use crate::allowance::unmarked_allowance;
use crate::coin_state::{At, CoinState, Holder};
use crate::wide_integer_math::mul_div_up;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// The balance fits in one allowance.
    Small,
    /// Not large, and the balance is above one allowance.
    Band,
    Large,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Band => "band",
            Self::Large => "large",
        }
    }
}

/// The smallest large balance: T x supply, rounded up.
pub fn large_threshold(state: &CoinState) -> u128 {
    mul_div_up(
        state.fixed.total_supply,
        state.fixed.large_threshold_bps as u128,
        10_000,
    )
}

/// The holder's class at `at`. A missing record is a new account with zero balance.
pub fn class_of(state: &CoinState, holder: Option<&Holder>, at: At) -> Class {
    let Some(holder) = holder else {
        return Class::Small;
    };
    if holder.is_large {
        Class::Large
    } else if holder.balance > unmarked_allowance(state, Some(holder), at).allowance {
        Class::Band
    } else {
        Class::Small
    }
}

/// The most tokens a send to this account can carry now without taking it to T; `None` for a
/// large account (a send to it is not limited), and 0 for an account with no storage record
/// (a send to it is refused).
pub fn max_receive(state: &CoinState, holder: Option<&Holder>) -> Option<u128> {
    match holder {
        None => Some(0),
        Some(h) if h.is_large => None,
        Some(h) => Some((large_threshold(state) - 1).saturating_sub(h.balance)),
    }
}

/// After a buy: marks the holder large when the buy took its balance to T. Returns true when
/// this buy crossed T (the account was not large before).
pub fn mark_large_after_buy(state: &CoinState, holder: &mut Holder) -> bool {
    if holder.is_large || holder.balance < large_threshold(state) {
        return false;
    }
    holder.is_large = true;
    true
}

/// After a sell or a send: a large account whose balance is zero is no longer large. Returns
/// true when the holder stopped being large.
pub fn clear_large_at_zero(holder: &mut Holder) -> bool {
    if holder.is_large && holder.balance == 0 {
        holder.is_large = false;
        return true;
    }
    false
}
