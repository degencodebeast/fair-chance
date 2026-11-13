//! The coin state the rule engine reads and returns: global pool state and per-account records.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::events::{Event, FtMint};
use crate::fixed_values::FixedValues;
use crate::refusals::{Refusal, RefusalCode};

const SECOND_NS: u64 = 1_000_000_000;
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
}

impl CoinState {
    pub fn near_reserve(&self) -> u128 {
        self.fixed.virtual_near + self.real_near
    }

    /// End of the 1 h window that holds `at`, on the fixed grid from launch.
    pub fn unmarked_window_end_ns(&self, at: At) -> u64 {
        let len = self.fixed.window_unmarked_s * SECOND_NS;
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
    };
    let events = vec![Event::FtMint(FtMint {
        owner_id: request.coin.to_string(),
        amount: supply,
    })];
    Ok(Launched { state, events })
}
