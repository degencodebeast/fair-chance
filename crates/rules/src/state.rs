use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::events::{Event, FcBuy, FcSell, FtMint, FtTransfer};
use crate::math::{mul_div, product_at_least};
use crate::refusal::{Refusal, RefusalCode};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const TOKEN_DECIMALS: u8 = 18;
const SECOND_NS: u64 = 1_000_000_000;

/// Block height and block timestamp (ns) of the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct At {
    pub block: u64,
    pub ns: u64,
}

/// The fixed values of every coin. One set for all coins; nobody can change them.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Rules {
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

impl Rules {
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

/// The coin's global state: rules, launch facts and the pool.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CoinState {
    pub rules: Rules,
    pub coin: String,
    pub creator: String,
    /// Code hash of the global contract this coin uses, as the chain writes it.
    pub code_hash: String,
    pub launch_block: u64,
    pub launch_ns: u64,
    pub token_reserve: u128,
    pub real_near: u128,
    pub creator_fee_balance: u128,
}

impl CoinState {
    pub fn near_reserve(&self) -> u128 {
        self.rules.virtual_near + self.real_near
    }

    /// End of the 1 h window that holds `at`, on the fixed grid from launch.
    pub fn unmarked_window_end_ns(&self, at: At) -> u64 {
        let len = self.rules.window_unmarked_s * SECOND_NS;
        let k = at.ns.saturating_sub(self.launch_ns) / len;
        self.launch_ns + (k + 1) * len
    }
}

/// One account's record. A missing record is a new account with zero balance.
#[derive(Debug, Clone, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Holder {
    pub balance: u128,
}

/// The records of the accounts an action touches, by account ID.
pub type Holders = BTreeMap<String, Holder>;

/// One action on the coin. Accounts are account IDs; amounts are base units or yoctoNEAR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Buy {
        account: String,
        near_in: u128,
        min_tokens_out: u128,
    },
    /// `min_near_out` is the NEAR the seller accepts for all of `amount`; the check is on the
    /// price per token of the filled part.
    Sell {
        account: String,
        amount: u128,
        min_near_out: u128,
        attached: u128,
    },
}

impl Action {
    /// The accounts whose [`Holder`] records the caller must pass to [`crate::apply`].
    pub fn accounts(&self) -> Vec<String> {
        match self {
            Self::Buy { account, .. } | Self::Sell { account, .. } => vec![account.clone()],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    Full,
    Partial,
    None,
}

impl Fill {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
            Self::None => "none",
        }
    }
}

/// NEAR the contract must send after the action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payout {
    pub account: String,
    pub amount: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub state: CoinState,
    pub holders: Holders,
    pub fill: Fill,
    pub events: Vec<Event>,
    pub payout: Option<Payout>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launched {
    pub state: CoinState,
    pub events: Vec<Event>,
}

/// The coin at `new`: the whole supply minted once into the pool.
pub fn launch(rules: Rules, coin: &str, creator: &str, code_hash: &str, at: At) -> Launched {
    let supply = rules.total_supply;
    let state = CoinState {
        rules,
        coin: coin.to_string(),
        creator: creator.to_string(),
        code_hash: code_hash.to_string(),
        launch_block: at.block,
        launch_ns: at.ns,
        token_reserve: supply,
        real_near: 0,
        creator_fee_balance: 0,
    };
    let events = vec![Event::FtMint(FtMint {
        owner_id: coin.to_string(),
        amount: supply,
    })];
    Launched { state, events }
}

pub fn apply(
    state: &CoinState,
    holders: Holders,
    action: &Action,
    at: At,
) -> Result<Applied, Refusal> {
    match action {
        Action::Buy {
            account,
            near_in,
            min_tokens_out,
        } => buy(state, holders, account, *near_in, *min_tokens_out, at),
        Action::Sell {
            account,
            amount,
            min_near_out,
            attached,
        } => sell(
            state,
            holders,
            account,
            *amount,
            *min_near_out,
            *attached,
            at,
        ),
    }
}

fn buy(
    state: &CoinState,
    mut holders: Holders,
    account: &str,
    near_in: u128,
    min_tokens_out: u128,
    at: At,
) -> Result<Applied, Refusal> {
    let tokens_out = mul_div(state.token_reserve, near_in, state.near_reserve() + near_in);
    if tokens_out < min_tokens_out {
        return Err(Refusal::new(RefusalCode::Slippage));
    }
    let mut next = state.clone();
    next.token_reserve -= tokens_out;
    next.real_near += near_in;
    holders.entry(account.to_string()).or_default().balance += tokens_out;

    let events = vec![
        Event::FtTransfer(FtTransfer {
            old_owner_id: state.coin.clone(),
            new_owner_id: account.to_string(),
            amount: tokens_out,
        }),
        Event::FcBuy(FcBuy {
            account: account.to_string(),
            near_in,
            fee_near: 0,
            creator_fee_near: 0,
            tokens_out,
            marked_tokens: 0,
            class_after: "small",
            block: at.block,
        }),
    ];
    Ok(Applied {
        state: next,
        holders,
        fill: Fill::Full,
        events,
        payout: None,
    })
}

fn sell(
    state: &CoinState,
    mut holders: Holders,
    account: &str,
    amount: u128,
    min_near_out: u128,
    attached: u128,
    at: At,
) -> Result<Applied, Refusal> {
    if attached != 1 {
        return Err(Refusal::new(RefusalCode::OneYocto));
    }
    let holder = holders.entry(account.to_string()).or_default();
    if amount == 0 || amount > holder.balance {
        return Err(Refusal::new(RefusalCode::NotEnoughBalance));
    }
    let tokens_sold = amount;
    let near_out = mul_div(
        state.near_reserve(),
        tokens_sold,
        state.token_reserve + tokens_sold,
    )
    .min(state.real_near);
    if !product_at_least(near_out, amount, min_near_out, tokens_sold) {
        return Err(Refusal::new(RefusalCode::Slippage));
    }
    holder.balance -= tokens_sold;
    let mut next = state.clone();
    next.token_reserve += tokens_sold;
    next.real_near -= near_out;

    let events = vec![
        Event::FtTransfer(FtTransfer {
            old_owner_id: account.to_string(),
            new_owner_id: state.coin.clone(),
            amount: tokens_sold,
        }),
        Event::FcSell(FcSell {
            account: account.to_string(),
            class: "small",
            tokens_asked: amount,
            tokens_sold,
            marked_sold: 0,
            near_out,
            fee_near: 0,
            creator_fee_near: 0,
            penalty_near: 0,
            fill: Fill::Full.as_str(),
            limit_reason: None,
            allowance_used: 0,
            allowance_left: 0,
            cap_used: 0,
            cap_left: 0,
            window_end_ns: state.unmarked_window_end_ns(at),
        }),
    ];
    let payout = Some(Payout {
        account: account.to_string(),
        amount: near_out,
    });
    Ok(Applied {
        state: next,
        holders,
        fill: Fill::Full,
        events,
        payout,
    })
}
