//! The falling buy fee in the early window (ADR 0008): fee(b) = f + (start - f) x (N + 1 - b) /
//! (N + 1) for b = 0 to N, where b = block height - launch height, f = `swap_fee_bps` and start
//! = `early_fee_start_bps`. With f = 1%, start = 50% and N = 17 that is 50% at b = 0 and 1% from
//! b = 18. No account is exempt. The extra above f stays in the pool; the creator share is of
//! the normal fee f only.

use crate::coin_state::{At, CoinState};
use crate::wide_integer_math::mul_div_up;

const BPS: u128 = 10_000;

/// The fee rate now as a fraction: (numerator, denominator).
fn rate(state: &CoinState, at: At) -> (u128, u128) {
    let normal = state.fixed.swap_fee_bps as u128;
    let steps = state.fixed.early_blocks as u128 + 1;
    let b = at.block.saturating_sub(state.launch_block) as u128;
    if b >= steps {
        return (normal, BPS);
    }
    let start = (state.fixed.early_fee_start_bps as u128).max(normal);
    (normal * steps + (start - normal) * (steps - b), BPS * steps)
}

/// The whole buy fee on `near_in` now, rounded up. Equal to the normal swap fee from block N + 1.
pub fn buy_fee(state: &CoinState, near_in: u128, at: At) -> u128 {
    let (numerator, denominator) = rate(state, at);
    mul_div_up(near_in, numerator, denominator)
}

/// The buy fee now in basis points, rounded up (`fc_early_window.buy_fee_bps_now`).
pub fn buy_fee_bps_now(state: &CoinState, at: At) -> u32 {
    let (numerator, denominator) = rate(state, at);
    mul_div_up(BPS, numerator, denominator) as u32
}
