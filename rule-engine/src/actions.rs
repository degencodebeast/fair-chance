//! The actions the rule engine applies (buy, sell) and what one action returns.

use crate::allowance::{
    self, count_bought, count_marked_sold, count_sold, unmarked_allowance, unmarked_window_end_ns,
};
use crate::coin_state::{At, CoinState, Holder, Holders};
use crate::creator_group::{creator_cap, in_creator_group};
use crate::early_buy_fee;
use crate::early_supply::{self, marked_allowance, record_first_touch};
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
    early_supply::record_early_supply_window(&mut state, at);
    let state = &state;
    let touched = touched_accounts(action);
    let mut holders = holders;
    for account in &touched {
        if let Some(holder) = holders.get_mut(*account) {
            raise_class_floor(state, holder, at);
            record_first_touch(state, holder, at);
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
    let fee_near = early_buy_fee::buy_fee(state, near_in, at);
    // The creator share is of the normal swap fee only, never of the early-fee extra.
    let creator_fee_near =
        pool_math::creator_fee(pool_math::fee(near_in, state.fixed.swap_fee_bps));
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
    let in_group = in_creator_group(state, account);
    let marked_tokens = early_supply::marked_by_buy(state, in_group, tokens_out, at);
    let holder = holders.entry(account.to_string()).or_default();
    record_first_touch(state, holder, at);
    holder.balance += tokens_out;
    holder.marked_balance += marked_tokens;
    next.marked_supply += marked_tokens;
    if in_group {
        next.creator_group_balance += tokens_out;
        if next.creator_group_balance > creator_cap(state) {
            return Err(Refusal::new(RefusalCode::CreatorCap));
        }
    }
    // Only the unmarked part gives unmarked allowance room back; the marked allowance is gross.
    count_bought(state, holder, tokens_out - marked_tokens, at);
    // The cap counts each buy as the full NEAR paid, fee included.
    let (cap_net, cap_used) =
        real_near_cap::after_buy(state, holder, near_in, tokens_out - marked_tokens, at);
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
            creator_fee_near,
            tokens_out,
            marked_tokens,
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
    // A sell fills marked tokens first, and marked tokens cannot be sold in blocks 0 to N.
    if holder.marked_balance > 0 && early_supply::in_early_window(state, at) {
        return Err(Refusal::new(RefusalCode::EarlyWindow));
    }
    let class = class_of(state, Some(holder), at);
    let fill_plan = SellFillPlan::new(state, holder, class, amount, at);
    let parts = fill_plan.tokens_sold(state, holder, at);
    let tokens_sold = parts.tokens();
    let outcome = sell_outcome(state, holder, &fill_plan, parts, at);
    let fill = if tokens_sold == amount {
        Fill::Full
    } else if tokens_sold == 0 {
        Fill::None
    } else {
        Fill::Partial
    };
    let limit_reason = fill_plan.limit_reason(class, parts);
    if !product_at_least(outcome.near_out, amount, min_near_out, tokens_sold) {
        return Err(Refusal::new(RefusalCode::Slippage));
    }
    let cap_was_full = cap_is_full(state, real_near_cap::cap_used(state, at), state.real_near);
    holder.balance -= tokens_sold;
    holder.marked_balance -= parts.marked;
    holder.marked_share.sold += parts.marked;
    count_sold(state, holder, parts.unmarked, at);
    count_marked_sold(state, holder, parts.marked, at);
    holder.cap_net = outcome.cap_net;
    let after_sell = unmarked_allowance(state, Some(holder), at);
    let mut next = state.clone();
    if clear_large_at_zero(holder) {
        next.large_holders -= 1;
    }
    next.marked_supply -= parts.marked;
    next.early_supply_window.sold += parts.marked;
    if in_creator_group(state, account) {
        next.creator_group_balance -= tokens_sold;
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
        marked_sold: parts.marked,
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

/// The tokens of one sell by kind. The marked part goes into the pool first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SoldParts {
    pub(crate) marked: u128,
    pub(crate) unmarked: u128,
}

impl SoldParts {
    pub(crate) fn tokens(self) -> u128 {
        self.marked + self.unmarked
    }
}

/// How one sell's tokens split by the rules, in tokens. The sell fills the marked part first,
/// up to the account's pro-rata share left, then the unmarked part, up to the account's own
/// allowance. Class rules apply to the unmarked part only.
///
/// - Marked part: the share left is the most that can fill. The cap holds the fill back, but
///   never below the account's part of the shared floor. Every marked token counts toward the
///   cap and none pays the band penalty (early supply never gets the penalty path).
/// - Unmarked part, large: the allowance room is the most that can fill. The cap holds the fill
///   back, but never below the account's floor left (floors stay outside the cap). Every token
///   counts.
/// - Unmarked part, band: the first allowance (the allowance room, with room back for buys)
///   always fills with no penalty. The part above pays the penalty and fills only as far as the
///   cap allows. The part that never counts toward the cap is the allowance less the tokens
///   sold in the window, with no room back for buys (ADR 0005: the bought part counts).
/// - Unmarked part, small: the tokens held from before the window fill and never count. The
///   tokens bought in the window count, so a small sell fills as far as it passes the cap check
///   or adds nothing to the cap used.
///
/// For the cap count only, the unmarked part sells the tokens bought in the current window first
/// (up to `bought_not_sold`); they always count. `never_counted` applies to the tokens after them.
pub(crate) struct SellFillPlan {
    amount: u128,
    class: Class,
    /// Unmarked tokens the account bought in the current cap window and has not sold yet.
    bought_not_sold: u128,
    /// Marked tokens that fill whatever the cap says.
    marked_always_fills: u128,
    /// The most marked tokens that can fill.
    marked_most: u128,
    /// Unmarked tokens that fill whatever the cap says.
    always_fills: u128,
    /// The most unmarked tokens that can fill.
    most: u128,
    /// The first unmarked tokens of the sell that pay no penalty.
    penalty_free: u128,
    /// The unmarked tokens held from before the window that never count toward the cap, first
    /// after the bought tokens.
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
        let marked = marked_allowance(state, Some(holder), at);
        let marked_most = amount.min(holder.marked_balance).min(marked.left);
        let unmarked_asked = (amount - marked_most).min(holder.balance - holder.marked_balance);
        let allowance = unmarked_allowance(state, Some(holder), at);
        let bought_not_sold = real_near_cap::cap_net_now(state, Some(holder), at).bought_not_sold;
        let (always_fills, most, penalty_free, never_counted) = match class {
            Class::Large => {
                let most = unmarked_asked.min(allowance.sell_room);
                (most.min(allowance.floor_left), most, u128::MAX, 0)
            }
            Class::Band => (
                unmarked_asked.min(allowance.sell_room),
                unmarked_asked,
                allowance.sell_room,
                allowance.first_allowance_left,
            ),
            // With no bought tokens left, nothing of a small sell counts, so all of it fills.
            Class::Small if bought_not_sold == 0 => {
                (unmarked_asked, unmarked_asked, u128::MAX, u128::MAX)
            }
            Class::Small => (0, unmarked_asked, u128::MAX, u128::MAX),
        };
        Self {
            amount,
            class,
            bought_not_sold,
            marked_always_fills: marked_most.min(marked.floor_left),
            marked_most,
            always_fills,
            most,
            penalty_free,
            never_counted,
        }
    }

    /// The fill whose cap check passes after the sell: the cap used, with this sell, at most
    /// c x the real NEAR left. The check runs for every limited sell, also when the seller's own
    /// net adds zero. First the largest marked part from `marked_always_fills` to `marked_most`,
    /// with the unmarked part that always fills; then the largest unmarked part from
    /// `always_fills` to `most` on top of that marked part. A small account's unmarked part also
    /// fits when it adds nothing to the cap used (small sells were never held by the check).
    pub(crate) fn tokens_sold(&self, state: &CoinState, holder: &Holder, at: At) -> SoldParts {
        let fits = |parts| {
            let outcome = sell_outcome(state, holder, self, parts, at);
            real_near_cap::within_cap(state, outcome.cap_used, outcome.real_near_left)
        };
        let cap_used_with = |parts| sell_outcome(state, holder, self, parts, at).cap_used;
        let marked = largest_that_fits(self.marked_always_fills, self.marked_most, |marked| {
            fits(SoldParts {
                marked,
                unmarked: self.always_fills,
            })
        });
        let used_by_marked_part = cap_used_with(SoldParts {
            marked,
            unmarked: 0,
        });
        let unmarked = largest_that_fits(self.always_fills, self.most, |unmarked| {
            let parts = SoldParts { marked, unmarked };
            fits(parts)
                || (self.class == Class::Small && cap_used_with(parts) <= used_by_marked_part)
        });
        SoldParts { marked, unmarked }
    }

    fn limit_reason(&self, class: Class, parts: SoldParts) -> Option<&'static str> {
        if parts.tokens() == self.amount {
            None
        } else if parts.marked < self.marked_most {
            if parts.marked == self.marked_always_fills && parts.marked > 0 {
                Some("floor")
            } else {
                Some("cap_full")
            }
        } else if parts.unmarked == self.most {
            Some("allowance")
        } else if class == Class::Large && parts.unmarked == self.always_fills && parts.unmarked > 0
        {
            Some("floor")
        } else {
            Some("cap_full")
        }
    }
}

/// The largest value from `allowed` to `most` that `fits`, where `allowed` always counts as
/// fitting. The check gets no easier as the value grows, so the search runs between a value
/// that is allowed and one that fails.
fn largest_that_fits(allowed: u128, most: u128, fits: impl Fn(u128) -> bool) -> u128 {
    if most == allowed || fits(most) {
        return most;
    }
    let (mut allowed, mut fails) = (allowed, most);
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

/// What a sell of these parts does now.
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
    parts: SoldParts,
    at: At,
) -> SellOutcome {
    let tokens = parts.tokens();
    // NEAR reserve minimum: the payout never takes the NEAR reserve below virtual P.
    let near_before_fee =
        pool_math::near_out_before_fee(state.token_reserve, state.near_reserve(), tokens)
            .min(state.real_near);
    let fee_near = pool_math::fee(near_before_fee, state.fixed.swap_fee_bps);
    // The marked part pays no penalty; it goes into the pool first.
    let penalty_near = penalty_above_first_allowance(
        state,
        parts.marked + parts.unmarked.min(plan.penalty_free),
        near_before_fee,
    );
    let near_out = near_before_fee - fee_near - penalty_near;
    let creator_fee_near = pool_math::creator_fee(fee_near);
    // Cap count order: the marked part, then the tokens bought in the window (both count), then
    // the tokens held from before the window, whose first `never_counted` do not count.
    let bought_sold = parts.unmarked.min(plan.bought_not_sold);
    let held_sold = parts.unmarked - bought_sold;
    let uncounted_out = payout_of_tokens_after(
        state,
        parts.marked + bought_sold,
        held_sold.min(plan.never_counted),
        near_before_fee,
    )
    .min(near_out);
    let marked_out = payout_of_first_tokens_rounded_up(state, parts.marked, near_before_fee)
        .min(near_out - uncounted_out);
    let (cap_net, cap_used) = real_near_cap::after_sell(
        state,
        holder,
        marked_out,
        near_out - uncounted_out - marked_out,
        bought_sold,
        at,
    );
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

/// The NEAR out after the swap fee of the first `tokens` of a sell that pays `near_before_fee` in
/// all, before the fee: the NEAR rounds up and the fee rounds down, so the part never comes out
/// smaller than it is. Used for the marked part, which counts gross, so a larger marked part
/// favors the pool.
fn payout_of_first_tokens_rounded_up(
    state: &CoinState,
    tokens: u128,
    near_before_fee: u128,
) -> u128 {
    if tokens == 0 {
        return 0;
    }
    let before_fee = pool_math::near_out_before_fee_rounded_up(
        state.token_reserve,
        state.near_reserve(),
        tokens,
    )
    .min(near_before_fee);
    before_fee - before_fee * state.fixed.swap_fee_bps as u128 / 10_000
}

/// The NEAR out after the swap fee of the `tokens` that go into the pool right after the first
/// `first` tokens of a sell that pays `near_before_fee` in all, before the fee. The NEAR of the
/// first tokens rounds up and the NEAR up to the end of this part rounds down, so this part's
/// NEAR rounds down: the uncounted part is never larger than it is.
fn payout_of_tokens_after(
    state: &CoinState,
    first: u128,
    tokens: u128,
    near_before_fee: u128,
) -> u128 {
    let (reserve, near) = (state.token_reserve, state.near_reserve());
    let up_to_end =
        pool_math::near_out_before_fee(reserve, near, first + tokens).min(near_before_fee);
    let up_to_start = if first == 0 {
        0
    } else {
        pool_math::near_out_before_fee_rounded_up(reserve, near, first).min(near_before_fee)
    };
    let before_fee = up_to_end.saturating_sub(up_to_start);
    before_fee - pool_math::fee(before_fee, state.fixed.swap_fee_bps)
}

/// The band penalty (ADR 0004): `penalty_bps` of the NEAR output of the part of a sell above the
/// first allowance. That part gets the NEAR the whole sell pays (`near_before_fee`) minus the
/// NEAR the tokens before it alone would pay (the marked part and the first allowance), both
/// before the swap fee; its NEAR output is that amount minus its own swap fee. Both roundings go
/// in the pool's favour. The penalty stays in the pool. Zero when the whole sell is inside the
/// marked part and the first allowance.
fn penalty_above_first_allowance(
    state: &CoinState,
    penalty_free_part: u128,
    near_before_fee: u128,
) -> u128 {
    let penalty_free_near = pool_math::near_out_before_fee(
        state.token_reserve,
        state.near_reserve(),
        penalty_free_part,
    )
    .min(near_before_fee);
    let above_before_fee = near_before_fee - penalty_free_near;
    let above_near_output =
        above_before_fee - pool_math::fee(above_before_fee, state.fixed.swap_fee_bps);
    pool_math::fee(above_near_output, state.fixed.penalty_bps)
}

/// A send (NEP-141 `ft_transfer`). Refused to yourself, for 0 tokens, to the coin, from a large
/// account, above the sender's balance, to an account with no storage record, and when it would
/// take a receiver that is not large to T, and with `FC_CREATOR_CAP` when it would take the creator
/// group above its cap. A send counts toward no allowance. Marked tokens leave first, and every
/// token the creator group receives is marked.
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
    let from_in_group = in_creator_group(state, from);
    let to_in_group = in_creator_group(state, to);
    let mut next = state.clone();
    if from_in_group {
        next.creator_group_balance -= amount;
    }
    if to_in_group {
        next.creator_group_balance += amount;
        if next.creator_group_balance > creator_cap(state) {
            return Err(Refusal::new(RefusalCode::CreatorCap));
        }
    }
    // Marked tokens leave first. Every token the creator group receives is marked.
    let marked_amount = amount.min(sender.marked_balance);
    let to_marked_amount = if to_in_group { amount } else { marked_amount };
    next.marked_supply = next.marked_supply - marked_amount + to_marked_amount;
    if let Some(sender) = holders.get_mut(from) {
        sender.balance -= amount;
        sender.marked_balance -= marked_amount;
    }
    if let Some(receiver) = holders.get_mut(to) {
        receiver.balance += amount;
        receiver.marked_balance += to_marked_amount;
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
            marked_amount,
            to_marked_amount,
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

/// The largest sell up to `target` tokens that fills in full now (`fc_account.max_full_fill_now`).
///
/// A sell of `target` fills some part. When the cap holds back the marked part, a larger sell
/// only gives more of its tokens to the marked part, which the cap holds back again; so the
/// marked fill alone is the answer, and a sell of it fills in full. When only the unmarked part
/// is held back, a sell of the filled part keeps the same marked part and fills in full. Each
/// case needs one more step at most; the bound on the steps only guards the loop.
pub(crate) fn largest_full_fill(
    state: &CoinState,
    holder: &Holder,
    class: Class,
    target: u128,
    at: At,
) -> u128 {
    let mut target = target;
    for _ in 0..8 {
        let plan = SellFillPlan::new(state, holder, class, target, at);
        let parts = plan.tokens_sold(state, holder, at);
        if parts.tokens() == target {
            return target;
        }
        target = if parts.marked < plan.marked_most {
            parts.marked
        } else {
            parts.tokens()
        };
    }
    target
}
