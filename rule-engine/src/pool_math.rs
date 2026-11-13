//! Constant-product pool math on `token_reserve` and `near_reserve` (virtual P plus real NEAR).
//! Every rounding goes in the pool's favour: tokens and NEAR out round down, fees round up.

use crate::fixed_values::FixedValues;
use crate::wide_integer_math::{mul_div, mul_div_up};

const BPS: u128 = 10_000;

/// The swap fee on `near` at `fee_bps`, rounded up.
pub fn fee(near: u128, fee_bps: u32) -> u128 {
    mul_div_up(near, fee_bps as u128, BPS)
}

/// `fee_bps` of `near`, rounded down. Only for a step where a smaller fee favours the pool.
pub fn fee_rounded_down(near: u128, fee_bps: u32) -> u128 {
    mul_div(near, fee_bps as u128, BPS)
}

/// Tokens out for `near_after_fee` NEAR in. The fee part of a buy does not trade; it stays in the
/// pool, less the creator share.
pub fn tokens_out(token_reserve: u128, near_reserve: u128, near_after_fee: u128) -> u128 {
    mul_div(token_reserve, near_after_fee, near_reserve + near_after_fee)
}

/// NEAR out before the fee for `tokens_in` tokens in.
pub fn near_out_before_fee(token_reserve: u128, near_reserve: u128, tokens_in: u128) -> u128 {
    mul_div(near_reserve, tokens_in, token_reserve + tokens_in)
}

/// The creator's share of the normal swap fee that the engine applies (ADR 0010), in basis points.
/// `fc_rules.creator_fee_share_bps` reports this value, so the view always tells what is applied.
pub fn applied_creator_fee_share_bps(fixed: &FixedValues) -> u32 {
    fixed.creator_fee_share_bps
}

/// The creator's share of `fee_near` (the normal swap fee), rounded down: the remainder stays in
/// the pool.
pub fn creator_fee(fixed: &FixedValues, fee_near: u128) -> u128 {
    mul_div(fee_near, applied_creator_fee_share_bps(fixed) as u128, BPS)
}

/// NEAR out before the fee for `tokens_in` tokens in, rounded up. Used only where a larger
/// value favours the pool.
pub fn near_out_before_fee_rounded_up(
    token_reserve: u128,
    near_reserve: u128,
    tokens_in: u128,
) -> u128 {
    mul_div_up(near_reserve, tokens_in, token_reserve + tokens_in)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coin_state::{launch, At, LaunchRequest};
    use crate::views::view_rules;

    #[test]
    fn fc_rules_reports_the_creator_share_the_engine_applies() {
        let state = launch(
            FixedValues::fixed(),
            LaunchRequest {
                coin: "coin.factory.test",
                caller: "factory.test",
                creator: "creator.test",
                code_hash: "hash",
                name: "Test Coin",
                symbol: "TEST",
                icon: None,
                description: None,
            },
            At { block: 1, ns: 1 },
        )
        .expect("launch")
        .state;
        assert_eq!(
            view_rules(&state).creator_fee_share_bps,
            applied_creator_fee_share_bps(&state.fixed)
        );
    }
}
