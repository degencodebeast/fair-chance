//! The coin state the rule engine reads and returns: global pool state and per-account records.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::events::{Event, FtMint};
use crate::fixed_values::FixedValues;

const SECOND_NS: u64 = 1_000_000_000;

/// Block height and block timestamp (ns) of the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct At {
    pub block: u64,
    pub ns: u64,
}

/// The coin's global state: fixed values, launch facts and the pool.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CoinState {
    pub fixed: FixedValues,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launched {
    pub state: CoinState,
    pub events: Vec<Event>,
}

/// The coin at `new`: the whole supply minted once into the pool.
pub fn launch(fixed: FixedValues, coin: &str, creator: &str, code_hash: &str, at: At) -> Launched {
    let supply = fixed.total_supply;
    let state = CoinState {
        fixed,
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
