//! The pool-wide cap on real NEAR (ADR 0005). Large sells, band sells above the first allowance
//! and marked sells together take at most c (`real_cap_c_bps`) of the real NEAR left after each
//! sell, per 1 h window on the same grid as the unmarked allowance.
//!
//! The cap counts two things per account per window. Marked sells count gross: a buy never
//! offsets them. The rest is a net: counted unmarked sell payouts add to one signed running
//! value, buys (the full NEAR paid, fee included) take from it. The net is clamped at zero only
//! when it is read, so one account's buys never offset another account's sells, and each buy
//! offsets once. An account adds its marked NEAR out plus its clamped net to the pool's `used`.
//!
//! Which tokens of a sell count: the marked part first (it always counts), then the unmarked
//! tokens the account bought in the current window and has not sold yet (they always count, for
//! every class), then the tokens it held from before the window (by class: large counts; a band
//! account's first allowance and a small account's tokens do not). This order is for the cap
//! count only; the penalty-free part, the allowances and the class rules do not follow it.

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
    /// NEAR out of the account's marked sells in the window, gross: no buy offsets it.
    pub marked_out: u128,
    /// Counted unmarked sell payouts minus the NEAR of all buys in the window. Negative while buy
    /// NEAR is not used yet. Read through [`CapNet::counted`].
    pub net_out: i128,
    /// Unmarked tokens bought in the window and not sold yet. The next sells count them first,
    /// after their marked part.
    pub bought_not_sold: u128,
}

impl CapNet {
    /// What this account adds to the pool's cap used: its marked NEAR out plus its net, the net
    /// clamped at zero.
    pub fn counted(self) -> u128 {
        self.marked_out + self.net_out.max(0) as u128
    }

    /// True when the account bought or sold in this window (the record is not empty).
    pub fn is_open(self) -> bool {
        self.marked_out != 0 || self.net_out != 0 || self.bought_not_sold != 0
    }

    fn after_buy(mut self, near_in: u128, unmarked_tokens: u128) -> Self {
        self.net_out -= near_in as i128;
        self.bought_not_sold += unmarked_tokens;
        self
    }

    fn after_sell(mut self, marked_out: u128, counted_out: u128, bought_tokens_sold: u128) -> Self {
        self.marked_out += marked_out;
        self.net_out += counted_out as i128;
        self.bought_not_sold -= bought_tokens_sold;
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

/// The holder's record and the pool's cap used after a buy of `near_in` that gives
/// `unmarked_tokens` unmarked tokens.
pub fn after_buy(
    state: &CoinState,
    holder: &Holder,
    near_in: u128,
    unmarked_tokens: u128,
    at: At,
) -> (CapNet, u128) {
    let before = cap_net_now(state, Some(holder), at);
    let after = before.after_buy(near_in, unmarked_tokens);
    (
        after,
        cap_used(state, at) - before.counted() + after.counted(),
    )
}

/// The holder's record and the pool's cap used after a sell that pays `marked_out` from its
/// marked part (gross), `counted_out` from its unmarked part that counts (net), and sells
/// `bought_tokens_sold` of the tokens bought in the window.
pub fn after_sell(
    state: &CoinState,
    holder: &Holder,
    marked_out: u128,
    counted_out: u128,
    bought_tokens_sold: u128,
    at: At,
) -> (CapNet, u128) {
    let before = cap_net_now(state, Some(holder), at);
    let after = before.after_sell(marked_out, counted_out, bought_tokens_sold);
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
