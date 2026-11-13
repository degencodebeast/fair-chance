//! Constant-product pool math on `token_reserve` and `near_reserve` (virtual P plus real NEAR).
//! Every rounding goes in the pool's favour: tokens and NEAR out round down, fees round up.

use crate::wide_integer_math::{mul_div, mul_div_up};

const BPS: u128 = 10_000;

/// The swap fee on `near` at `fee_bps`, rounded up.
pub fn fee(near: u128, fee_bps: u32) -> u128 {
    mul_div_up(near, fee_bps as u128, BPS)
}

/// Tokens out for `near_after_fee` NEAR in. The fee part of a buy stays in the pool and does
/// not trade.
pub fn tokens_out(token_reserve: u128, near_reserve: u128, near_after_fee: u128) -> u128 {
    mul_div(token_reserve, near_after_fee, near_reserve + near_after_fee)
}

/// NEAR out before the fee for `tokens_in` tokens in.
pub fn near_out_before_fee(token_reserve: u128, near_reserve: u128, tokens_in: u128) -> u128 {
    mul_div(near_reserve, tokens_in, token_reserve + tokens_in)
}

/// The creator's share of a swap fee that the engine applies now: none. `fc_rules` reports
/// `creator_fee_share_bps` from the fixed values; the creator fee work applies it here.
pub fn applied_creator_fee_share_bps() -> u32 {
    0
}

/// The creator's share of `fee_near` (the normal swap fee), rounded down.
pub fn creator_fee(fee_near: u128) -> u128 {
    mul_div(fee_near, applied_creator_fee_share_bps() as u128, BPS)
}
