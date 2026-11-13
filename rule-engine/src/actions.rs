//! The actions the rule engine applies (buy, sell) and what one action returns.

use crate::allowance::{
    self, count_bought, count_sold, unmarked_allowance, unmarked_window_end_ns,
};
use crate::coin_state::{At, CoinState, Holder, Holders};
use crate::events::{Event, FcBecameLarge, FcBuy, FcCapFilled, FcSell, FcSend, FtTransfer};
use crate::holder_class::{
    class_of, clear_large_at_zero, large_threshold, mark_large_after_buy, max_receive,
    raise_class_floor, Class,
};
use crate::pool_math;
use crate::real_near_cap::{self, CapNet};
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
    real_near_cap::record_cap_window(&mut state, at);
    let state = &state;
    let touched = touched_accounts(action);
    let mut holders = holders;
    for account in &touched {
        if let Some(holder) = holders.get_mut(*account) {
            raise_class_floor(state, holder, at);
        }
    }
    let mut applied = match action {
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
    }?;
    for account in &touched {
        if let Some(holder) = applied.holders.get_mut(*account) {
            raise_class_floor(&applied.state, holder, at);
        }
    }
    Ok(applied)
}

/// The accounts whose balance an action can change: the buyer, the seller, or the sender and the
/// receiver.
fn touched_accounts(action: &Action) -> Vec<&str> {
    match action {
        Action::Buy { account, .. } | Action::Sell { account, .. } => vec![account.as_str()],
        Action::Send { from, to, .. } => vec![from.as_str(), to.as_str()],
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
    // The cap counts each buy as the full NEAR paid, fee included.
    let (cap_net, cap_used) = real_near_cap::after_buy(state, holder, near_in, at);
    holder.cap_net = cap_net;
    next.cap_window.used = cap_used;
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
    let class = class_of(state, Some(holder), at);
    let fill_plan = SellFillPlan::new(state, holder, class, amount, at);
    let tokens_sold = fill_plan.tokens_sold(state, holder, at);
    let outcome = sell_outcome(state, holder, &fill_plan, tokens_sold, at);
    let fill = if tokens_sold == amount {
        Fill::Full
    } else if tokens_sold == 0 {
        Fill::None
    } else {
        Fill::Partial
    };
    let limit_reason = fill_plan.limit_reason(class, tokens_sold);
    if !product_at_least(outcome.near_out, amount, min_near_out, tokens_sold) {
        return Err(Refusal::new(RefusalCode::Slippage));
    }
    let cap_was_full = cap_is_full(state, real_near_cap::cap_used(state, at), state.real_near);
    holder.balance -= tokens_sold;
    count_sold(state, holder, tokens_sold, at);
    holder.cap_net = outcome.cap_net;
    let after_sell = unmarked_allowance(state, Some(holder), at);
    let mut next = state.clone();
    if clear_large_at_zero(holder) {
        next.large_holders -= 1;
    }
    next.token_reserve += tokens_sold;
    next.real_near = outcome.real_near_left;
    next.creator_fee_balance += outcome.creator_fee_near;
    next.cap_window.used = outcome.cap_used;
    let cap_left_after = real_near_cap::cap_left(
        &next,
        outcome.cap_used,
        next.real_near,
        pool_math::applied_creator_fee_share_bps(),
    );
    let window_end_ns = unmarked_window_end_ns(state, at);

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
        near_out: outcome.near_out,
        fee_near: outcome.fee_near,
        creator_fee_near: outcome.creator_fee_near,
        penalty_near: outcome.penalty_near,
        fill: fill.as_str(),
        limit_reason,
        allowance_used: after_sell.used,
        allowance_left: after_sell.left,
        cap_used: outcome.cap_used,
        cap_left: cap_left_after,
        window_end_ns,
    }));
    if !cap_was_full && cap_left_after == 0 {
        next.cap_window.fills += 1;
        events.push(Event::FcCapFilled(FcCapFilled {
            window_start_ns: window_end_ns - state.fixed.window_unmarked_s * 1_000_000_000,
            window_end_ns,
            real_near: next.real_near,
            cap_total: real_near_cap::cap_total(&next, next.real_near),
            cap_used: outcome.cap_used,
            by_account: account.to_string(),
            fill_index: next.cap_window.fills,
        }));
    }
    let payout = (tokens_sold > 0).then(|| Payout {
        account: account.to_string(),
        amount: outcome.near_out,
    });
    Ok(Applied {
        state: next,
        holders,
        fill,
        events,
        payout,
    })
}

/// True when one more counted sell can take no NEAR now (`fc_cap.is_full`).
pub(crate) fn cap_is_full(state: &CoinState, used: u128, real_near: u128) -> bool {
    real_near_cap::cap_left(
        state,
        used,
        real_near,
        pool_math::applied_creator_fee_share_bps(),
    ) == 0
}

/// How one sell's tokens split by the class rules, in tokens.
///
/// - Large: the allowance room is the most that can fill. The cap holds the fill back, but never
///   below the account's floor left (floors stay outside the cap). Every token counts.
/// - Band: the first allowance (the allowance room, with room back for buys) always fills with
///   no penalty. The part above pays the penalty and fills only as far as the cap allows. The
///   part that never counts toward the cap is the allowance less the tokens sold in the window,
///   with no room back for buys (ADR 0005: the bought part counts).
/// - Small: everything fills, and nothing counts.
pub(crate) struct SellFillPlan {
    amount: u128,
    /// Tokens that fill whatever the cap says.
    always_fills: u128,
    /// The most tokens that can fill.
    most: u128,
    /// The first tokens of the sell that pay no penalty.
    penalty_free: u128,
    /// The first tokens of the sell that never count toward the cap.
    never_counted: u128,
}

impl SellFillPlan {
    pub(crate) fn new(
        state: &CoinState,
        holder: &Holder,
        class: Class,
        amount: u128,
        at: At,
    ) -> Self {
        let allowance = unmarked_allowance(state, Some(holder), at);
        match class {
            Class::Large => {
                let most = amount.min(allowance.sell_room);
                Self {
                    amount,
                    always_fills: most.min(allowance.floor_left),
                    most,
                    penalty_free: u128::MAX,
                    never_counted: 0,
                }
            }
            Class::Band => Self {
                amount,
                always_fills: amount.min(allowance.sell_room),
                most: amount,
                penalty_free: allowance.sell_room,
                never_counted: allowance.first_allowance_left,
            },
            Class::Small => Self {
                amount,
                always_fills: amount,
                most: amount,
                penalty_free: u128::MAX,
                never_counted: u128::MAX,
            },
        }
    }

    /// The largest fill from `always_fills` to `most` whose cap check passes after the sell:
    /// the cap used, with this sell, at most c x the real NEAR left. The check runs for every
    /// limited sell, also when the seller's own net adds zero.
    pub(crate) fn tokens_sold(&self, state: &CoinState, holder: &Holder, at: At) -> u128 {
        let fits = |tokens| {
            let outcome = sell_outcome(state, holder, self, tokens, at);
            real_near_cap::within_cap(state, outcome.cap_used, outcome.real_near_left)
        };
        if self.most == self.always_fills || fits(self.most) {
            return self.most;
        }
        // The check gets no easier as the sell grows, so search between a fill that is allowed
        // and one that fails.
        let (mut allowed, mut fails) = (self.always_fills, self.most);
        while fails - allowed > 1 {
            let middle = allowed + (fails - allowed) / 2;
            if fits(middle) {
                allowed = middle;
            } else {
                fails = middle;
            }
        }
        allowed
    }

    fn limit_reason(&self, class: Class, tokens_sold: u128) -> Option<&'static str> {
        if tokens_sold == self.amount {
            None
        } else if tokens_sold == self.most {
            Some("allowance")
        } else if class == Class::Large && tokens_sold == self.always_fills && tokens_sold > 0 {
            Some("floor")
        } else {
            Some("cap_full")
        }
    }
}

/// What a sell of `tokens` does now.
struct SellOutcome {
    near_out: u128,
    fee_near: u128,
    creator_fee_near: u128,
    penalty_near: u128,
    cap_net: CapNet,
    /// The pool's cap used after the sell.
    cap_used: u128,
    real_near_left: u128,
}

fn sell_outcome(
    state: &CoinState,
    holder: &Holder,
    plan: &SellFillPlan,
    tokens: u128,
    at: At,
) -> SellOutcome {
    // NEAR reserve minimum: the payout never takes the NEAR reserve below virtual P.
    let near_before_fee =
        pool_math::near_out_before_fee(state.token_reserve, state.near_reserve(), tokens)
            .min(state.real_near);
    let fee_near = pool_math::fee(near_before_fee, state.fixed.swap_fee_bps);
    let penalty_near =
        penalty_above_first_allowance(state, tokens.min(plan.penalty_free), near_before_fee);
    let near_out = near_before_fee - fee_near - penalty_near;
    let creator_fee_near = pool_math::creator_fee(fee_near);
    let uncounted_out =
        payout_of_first_tokens(state, tokens.min(plan.never_counted), near_before_fee)
            .min(near_out);
    let (cap_net, cap_used) =
        real_near_cap::after_sell(state, holder, uncounted_out, near_out - uncounted_out, at);
    SellOutcome {
        near_out,
        fee_near,
        creator_fee_near,
        penalty_near,
        cap_net,
        cap_used,
        real_near_left: state.real_near - near_out - creator_fee_near,
    }
}

/// The NEAR out after the swap fee of the first `tokens` of a sell that pays `near_before_fee`
/// in all, before the fee.
fn payout_of_first_tokens(state: &CoinState, tokens: u128, near_before_fee: u128) -> u128 {
    let before_fee =
        pool_math::near_out_before_fee(state.token_reserve, state.near_reserve(), tokens)
            .min(near_before_fee);
    before_fee - pool_math::fee(before_fee, state.fixed.swap_fee_bps)
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
