//! The actions the rule engine applies (buy, sell) and what one action returns.

use crate::coin_state::{At, CoinState, Holders};
use crate::events::{Event, FcBuy, FcSell, FtTransfer};
use crate::refusals::{Refusal, RefusalCode};
use crate::wide_integer_math::{mul_div, product_at_least};

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
