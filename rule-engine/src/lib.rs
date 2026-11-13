//! Fair Chance rule engine.
//!
//! Every coin rule lives here, as plain Rust with no chain types. The coin contract builds an
//! [`Action`], loads the [`Holder`] records that [`Action::accounts`] names, calls [`apply`], and
//! stores and logs what comes back. Views are pure functions of the same inputs.

mod actions;
mod coin_state;
pub mod decimal_string;
mod events;
mod fixed_values;
mod pool_math;
mod refusals;
mod views;
mod wide_integer_math;

pub use actions::{Action, Applied, Fill, Payout};
pub use coin_state::{
    launch, At, CoinMetadata, CoinState, Holder, Holders, LaunchRequest, Launched,
};
pub use events::{Event, FcBuy, FcSell, FtMint, FtTransfer};
pub use fixed_values::FixedValues;
pub use refusals::{Refusal, RefusalCode};
pub use views::{
    quote_buy, quote_sell, view_account, view_cap, view_creator_fees, view_early_window, view_pool,
    view_rules, AccountView, BuyQuote, CapView, CreatorFeesView, EarlyWindowView, PoolView,
    RulesView, SellQuote,
};

/// Version string of the coin interface (INTERFACE.md) and of every `fair_chance` event.
pub const INTERFACE_VERSION: &str = "1.0.0-rc.5";

/// The pure rule step: one action on the coin state at one block.
///
/// The input state and holders are not changed. On success the caller stores
/// `Applied::state` and `Applied::holders`, logs `Applied::events` in order and sends
/// `Applied::payout`. On refusal the caller panics with the refusal's display string.
pub fn apply(
    state: &CoinState,
    holders: Holders,
    action: &Action,
    at: At,
) -> Result<Applied, Refusal> {
    actions::apply(state, holders, action, at)
}
