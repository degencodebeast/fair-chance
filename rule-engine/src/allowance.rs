//! The unmarked allowance (ADR 0004): one allowance per account per 1 h window on the fixed grid
//! from launch. The window's token reserve is recorded at its first action, before that action
//! takes effect, and sizes every account's allowance for the window: the most tokens one sell can
//! put into the pool while the price falls by at most `p_bps`. Each account counts its tokens
//! sold and bought in the window; `used` = max(0, sold - bought). Nothing carries over.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::coin_state::{At, CoinState, Holder};
use crate::wide_integer_math::{mul_div, tokens_for_price_drop};

const SECOND_NS: u64 = 1_000_000_000;

/// The token reserve recorded at the first action of one 1 h window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WindowReserve {
    pub window: u64,
    pub token_reserve: u128,
}

/// One account's unmarked tokens sold and bought in one 1 h window, and its marked tokens sold
/// in that window (they use up the unmarked floor: one floor per account per window).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WindowTotals {
    pub window: u64,
    pub sold: u128,
    pub bought: u128,
    pub marked_sold: u128,
}

/// One account's unmarked allowance at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnmarkedAllowance {
    pub allowance: u128,
    /// max(0, sold - bought) in the window.
    pub used: u128,
    /// allowance - used, at least 0.
    pub left: u128,
    /// The most tokens one more sell can take while `used` stays at or below `allowance`:
    /// allowance + bought - sold, at least 0. A buy gives room back.
    pub sell_room: u128,
    /// The allowance less the tokens sold in the window, at least 0, with no room back for
    /// buys: the part of a band account's sells that never counts toward the real NEAR cap. A
    /// band account that buys and sells back in the window counts the bought part (ADR 0005).
    pub first_allowance_left: u128,
    /// The floor (ADR 0005) less `used` and less the marked tokens the account sold in the
    /// window, at least 0. The floor is `floor_bps` of the window's token reserve, rounded down:
    /// the room per window a limited sell keeps when the real NEAR cap is full. One floor per
    /// account per window, marked and unmarked together.
    pub floor_left: u128,
    pub window_end_ns: u64,
}

fn window_length_ns(state: &CoinState) -> u64 {
    state.fixed.window_unmarked_s * SECOND_NS
}

/// Index k of the 1 h window that holds `at`: launch + k x 1 h <= at < launch + (k + 1) x 1 h.
pub fn unmarked_window(state: &CoinState, at: At) -> u64 {
    at.ns.saturating_sub(state.launch_ns) / window_length_ns(state)
}

/// End of the 1 h window that holds `at`.
pub fn unmarked_window_end_ns(state: &CoinState, at: At) -> u64 {
    state.launch_ns + (unmarked_window(state, at) + 1) * window_length_ns(state)
}

/// The token reserve that sizes the allowances of the window that holds `at`. Before the
/// window's first action it is the reserve now, which that action will record.
fn window_token_reserve(state: &CoinState, at: At) -> u128 {
    let window = unmarked_window(state, at);
    if state.unmarked_window_reserve.window == window {
        state.unmarked_window_reserve.token_reserve
    } else {
        state.token_reserve
    }
}

/// Records the window's token reserve if this is the window's first action. Call it before the
/// action changes the pool.
pub fn record_window_reserve(state: &mut CoinState, at: At) {
    state.unmarked_window_reserve = WindowReserve {
        window: unmarked_window(state, at),
        token_reserve: window_token_reserve(state, at),
    };
}

/// The holder's totals for the window that holds `at`; zero in a new window (no carry-over).
fn totals_now(state: &CoinState, holder: Option<&Holder>, at: At) -> WindowTotals {
    let window = unmarked_window(state, at);
    match holder {
        Some(h) if h.unmarked_window.window == window => h.unmarked_window,
        _ => WindowTotals {
            window,
            ..WindowTotals::default()
        },
    }
}

/// The account's unmarked allowance at `at`.
pub fn unmarked_allowance(state: &CoinState, holder: Option<&Holder>, at: At) -> UnmarkedAllowance {
    let reserve = window_token_reserve(state, at);
    let allowance = tokens_for_price_drop(reserve, state.fixed.p_bps);
    let floor = mul_div(reserve, state.fixed.floor_bps as u128, 10_000);
    let totals = totals_now(state, holder, at);
    let used = totals.sold.saturating_sub(totals.bought);
    UnmarkedAllowance {
        allowance,
        used,
        left: allowance.saturating_sub(used),
        sell_room: (allowance + totals.bought).saturating_sub(totals.sold),
        first_allowance_left: allowance.saturating_sub(totals.sold),
        floor_left: floor
            .saturating_sub(used)
            .saturating_sub(totals.marked_sold),
        window_end_ns: unmarked_window_end_ns(state, at),
    }
}

/// Adds unmarked tokens sold to the holder's totals for the window that holds `at`.
pub fn count_sold(state: &CoinState, holder: &mut Holder, tokens: u128, at: At) {
    let mut totals = totals_now(state, Some(holder), at);
    totals.sold += tokens;
    holder.unmarked_window = totals;
}

/// Adds unmarked tokens bought to the holder's totals for the window that holds `at`.
pub fn count_bought(state: &CoinState, holder: &mut Holder, tokens: u128, at: At) {
    let mut totals = totals_now(state, Some(holder), at);
    totals.bought += tokens;
    holder.unmarked_window = totals;
}

/// Adds marked tokens sold to the holder's totals for the window that holds `at`.
pub fn count_marked_sold(state: &CoinState, holder: &mut Holder, tokens: u128, at: At) {
    let mut totals = totals_now(state, Some(holder), at);
    totals.marked_sold += tokens;
    holder.unmarked_window = totals;
}
