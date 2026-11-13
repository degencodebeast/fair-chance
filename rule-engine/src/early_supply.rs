//! Early supply, also called marked tokens (ADR 0001, ADR 0008).
//!
//! Tokens bought in blocks 0 to N (the early window), bought by the creator group at any time,
//! or received by the creator group are marked. The mark follows the tokens: a send moves
//! marked tokens first. Marked tokens cannot be sold in the early window.
//!
//! All marked tokens share one allowance per 24 h window on the fixed grid from launch. The
//! window's token reserve and all marked supply are recorded at its first action (in window 0,
//! at the first action after block N). Each account may use a pro-rata share: the shared
//! allowance times its marked balance just before its first touch in the window (buy, send,
//! receive or sell; in window 0, its first touch after block N), divided by the marked supply at
//! the window start. A marked balance changes only at a touch of that account, so the balance
//! at the first touch is the balance at the window start, and each marked token counts once per
//! window. The marked allowance counts gross: a buy gives no room back. One floor is shared in
//! the same way; an account's part of it also loses the unmarked tokens it sold in the current
//! 1 h window, so it gets one floor per 1 h window, marked and unmarked together.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::allowance::unmarked_sold_in_window;
use crate::coin_state::{At, CoinState, Holder};
use crate::wide_integer_math::{mul_div, tokens_for_price_drop};

const SECOND_NS: u64 = 1_000_000_000;
const BPS: u128 = 10_000;

/// The shared early-supply window: what the coin recorded at its first action.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EarlySupplyWindow {
    pub window: u64,
    /// False until the window's first action after the early window records it.
    pub recorded: bool,
    /// The token reserve before that action: it sizes the shared allowance and floor.
    pub token_reserve: u128,
    /// All marked supply before that action: the base of every pro-rata share.
    pub marked_supply_at_start: u128,
    /// Marked tokens sold in the window, all accounts together.
    pub sold: u128,
}

/// One account's record in its last 24 h window with a touch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MarkedShare {
    pub window: u64,
    /// False until the account's first touch after the early window records it.
    pub recorded: bool,
    /// The marked balance just before the account's first touch in the window.
    pub balance_at_first_touch: u128,
    /// Marked tokens the account sold in the window (gross).
    pub sold: u128,
}

/// One account's marked allowance at one moment (`fc_account.marked`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkedAllowance {
    pub share_bps: u32,
    pub allowance: u128,
    pub used: u128,
    pub left: u128,
    /// The account's part of the shared floor, less `used` and less the unmarked tokens it sold
    /// in the current 1 h window, at least 0: one floor per account per 1 h window.
    pub floor_left: u128,
    pub window_end_ns: u64,
}

/// The shared early-supply facts at one moment (`fc_early_window`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedAllowance {
    /// The shared allowance of the window; 0 in the early window.
    pub allowance: u128,
    /// The shared floor of the window; 0 in the early window.
    pub floor: u128,
    /// The marked supply that is the base of every share in the window.
    pub marked_supply_at_start: u128,
    /// Marked tokens sold in the window, all accounts together; 0 before its first action.
    pub sold: u128,
}

/// True in blocks 0 to N of the coin.
pub fn in_early_window(state: &CoinState, at: At) -> bool {
    at.block.saturating_sub(state.launch_block) <= state.fixed.early_blocks
}

fn window_length_ns(state: &CoinState) -> u64 {
    state.fixed.window_early_s * SECOND_NS
}

/// Index k of the 24 h window that holds `at`.
pub fn early_supply_window(state: &CoinState, at: At) -> u64 {
    at.ns.saturating_sub(state.launch_ns) / window_length_ns(state)
}

/// End of the 24 h window that holds `at`.
pub fn early_supply_window_end_ns(state: &CoinState, at: At) -> u64 {
    state.launch_ns + (early_supply_window(state, at) + 1) * window_length_ns(state)
}

/// Records the shared window at its first action after the early window. Call it before the
/// action changes the pool or any marked balance.
pub fn record_early_supply_window(state: &mut CoinState, at: At) {
    if in_early_window(state, at) {
        return;
    }
    let window = early_supply_window(state, at);
    if state.early_supply_window.recorded && state.early_supply_window.window == window {
        return;
    }
    state.early_supply_window = EarlySupplyWindow {
        window,
        recorded: true,
        token_reserve: state.token_reserve,
        marked_supply_at_start: state.marked_supply,
        sold: 0,
    };
}

/// Records the account's marked balance at its first touch in the window after the early
/// window. Call it for each account an action touches, before the action changes its balance.
pub fn record_first_touch(state: &CoinState, holder: &mut Holder, at: At) {
    if in_early_window(state, at) {
        return;
    }
    let window = early_supply_window(state, at);
    if holder.marked_share.recorded && holder.marked_share.window == window {
        return;
    }
    holder.marked_share = MarkedShare {
        window,
        recorded: true,
        balance_at_first_touch: holder.marked_balance,
        sold: 0,
    };
}

/// The shared allowance and floor of the window that holds `at`. Before the window's first
/// action they come from the pool now, which that action will record.
pub fn shared_allowance(state: &CoinState, at: At) -> SharedAllowance {
    if in_early_window(state, at) {
        return SharedAllowance {
            allowance: 0,
            floor: 0,
            marked_supply_at_start: state.marked_supply,
            sold: 0,
        };
    }
    let recorded = state.early_supply_window;
    let (reserve, marked_supply_at_start, sold) =
        if recorded.recorded && recorded.window == early_supply_window(state, at) {
            (
                recorded.token_reserve,
                recorded.marked_supply_at_start,
                recorded.sold,
            )
        } else {
            (state.token_reserve, state.marked_supply, 0)
        };
    SharedAllowance {
        allowance: tokens_for_price_drop(reserve, state.fixed.p_bps),
        floor: mul_div(reserve, state.fixed.floor_bps as u128, BPS),
        marked_supply_at_start,
        sold,
    }
}

/// The account's marked allowance at `at`. Every rounding is down, so the shares never add up
/// to more than the shared allowance.
pub fn marked_allowance(state: &CoinState, holder: Option<&Holder>, at: At) -> MarkedAllowance {
    let window_end_ns = early_supply_window_end_ns(state, at);
    let shared = shared_allowance(state, at);
    let window = early_supply_window(state, at);
    let (base, used) = match holder {
        Some(h) if h.marked_share.recorded && h.marked_share.window == window => {
            (h.marked_share.balance_at_first_touch, h.marked_share.sold)
        }
        Some(h) => (h.marked_balance, 0),
        None => (0, 0),
    };
    let denominator = shared.marked_supply_at_start;
    if denominator == 0 || shared.allowance == 0 {
        return MarkedAllowance {
            share_bps: 0,
            allowance: 0,
            used,
            left: 0,
            floor_left: 0,
            window_end_ns,
        };
    }
    let allowance = mul_div(shared.allowance, base, denominator);
    let floor = mul_div(shared.floor, base, denominator);
    MarkedAllowance {
        share_bps: mul_div(base, BPS, denominator) as u32,
        allowance,
        used,
        left: allowance.saturating_sub(used),
        floor_left: floor
            .saturating_sub(used)
            .saturating_sub(unmarked_sold_in_window(state, holder, at)),
        window_end_ns,
    }
}

/// Tokens a buy marks: all of them in the early window or for a creator-group buyer, else 0.
pub fn marked_by_buy(state: &CoinState, buyer_in_group: bool, tokens_out: u128, at: At) -> u128 {
    if buyer_in_group || in_early_window(state, at) {
        tokens_out
    } else {
        0
    }
}
