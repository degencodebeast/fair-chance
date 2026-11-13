//! The creator fee claim (ADR 0010). The creator's share of the normal swap fee builds up in a
//! balance outside the pool's real NEAR. The creator can claim it once the pool has held at least
//! `claim_min_real_near` (X) for `claim_depth_hold_s` (24 h).
//!
//! Depth: the coin keeps the lowest real NEAR of each 1 h window, on the allowance grid from
//! launch, read after each action. A new window starts that value at the real NEAR carried from
//! the last action, and a window with no actions keeps it. Window 0 starts at 0. `depth_ok_since`
//! is the start of the first window of the current run of windows whose lowest value is at or
//! above X; any window below X clears it. The current window takes part with its lowest value
//! so far, so a dip in it refuses a claim at once. A claim needs at least 24 h since
//! `depth_ok_since`, so the last 24 full windows and the current one all held X.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::allowance::unmarked_window;
use crate::coin_state::{At, CoinState};
use crate::events::{Event, FcCreatorClaim};
use crate::refusals::{Refusal, RefusalCode};

const SECOND_NS: u64 = 1_000_000_000;

/// The lowest real NEAR of the current or last 1 h window with an action, and the depth count
/// of the windows before it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ClaimDepth {
    pub window: u64,
    /// The lowest real NEAR in `window` so far, read after each action.
    pub lowest_real_near: u128,
    /// The start of the first window of the run of windows before `window` whose lowest real NEAR
    /// was at least X. `None` when the window right before `window` was below X.
    pub ok_since_ns: Option<u64>,
}

fn window_start_ns(state: &CoinState, window: u64) -> u64 {
    state.launch_ns + window * state.fixed.window_unmarked_s * SECOND_NS
}

/// `ok_since` after one more window with this lowest real NEAR.
fn count_window(
    state: &CoinState,
    ok_since: Option<u64>,
    window: u64,
    lowest: u128,
) -> Option<u64> {
    if lowest >= state.fixed.claim_min_real_near {
        ok_since.or(Some(window_start_ns(state, window)))
    } else {
        None
    }
}

/// The depth record for the window that holds `at`. The windows passed since the record close:
/// the record's own window with its lowest value, and every window with no actions with the
/// real NEAR carried from the last action. The new window starts at that carried value.
fn depth_now(state: &CoinState, at: At) -> ClaimDepth {
    let depth = state.claim_depth;
    let window = unmarked_window(state, at);
    if window <= depth.window {
        return depth;
    }
    let mut ok_since = count_window(
        state,
        depth.ok_since_ns,
        depth.window,
        depth.lowest_real_near,
    );
    if window > depth.window + 1 {
        // Every window with no actions holds the same carried value, so the first one decides.
        ok_since = count_window(state, ok_since, depth.window + 1, state.real_near);
    }
    ClaimDepth {
        window,
        lowest_real_near: state.real_near,
        ok_since_ns: ok_since,
    }
}

/// Closes the windows passed since the last action. Call it before the action.
pub fn record_claim_depth_window(state: &mut CoinState, at: At) {
    state.claim_depth = depth_now(state, at);
}

/// Lowers the current window's lowest real NEAR to the real NEAR after the action.
pub fn record_real_near_after_action(state: &mut CoinState) {
    let lowest = &mut state.claim_depth.lowest_real_near;
    *lowest = (*lowest).min(state.real_near);
}

/// `depth_ok_since` now: the start of the run of windows, the current one included, whose
/// lowest real NEAR is at least X. `None` when the current window's lowest value is below X.
pub fn depth_ok_since_ns(state: &CoinState, at: At) -> Option<u64> {
    let depth = depth_now(state, at);
    count_window(
        state,
        depth.ok_since_ns,
        depth.window,
        depth.lowest_real_near,
    )
}

/// The earliest claim time if real NEAR stays at least X; `None` when the count is off.
pub fn claimable_at_ns(state: &CoinState, at: At) -> Option<u64> {
    depth_ok_since_ns(state, at).map(|since| since + state.fixed.claim_depth_hold_s * SECOND_NS)
}

/// True when the creator can claim now: the depth held for 24 h and the balance is above 0.
pub fn claimable_now(state: &CoinState, at: At) -> bool {
    state.creator_fee_balance > 0 && claimable_at_ns(state, at).is_some_and(|ready| at.ns >= ready)
}

/// The claim: refused without exactly 1 yoctoNEAR, from any account but the creator, and before
/// it is claimable. Sets the balance to 0 and returns the amount to send to the creator. Real
/// NEAR and the cap do not change.
pub fn claim(
    state: &CoinState,
    account: &str,
    attached: u128,
    at: At,
) -> Result<(CoinState, u128), Refusal> {
    if attached != 1 {
        return Err(Refusal::new(RefusalCode::OneYocto));
    }
    if account != state.creator {
        return Err(Refusal::new(RefusalCode::NotCreator));
    }
    if !claimable_now(state, at) {
        return Err(Refusal::new(RefusalCode::ClaimNotReady));
    }
    let mut next = state.clone();
    next.creator_fee_balance = 0;
    Ok((next, state.creator_fee_balance))
}

/// The state and events after the claim's payout transfer settles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimSettled {
    pub state: CoinState,
    pub events: Vec<Event>,
}

/// Settles a claim of `amount` when its transfer to the creator returns. On success the amount
/// counts as claimed and `fc_creator_claim` is logged. On failure the amount goes back to the
/// creator balance, so no NEAR is lost between counters, and nothing is logged.
pub fn settle_creator_claim(
    state: &CoinState,
    amount: u128,
    depth_ok_since_ns: u64,
    transfer_succeeded: bool,
    at: At,
) -> ClaimSettled {
    let mut next = state.clone();
    if !transfer_succeeded {
        next.creator_fee_balance += amount;
        return ClaimSettled {
            state: next,
            events: vec![],
        };
    }
    next.creator_fees_claimed += amount;
    let events = vec![Event::FcCreatorClaim(FcCreatorClaim {
        creator: state.creator.clone(),
        amount,
        depth_ok_since_ns,
        block: at.block,
    })];
    ClaimSettled {
        state: next,
        events,
    }
}
