//! The eight coin views of INTERFACE.md, as pure functions of the coin state.

use serde::Serialize;

use crate::actions::{apply, largest_full_fill, Action};
use crate::allowance::{unmarked_allowance, unmarked_window_end_ns};
use crate::coin_state::{At, CoinState, Holder, Holders};
use crate::creator_group::in_creator_group;
use crate::early_buy_fee;
use crate::early_supply::{self, marked_allowance};
use crate::events::Event;
use crate::holder_class::{class_of, max_receive, Class};
use crate::pool_math;
use crate::real_near_cap;
use crate::wide_integer_math::mul_div;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RulesView {
    pub creator: String,
    pub launch_block: u64,
    #[serde(with = "crate::decimal_string")]
    pub launch_ns: u64,
    pub p_bps: u32,
    pub window_unmarked_s: u64,
    pub window_early_s: u64,
    pub early_blocks: u64,
    pub large_threshold_bps: u32,
    pub creator_cap_bps: u32,
    pub real_cap_c_bps: u32,
    pub floor_bps: u32,
    pub penalty_bps: u32,
    #[serde(with = "crate::decimal_string")]
    pub virtual_near: u128,
    pub swap_fee_bps: u32,
    pub early_fee_start_bps: u32,
    pub brake_drop_bps: Option<u32>,
    pub creator_fee_share_bps: u32,
    #[serde(with = "crate::decimal_string")]
    pub claim_min_real_near: u128,
    pub claim_depth_hold_s: u64,
    pub unclaimed_to_pool_s: Option<u64>,
    pub code_hash: String,
    /// The one-line launch description; null when the creator gave none.
    pub description: Option<String>,
}

pub fn view_rules(state: &CoinState) -> RulesView {
    let r = &state.fixed;
    RulesView {
        creator: state.creator.clone(),
        launch_block: state.launch_block,
        launch_ns: state.launch_ns,
        p_bps: r.p_bps,
        window_unmarked_s: r.window_unmarked_s,
        window_early_s: r.window_early_s,
        early_blocks: r.early_blocks,
        large_threshold_bps: r.large_threshold_bps,
        creator_cap_bps: r.creator_cap_bps,
        real_cap_c_bps: r.real_cap_c_bps,
        floor_bps: r.floor_bps,
        penalty_bps: r.penalty_bps,
        virtual_near: r.virtual_near,
        swap_fee_bps: r.swap_fee_bps,
        early_fee_start_bps: r.early_fee_start_bps,
        brake_drop_bps: r.brake_drop_bps,
        creator_fee_share_bps: r.creator_fee_share_bps,
        claim_min_real_near: r.claim_min_real_near,
        claim_depth_hold_s: r.claim_depth_hold_s,
        unclaimed_to_pool_s: r.unclaimed_to_pool_s,
        code_hash: state.code_hash.clone(),
        description: state.metadata.description.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PoolView {
    #[serde(with = "crate::decimal_string")]
    pub token_reserve: u128,
    #[serde(with = "crate::decimal_string")]
    pub near_reserve: u128,
    #[serde(with = "crate::decimal_string")]
    pub virtual_near: u128,
    #[serde(with = "crate::decimal_string")]
    pub real_near: u128,
    #[serde(with = "crate::decimal_string")]
    pub creator_fee_balance: u128,
    pub large_holders: u32,
    pub creator_group_share_bps: u32,
    pub block: u64,
}

pub fn view_pool(state: &CoinState, at: At) -> PoolView {
    PoolView {
        token_reserve: state.token_reserve,
        near_reserve: state.near_reserve(),
        virtual_near: state.fixed.virtual_near,
        real_near: state.real_near,
        creator_fee_balance: state.creator_fee_balance,
        large_holders: state.large_holders,
        // Rounded down.
        creator_group_share_bps: mul_div(
            state.creator_group_balance,
            10_000,
            state.fixed.total_supply,
        ) as u32,
        block: at.block,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EarlyWindowView {
    pub early_blocks: u64,
    pub blocks_left: u64,
    #[serde(with = "crate::decimal_string")]
    pub early_supply_total: u128,
    #[serde(with = "crate::decimal_string")]
    pub shared_allowance: u128,
    #[serde(with = "crate::decimal_string")]
    pub rate_per_token: u128,
    pub buy_fee_bps_now: u32,
    pub fee_end_block: u64,
    /// Marked tokens sold in the current 24 h window, all accounts together.
    #[serde(with = "crate::decimal_string")]
    pub shared_allowance_used: u128,
    /// `shared_allowance` less `shared_allowance_used`, at least 0.
    #[serde(with = "crate::decimal_string")]
    pub shared_allowance_left: u128,
    /// End of the current 24 h window, when the shared allowance refills.
    #[serde(with = "crate::decimal_string")]
    pub window_end_ns: u64,
}

pub fn view_early_window(state: &CoinState, at: At) -> EarlyWindowView {
    let shared = early_supply::shared_allowance(state, at);
    let end = state.launch_block + state.fixed.early_blocks;
    EarlyWindowView {
        early_blocks: state.fixed.early_blocks,
        // The early window is blocks 0 to `early_blocks` after the launch, so the current block
        // counts while it is one of them: 18 at block 0, 1 at block 17, 0 from block 18.
        blocks_left: (end + 1).saturating_sub(at.block),
        early_supply_total: state.marked_supply,
        shared_allowance: shared.allowance,
        // The shared allowance per marked token at the window start, in basis points, rounded
        // down: an account may sell this share of its marked balance in the window.
        rate_per_token: if shared.marked_supply_at_start == 0 {
            0
        } else {
            mul_div(shared.allowance, 10_000, shared.marked_supply_at_start)
        },
        buy_fee_bps_now: early_buy_fee::buy_fee_bps_now(state, at),
        fee_end_block: end + 1,
        shared_allowance_used: shared.sold,
        shared_allowance_left: shared.allowance.saturating_sub(shared.sold),
        window_end_ns: early_supply::early_supply_window_end_ns(state, at),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapView {
    #[serde(with = "crate::decimal_string")]
    pub window_start_ns: u64,
    #[serde(with = "crate::decimal_string")]
    pub window_end_ns: u64,
    #[serde(with = "crate::decimal_string")]
    pub real_near_now: u128,
    #[serde(with = "crate::decimal_string")]
    pub cap_total: u128,
    #[serde(with = "crate::decimal_string")]
    pub cap_used: u128,
    #[serde(with = "crate::decimal_string")]
    pub cap_left: u128,
    pub is_full: bool,
    pub fills_this_window: u32,
}

pub fn view_cap(state: &CoinState, at: At) -> CapView {
    let end = unmarked_window_end_ns(state, at);
    let used = real_near_cap::cap_used(state, at);
    let cap_left = real_near_cap::cap_left(
        state,
        used,
        state.real_near,
        pool_math::applied_creator_fee_share_bps(),
    );
    CapView {
        window_start_ns: end - state.fixed.window_unmarked_s * 1_000_000_000,
        window_end_ns: end,
        real_near_now: state.real_near,
        cap_total: real_near_cap::cap_total(state, state.real_near),
        cap_used: used,
        cap_left,
        is_full: cap_left == 0,
        fills_this_window: real_near_cap::fills_this_window(state, at),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AllowanceView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_bps: Option<u32>,
    #[serde(with = "crate::decimal_string")]
    pub allowance: u128,
    #[serde(with = "crate::decimal_string")]
    pub used: u128,
    #[serde(with = "crate::decimal_string")]
    pub left: u128,
    #[serde(with = "crate::decimal_string")]
    pub floor_left: u128,
    #[serde(with = "crate::decimal_string")]
    pub window_end_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountView {
    pub class: &'static str,
    #[serde(with = "crate::decimal_string")]
    pub balance: u128,
    #[serde(with = "crate::decimal_string")]
    pub marked_balance: u128,
    #[serde(with = "crate::decimal_string")]
    pub unmarked_balance: u128,
    pub is_large: bool,
    pub can_send: bool,
    pub in_creator_group: bool,
    #[serde(with = "crate::decimal_string::opt")]
    pub max_receive: Option<u128>,
    pub unmarked: AllowanceView,
    pub marked: AllowanceView,
    #[serde(with = "crate::decimal_string")]
    pub max_full_fill_now: u128,
    #[serde(with = "crate::decimal_string::opt")]
    pub penalty_applies_above: Option<u128>,
}

pub fn view_account(
    state: &CoinState,
    account: &str,
    holder: Option<&Holder>,
    at: At,
) -> AccountView {
    let balance = holder.map_or(0, |h| h.balance);
    let marked_balance = holder.map_or(0, |h| h.marked_balance);
    let marked = marked_allowance(state, holder, at);
    let class = class_of(state, holder, at);
    let is_large = class == Class::Large;
    let unmarked = unmarked_allowance(state, holder, at);
    let marked_part = marked_balance.min(marked.left);
    let unmarked_part = match class {
        Class::Large | Class::Band => (balance - marked_balance).min(unmarked.sell_room),
        Class::Small => balance - marked_balance,
    };
    let window_end_ns = unmarked.window_end_ns;
    AccountView {
        class: class.as_str(),
        balance,
        marked_balance,
        unmarked_balance: balance - marked_balance,
        is_large,
        can_send: !is_large,
        in_creator_group: in_creator_group(state, account),
        max_receive: max_receive(state, holder),
        unmarked: AllowanceView {
            share_bps: None,
            allowance: unmarked.allowance,
            used: unmarked.used,
            left: unmarked.left,
            floor_left: unmarked.floor_left,
            window_end_ns,
        },
        marked: AllowanceView {
            share_bps: Some(marked.share_bps),
            allowance: marked.allowance,
            used: marked.used,
            left: marked.left,
            floor_left: marked.floor_left,
            window_end_ns: marked.window_end_ns,
        },
        // The marked part fills first, up to the share left; then the unmarked part up to the
        // allowance room (large and band) or in full (small). A large or marked sell above what
        // the real NEAR cap lets through fills in part; a band sell above the room pays the
        // penalty.
        max_full_fill_now: {
            let holder = holder.cloned().unwrap_or_default();
            largest_full_fill(state, &holder, class, marked_part + unmarked_part, at)
        },
        // A band sell above the marked share left plus the unmarked first allowance room pays
        // the penalty. It equals `max_full_fill_now` unless the cap holds the marked part.
        penalty_applies_above: (class == Class::Band).then_some(marked_part + unmarked_part),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SellQuote {
    #[serde(with = "crate::decimal_string")]
    pub tokens_filled: u128,
    #[serde(with = "crate::decimal_string")]
    pub near_out: u128,
    #[serde(with = "crate::decimal_string")]
    pub penalty_near: u128,
    pub fill: &'static str,
    pub limit_reason: Option<&'static str>,
    #[serde(with = "crate::decimal_string")]
    pub cap_left_after: u128,
}

/// A preview of one sell now: the same rule step as the real sell, with the result dropped.
pub fn quote_sell(
    state: &CoinState,
    holders: Holders,
    account: &str,
    amount: u128,
    at: At,
) -> Option<SellQuote> {
    let action = Action::Sell {
        account: account.to_string(),
        amount,
        min_near_out: 0,
        attached: 1,
    };
    let applied = apply(state, holders, &action, at).ok()?;
    applied.events.iter().find_map(|e| match e {
        Event::FcSell(s) => Some(SellQuote {
            tokens_filled: s.tokens_sold,
            near_out: s.near_out,
            penalty_near: s.penalty_near,
            fill: s.fill,
            limit_reason: s.limit_reason,
            cap_left_after: s.cap_left,
        }),
        _ => None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuyQuote {
    #[serde(with = "crate::decimal_string")]
    pub tokens_out: u128,
    #[serde(with = "crate::decimal_string")]
    pub fee_near: u128,
    #[serde(with = "crate::decimal_string")]
    pub creator_fee_near: u128,
    #[serde(with = "crate::decimal_string")]
    pub marked_tokens: u128,
    pub class_after: &'static str,
    pub becomes_large: bool,
    #[serde(with = "crate::decimal_string")]
    pub allowance_after: u128,
}

/// A preview of one buy now: the same rule step as the real buy, with the result dropped.
pub fn quote_buy(
    state: &CoinState,
    holders: Holders,
    account: &str,
    near_in: u128,
    at: At,
) -> Option<BuyQuote> {
    let action = Action::Buy {
        account: account.to_string(),
        near_in,
        min_tokens_out: 0,
    };
    let applied = apply(state, holders, &action, at).ok()?;
    let allowance_after =
        unmarked_allowance(&applied.state, applied.holders.get(account), at).allowance;
    applied.events.iter().find_map(|e| match e {
        Event::FcBuy(b) => Some(BuyQuote {
            tokens_out: b.tokens_out,
            fee_near: b.fee_near,
            creator_fee_near: b.creator_fee_near,
            marked_tokens: b.marked_tokens,
            class_after: b.class_after,
            becomes_large: applied
                .events
                .iter()
                .any(|e| matches!(e, Event::FcBecameLarge(_))),
            allowance_after,
        }),
        _ => None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreatorFeesView {
    pub creator: String,
    #[serde(with = "crate::decimal_string")]
    pub balance: u128,
    #[serde(with = "crate::decimal_string")]
    pub claimed_total: u128,
    #[serde(with = "crate::decimal_string::opt")]
    pub depth_ok_since_ns: Option<u64>,
    pub claimable_now: bool,
    #[serde(with = "crate::decimal_string::opt")]
    pub claimable_at_ns: Option<u64>,
}

pub fn view_creator_fees(state: &CoinState) -> CreatorFeesView {
    CreatorFeesView {
        creator: state.creator.clone(),
        balance: state.creator_fee_balance,
        claimed_total: 0,
        depth_ok_since_ns: None,
        claimable_now: false,
        claimable_at_ns: None,
    }
}
