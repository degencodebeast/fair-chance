//! The actions the rule engine applies (buy, sell) and what one action returns.

use crate::allowance::{
    self, count_bought, count_sold, unmarked_allowance, unmarked_window_end_ns,
};
use crate::coin_state::{At, CoinState, Holders};
use crate::events::{Event, FcBecameLarge, FcBuy, FcSell, FcSend, FtTransfer};
use crate::holder_class::{
    class_of, clear_large_at_zero, large_threshold, mark_large_after_buy, max_receive, Class,
};
use crate::pool_math;
use crate::refusals::{Refusal, RefusalCode};
use crate::wide_integer_math::product_at_least;

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
    /// A NEP-141 `ft_transfer` from `from` to `to`. The receiver must hold a storage record.
    Send {
        from: String,
        to: String,
        amount: u128,
        memo: Option<String>,
    },
}

impl Action {
    /// The accounts whose [`Holder`] records the caller must pass to [`crate::apply`].
    pub fn accounts(&self) -> Vec<String> {
        match self {
            Self::Buy { account, .. } | Self::Sell { account, .. } => vec![account.clone()],
            Self::Send { from, to, .. } => vec![from.clone(), to.clone()],
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
    // The first action of a window records the token reserve before it changes the pool.
    let mut state = state.clone();
    allowance::record_window_reserve(&mut state, at);
    let state = &state;
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
        Action::Send {
            from,
            to,
            amount,
            memo,
        } => send(state, holders, from, to, *amount, memo.clone(), at),
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
    let fee_near = pool_math::fee(near_in, state.fixed.swap_fee_bps);
    let tokens_out = pool_math::tokens_out(
        state.token_reserve,
        state.near_reserve(),
        near_in - fee_near,
    );
    if tokens_out < min_tokens_out {
        return Err(Refusal::new(RefusalCode::Slippage));
    }
    let mut next = state.clone();
    next.token_reserve -= tokens_out;
    next.real_near += near_in;
    let holder = holders.entry(account.to_string()).or_default();
    holder.balance += tokens_out;
    count_bought(state, holder, tokens_out, at);
    let crossed = mark_large_after_buy(state, holder);
    if crossed {
        next.large_holders += 1;
    }
    let balance = holder.balance;
    let class_after = class_of(&next, Some(holder), at);

    let mut events = vec![
        Event::FtTransfer(FtTransfer {
            old_owner_id: state.coin.clone(),
            new_owner_id: account.to_string(),
            amount: tokens_out,
            memo: None,
        }),
        Event::FcBuy(FcBuy {
            account: account.to_string(),
            near_in,
            fee_near,
            creator_fee_near: 0,
            tokens_out,
            marked_tokens: 0,
            class_after: class_after.as_str(),
            block: at.block,
        }),
    ];
    if crossed {
        events.push(Event::FcBecameLarge(FcBecameLarge {
            account: account.to_string(),
            balance,
            large_threshold: large_threshold(state),
        }));
    }
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
    // A large account is held to its allowance by a partial fill. A band account's first
    // allowance always fills, and the part above it fills with a penalty. Small sells fill in
    // full.
    let class = class_of(state, Some(holder), at);
    let room = unmarked_allowance(state, Some(holder), at).sell_room;
    let (tokens_sold, fill, limit_reason) = if class == Class::Large && amount > room {
        let fill = if room == 0 { Fill::None } else { Fill::Partial };
        (room, fill, Some("allowance"))
    } else {
        (amount, Fill::Full, None)
    };
    let first_allowance_part = if class == Class::Band {
        tokens_sold.min(room)
    } else {
        tokens_sold
    };
    // NEAR reserve minimum: the payout never takes the NEAR reserve below virtual P.
    let near_before_fee =
        pool_math::near_out_before_fee(state.token_reserve, state.near_reserve(), tokens_sold)
            .min(state.real_near);
    let fee_near = pool_math::fee(near_before_fee, state.fixed.swap_fee_bps);
    let penalty_near = penalty_above_first_allowance(state, first_allowance_part, near_before_fee);
    let near_out = near_before_fee - fee_near - penalty_near;
    if !product_at_least(near_out, amount, min_near_out, tokens_sold) {
        return Err(Refusal::new(RefusalCode::Slippage));
    }
    holder.balance -= tokens_sold;
    count_sold(state, holder, tokens_sold, at);
    let after_sell = unmarked_allowance(state, Some(holder), at);
    let mut next = state.clone();
    if clear_large_at_zero(holder) {
        next.large_holders -= 1;
    }
    next.token_reserve += tokens_sold;
    next.real_near -= near_out;

    let mut events = Vec::new();
    if tokens_sold > 0 {
        events.push(Event::FtTransfer(FtTransfer {
            old_owner_id: account.to_string(),
            new_owner_id: state.coin.clone(),
            amount: tokens_sold,
            memo: None,
        }));
    }
    events.push(Event::FcSell(FcSell {
        account: account.to_string(),
        class: class.as_str(),
        tokens_asked: amount,
        tokens_sold,
        marked_sold: 0,
        near_out,
        fee_near,
        creator_fee_near: 0,
        penalty_near,
        fill: fill.as_str(),
        limit_reason,
        allowance_used: after_sell.used,
        allowance_left: after_sell.left,
        cap_used: 0,
        cap_left: 0,
        window_end_ns: unmarked_window_end_ns(state, at),
    }));
    let payout = (tokens_sold > 0).then(|| Payout {
        account: account.to_string(),
        amount: near_out,
    });
    Ok(Applied {
        state: next,
        holders,
        fill,
        events,
        payout,
    })
}

/// The band penalty (ADR 0004): `penalty_bps` of the NEAR output of the part of a sell above the
/// first allowance. That part gets the NEAR the whole sell pays (`near_before_fee`) minus the
/// NEAR the first-allowance part alone would pay, both before the swap fee; its NEAR output is
/// that amount minus its own swap fee. Both roundings go in the pool's favour. The penalty stays
/// in the pool. Zero when the whole sell is inside the first allowance.
fn penalty_above_first_allowance(
    state: &CoinState,
    first_allowance_part: u128,
    near_before_fee: u128,
) -> u128 {
    let first_allowance_near = pool_math::near_out_before_fee(
        state.token_reserve,
        state.near_reserve(),
        first_allowance_part,
    )
    .min(near_before_fee);
    let above_before_fee = near_before_fee - first_allowance_near;
    let above_near_output =
        above_before_fee - pool_math::fee(above_before_fee, state.fixed.swap_fee_bps);
    pool_math::fee(above_near_output, state.fixed.penalty_bps)
}

/// A send (NEP-141 `ft_transfer`). Refused to yourself, for 0 tokens, to the coin, from a large
/// account, above the sender's balance, to an account with no storage record, and when it would
/// take a receiver that is not large to T. A send counts toward no allowance.
fn send(
    state: &CoinState,
    mut holders: Holders,
    from: &str,
    to: &str,
    amount: u128,
    memo: Option<String>,
    _at: At,
) -> Result<Applied, Refusal> {
    if from == to {
        return Err(Refusal::new(RefusalCode::SendToSelf));
    }
    if amount == 0 {
        return Err(Refusal::new(RefusalCode::AmountNotPositive));
    }
    if to == state.coin {
        return Err(Refusal::new(RefusalCode::SendToCoin));
    }
    let sender = holders.get(from).cloned().unwrap_or_default();
    if sender.is_large {
        return Err(Refusal::new(RefusalCode::LargeCannotSend));
    }
    if amount > sender.balance {
        return Err(Refusal::new(RefusalCode::NotEnoughBalance));
    }
    let Some(receiver) = holders.get(to) else {
        return Err(Refusal::with_detail(RefusalCode::NotRegistered, to));
    };
    if let Some(room) = max_receive(state, Some(receiver)) {
        if amount > room {
            return Err(Refusal::with_detail(
                RefusalCode::ReceiverWouldBeLarge,
                room.to_string(),
            ));
        }
    }
    if let Some(sender) = holders.get_mut(from) {
        sender.balance -= amount;
    }
    if let Some(receiver) = holders.get_mut(to) {
        receiver.balance += amount;
    }
    let events = vec![
        Event::FtTransfer(FtTransfer {
            old_owner_id: from.to_string(),
            new_owner_id: to.to_string(),
            amount,
            memo,
        }),
        Event::FcSend(FcSend {
            from: from.to_string(),
            to: to.to_string(),
            amount,
            marked_amount: 0,
            to_marked_amount: 0,
        }),
    ];
    Ok(Applied {
        state: state.clone(),
        holders,
        fill: Fill::Full,
        events,
        payout: None,
    })
}
