//! The fixed values of every coin (the `fc_rules` view). Code constants; nobody can change them.

use borsh::{BorshDeserialize, BorshSerialize};

pub(crate) const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const TOKEN_DECIMALS: u8 = 18;

/// The fixed values of every coin. One set for all coins; nobody can change them.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FixedValues {
    pub total_supply: u128,
    pub decimals: u8,
    pub virtual_near: u128,
    pub p_bps: u32,
    pub window_unmarked_s: u64,
    pub window_early_s: u64,
    pub early_blocks: u64,
    pub large_threshold_bps: u32,
    pub creator_cap_bps: u32,
    pub real_cap_c_bps: u32,
    pub floor_bps: u32,
    pub penalty_bps: u32,
    pub swap_fee_bps: u32,
    pub early_fee_start_bps: u32,
    pub brake_drop_bps: Option<u32>,
    pub creator_fee_share_bps: u32,
    pub claim_min_real_near: u128,
    pub claim_depth_hold_s: u64,
    pub unclaimed_to_pool_s: Option<u64>,
}

impl FixedValues {
    pub fn fixed() -> Self {
        Self {
            total_supply: 1_000_000_000 * 10u128.pow(TOKEN_DECIMALS as u32),
            decimals: TOKEN_DECIMALS,
            virtual_near: 100 * ONE_NEAR,
            p_bps: 200,
            window_unmarked_s: 3600,
            window_early_s: 86_400,
            early_blocks: 17,
            large_threshold_bps: 200,
            creator_cap_bps: 400,
            real_cap_c_bps: 500,
            floor_bps: 10,
            penalty_bps: 1000,
            swap_fee_bps: 100,
            early_fee_start_bps: 5000,
            brake_drop_bps: None,
            creator_fee_share_bps: 3000,
            claim_min_real_near: 10 * ONE_NEAR,
            claim_depth_hold_s: 86_400,
            unclaimed_to_pool_s: None,
        }
    }
}
