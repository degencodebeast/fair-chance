//! The coin state the rule engine reads and returns: global pool state and per-account records.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::allowance::{WindowReserve, WindowTotals};
use crate::early_supply::{EarlySupplyWindow, MarkedShare};
use crate::events::{Event, FtMint};
use crate::fixed_values::FixedValues;
use crate::real_near_cap::{CapNet, CapWindow};
use crate::refusals::{Refusal, RefusalCode};

const ICON_PREFIX: &str = "data:image/webp;base64,";
/// The icon data URL must be under 8 KB.
const ICON_MAX_BYTES: usize = 8 * 1024 - 1;

/// Block height and block timestamp (ns) of the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct At {
    pub block: u64,
    pub ns: u64,
}

/// What the creator gave at launch. Set once at `new`; no action changes it.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CoinMetadata {
    pub name: String,
    pub symbol: String,
    /// NEP-148 icon: a WebP data URL.
    pub icon: Option<String>,
    /// The one-line launch description (returned by `fc_rules`).
    pub description: Option<String>,
}

/// The coin's global state: fixed values, launch facts and the pool.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CoinState {
    pub fixed: FixedValues,
    pub coin: String,
    pub creator: String,
    /// Code hash of the global contract this coin uses, as the chain writes it.
    pub code_hash: String,
    pub metadata: CoinMetadata,
    pub launch_block: u64,
    pub launch_ns: u64,
    pub token_reserve: u128,
    pub real_near: u128,
    pub creator_fee_balance: u128,
    /// The token reserve recorded at the first action of the current or last 1 h window.
    pub unmarked_window_reserve: WindowReserve,
    /// Number of accounts whose class is `large` now.
    pub large_holders: u32,
    /// The real NEAR cap used in the current or last 1 h window.
    pub cap_window: CapWindow,
    /// All marked tokens that accounts hold now.
    pub marked_supply: u128,
    /// Tokens the creator group holds now.
    pub creator_group_balance: u128,
    /// The shared early-supply window recorded at its first action after the early window.
    pub early_supply_window: EarlySupplyWindow,
}

impl CoinState {
    pub fn near_reserve(&self) -> u128 {
        self.fixed.virtual_near + self.real_near
    }
}

/// One account's record. A missing record is a new account with zero balance.
#[derive(Debug, Clone, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Holder {
    pub balance: u128,
    /// Unmarked tokens sold and bought in the account's last 1 h window with an action.
    pub unmarked_window: WindowTotals,
    /// Set by the buy that takes the balance to T; cleared when the balance is zero.
    pub is_large: bool,
    /// yoctoNEAR the account paid for this record under NEP-145 storage; 0 when a buy made it.
    pub storage_deposit: u128,
    /// The account's net NEAR out for the real NEAR cap in its last 1 h window with a trade.
    pub cap_net: CapNet,
    /// The lowest class the account can have in its last 1 h window with an action.
    pub class_floor: ClassFloor,
    /// The marked part of `balance`: early supply.
    pub marked_balance: u128,
    /// The account's pro-rata share record in its last 24 h window with a touch.
    pub marked_share: MarkedShare,
}

/// The class floor (ADR 0001): in a window, the account's class is never below its class just
/// before its first action in that window. A buy or a receive can raise it; a sell or a send
/// cannot drop it until the next window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ClassFloor {
    pub window: u64,
    /// True when the account was band or large at some point in the window, from just before
    /// its first action on.
    pub band: bool,
}

impl Holder {
    /// An empty record, made by a NEP-145 storage deposit of `storage_deposit` yoctoNEAR.
    pub fn registered(storage_deposit: u128) -> Self {
        Self {
            storage_deposit,
            ..Self::default()
        }
    }
}

/// The records of the accounts an action touches, by account ID.
pub type Holders = BTreeMap<String, Holder>;

/// The inputs of `new`. `caller` is the account that calls `new`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchRequest<'a> {
    pub coin: &'a str,
    pub caller: &'a str,
    pub creator: &'a str,
    pub code_hash: &'a str,
    pub name: &'a str,
    pub symbol: &'a str,
    pub icon: Option<&'a str>,
    pub description: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launched {
    pub state: CoinState,
    pub events: Vec<Event>,
}

/// The coin's parent account: `factory.near` for `walk.factory.near`. A top-level account has
/// none.
fn parent_account(account: &str) -> Option<&str> {
    account.split_once('.').map(|(_, parent)| parent)
}

fn icon_is_small_webp(icon: &str) -> bool {
    let Some(base64) = icon.strip_prefix(ICON_PREFIX) else {
        return false;
    };
    icon.len() <= ICON_MAX_BYTES
        && !base64.is_empty()
        && base64
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
}

/// The coin at `new`: the whole supply minted once into the pool. Refused unless the caller is
/// the coin's parent account (the factory) and the icon, if any, is a WebP data URL under 8 KB.
pub fn launch(fixed: FixedValues, request: LaunchRequest<'_>, at: At) -> Result<Launched, Refusal> {
    if parent_account(request.coin) != Some(request.caller) {
        return Err(Refusal::new(RefusalCode::NotParentAccount));
    }
    if request.icon.is_some_and(|icon| !icon_is_small_webp(icon)) {
        return Err(Refusal::new(RefusalCode::IconNotSmallWebp));
    }
    let supply = fixed.total_supply;
    let state = CoinState {
        fixed,
        coin: request.coin.to_string(),
        creator: request.creator.to_string(),
        code_hash: request.code_hash.to_string(),
        metadata: CoinMetadata {
            name: request.name.to_string(),
            symbol: request.symbol.to_string(),
            icon: request.icon.map(str::to_string),
            description: request.description.map(str::to_string),
        },
        launch_block: at.block,
        launch_ns: at.ns,
        token_reserve: supply,
        real_near: 0,
        creator_fee_balance: 0,
        unmarked_window_reserve: WindowReserve {
            window: 0,
            token_reserve: supply,
        },
        large_holders: 0,
        cap_window: CapWindow::default(),
        marked_supply: 0,
        creator_group_balance: 0,
        early_supply_window: EarlySupplyWindow::default(),
    };
    let events = vec![Event::FtMint(FtMint {
        owner_id: request.coin.to_string(),
        amount: supply,
    })];
    Ok(Launched { state, events })
}
