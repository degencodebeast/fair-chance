//! The pool-wide cap on real NEAR (ADR 0005). Large sells, band sells above the first allowance
//! and marked sells together take at most c (`real_cap_c_bps`) of the real NEAR left after each
//! sell, per 1 h window on the same grid as the unmarked allowance.
//!
//! The cap counts each account's net NEAR out in the window. Each account keeps a signed running
//! value: counted sell payouts add to it, buys (the full NEAR paid, fee included) take from it.
//! It is clamped at zero only when it is read, so one account's buys never offset another
//! account's sells. A buy first offsets the account's sells that do not count (small sells and a
//! band account's first allowance) that came before it; a sell that does not count first uses up
//! buy NEAR that no sell used yet. Only the rest of the buys offsets counted sells. The pool's
//! `used` is the sum of the clamped values.

use borsh::{BorshDeserialize, BorshSerialize};
use ethnum::U256;

use crate::allowance::unmarked_window;
use crate::coin_state::{At, CoinState, Holder};

const BPS: u128 = 10_000;

/// The cap used in the current or last 1 h window with an action.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CapWindow {
    pub window: u64,
    /// Sum over accounts of their net NEAR out, each clamped at zero.
    pub used: u128,
    /// `fc_cap_filled` events in the window.
    pub fills: u32,
}

/// One account's net NEAR out for the cap, in its last 1 h window with a buy or a sell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CapNet {
    pub window: u64,
    /// Counted sell payouts minus the buy NEAR left after the uncounted sells took theirs.
    /// Negative while buy NEAR is not used yet. Read through [`CapNet::counted`].
    pub net_out: i128,
    /// Payouts of uncounted sells that no buy offset yet.
    pub uncounted_not_offset: u128,
}

impl CapNet {
    /// What this account adds to the pool's cap used: its net, clamped at zero.
    pub fn counted(self) -> u128 {
        self.net_out.max(0) as u128
    }

    /// True when the account bought or sold in this window (the record is not empty).
    pub fn is_open(self) -> bool {
        self.net_out != 0 || self.uncounted_not_offset != 0
    }

    fn after_buy(mut self, near_in: u128) -> Self {
        let offsets_uncounted = near_in.min(self.uncounted_not_offset);
        self.uncounted_not_offset -= offsets_uncounted;
        self.net_out -= (near_in - offsets_uncounted) as i128;
        self
    }

    fn after_sell(mut self, uncounted_out: u128, counted_out: u128) -> Self {
        let buy_near_not_used = self.net_out.min(0).unsigned_abs();
        let offset = uncounted_out.min(buy_near_not_used);
        self.net_out += offset as i128;
        self.uncounted_not_offset += uncounted_out - offset;
        self.net_out += counted_out as i128;
        self
    }
}

/// Starts a new cap window at the first action of the window: nothing is used yet.
pub fn record_cap_window(state: &mut CoinState, at: At) {
    let window = unmarked_window(state, at);
    if state.cap_window.window != window {
        state.cap_window = CapWindow {
            window,
            ..CapWindow::default()
        };
    }
}

/// The pool's cap used in the window that holds `at`.
pub fn cap_used(state: &CoinState, at: At) -> u128 {
    if state.cap_window.window == unmarked_window(state, at) {
        state.cap_window.used
    } else {
        0
    }
}

/// `fc_cap_filled` events so far in the window that holds `at`.
pub fn fills_this_window(state: &CoinState, at: At) -> u32 {
    if state.cap_window.window == unmarked_window(state, at) {
        state.cap_window.fills
    } else {
        0
    }
}

/// The holder's cap record for the window that holds `at`; empty in a new window.
pub fn cap_net_now(state: &CoinState, holder: Option<&Holder>, at: At) -> CapNet {
    let window = unmarked_window(state, at);
    match holder {
        Some(h) if h.cap_net.window == window => h.cap_net,
        _ => CapNet {
            window,
            ..CapNet::default()
        },
    }
}

/// The holder's record and the pool's cap used after a buy of `near_in`.
pub fn after_buy(state: &CoinState, holder: &Holder, near_in: u128, at: At) -> (CapNet, u128) {
    let before = cap_net_now(state, Some(holder), at);
    let after = before.after_buy(near_in);
    (
        after,
        cap_used(state, at) - before.counted() + after.counted(),
    )
}

/// The holder's record and the pool's cap used after a sell that pays `uncounted_out` from its
/// part that does not count and `counted_out` from its part that counts.
pub fn after_sell(
    state: &CoinState,
    holder: &Holder,
    uncounted_out: u128,
    counted_out: u128,
    at: At,
) -> (CapNet, u128) {
    let before = cap_net_now(state, Some(holder), at);
    let after = before.after_sell(uncounted_out, counted_out);
    (
        after,
        cap_used(state, at) - before.counted() + after.counted(),
    )
}

/// The check after a sell: cap used <= c x the real NEAR left.
pub fn within_cap(state: &CoinState, used: u128, real_near_left: u128) -> bool {
    U256::from(used) * U256::from(BPS)
        <= U256::from(real_near_left) * U256::from(state.fixed.real_cap_c_bps)
}

/// c x real NEAR, rounded down.
pub fn cap_total(state: &CoinState, real_near: u128) -> u128 {
    (U256::from(real_near) * U256::from(state.fixed.real_cap_c_bps) / U256::from(BPS)).as_u128()
}

/// The largest NEAR out x that one more counted sell can take, checked against the real NEAR
/// left after it: used + x <= c x (real NEAR - x - creator share of x). The creator share is
/// s x the swap fee, and the swap fee is `swap_fee_bps` of the NEAR before the fee, so it is
/// x x s x f / (1 - f): one linear term. Solved for x and rounded down, at least 0.
pub fn cap_left(state: &CoinState, used: u128, real_near: u128, creator_share_bps: u32) -> u128 {
    let b = U256::from(BPS);
    let c = U256::from(state.fixed.real_cap_c_bps);
    let f = U256::from(state.fixed.swap_fee_bps);
    let s = U256::from(creator_share_bps);
    let room = c * U256::from(real_near);
    let taken = b * U256::from(used);
    if room <= taken {
        return 0;
    }
    let numerator = (room - taken) * b * (b - f);
    let denominator = b * b * (b - f) + c * b * (b - f) + c * s * f;
    (numerator / denominator).as_u128()
}
