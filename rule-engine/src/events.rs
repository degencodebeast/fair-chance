use serde::Serialize;
use serde_json::json;

use crate::INTERFACE_VERSION;

/// Every event the coin emits, in the order the rule step produced them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    FcBuy(FcBuy),
    FcSell(FcSell),
    FcSend(FcSend),
    FcBecameLarge(FcBecameLarge),
    FcCapFilled(FcCapFilled),
    FtMint(FtMint),
    FtTransfer(FtTransfer),
}

impl Event {
    /// The NEP-297 log line, `EVENT_JSON:` included.
    pub fn log_line(&self) -> String {
        let (standard, version, event, data) = match self {
            Self::FcBuy(e) => ("fair_chance", INTERFACE_VERSION, "fc_buy", json!([e])),
            Self::FcSell(e) => ("fair_chance", INTERFACE_VERSION, "fc_sell", json!([e])),
            Self::FcSend(e) => ("fair_chance", INTERFACE_VERSION, "fc_send", json!([e])),
            Self::FcBecameLarge(e) => (
                "fair_chance",
                INTERFACE_VERSION,
                "fc_became_large",
                json!([e]),
            ),
            Self::FcCapFilled(e) => (
                "fair_chance",
                INTERFACE_VERSION,
                "fc_cap_filled",
                json!([e]),
            ),
            Self::FtMint(e) => ("nep141", "1.0.0", "ft_mint", json!([e])),
            Self::FtTransfer(e) => ("nep141", "1.0.0", "ft_transfer", json!([e])),
        };
        let body =
            json!({ "standard": standard, "version": version, "event": event, "data": data });
        format!("EVENT_JSON:{body}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcBuy {
    pub account: String,
    #[serde(with = "crate::decimal_string")]
    pub near_in: u128,
    #[serde(with = "crate::decimal_string")]
    pub fee_near: u128,
    #[serde(with = "crate::decimal_string")]
    pub creator_fee_near: u128,
    #[serde(with = "crate::decimal_string")]
    pub tokens_out: u128,
    #[serde(with = "crate::decimal_string")]
    pub marked_tokens: u128,
    pub class_after: &'static str,
    pub block: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcSell {
    pub account: String,
    pub class: &'static str,
    #[serde(with = "crate::decimal_string")]
    pub tokens_asked: u128,
    #[serde(with = "crate::decimal_string")]
    pub tokens_sold: u128,
    #[serde(with = "crate::decimal_string")]
    pub marked_sold: u128,
    #[serde(with = "crate::decimal_string")]
    pub near_out: u128,
    #[serde(with = "crate::decimal_string")]
    pub fee_near: u128,
    #[serde(with = "crate::decimal_string")]
    pub creator_fee_near: u128,
    #[serde(with = "crate::decimal_string")]
    pub penalty_near: u128,
    pub fill: &'static str,
    pub limit_reason: Option<&'static str>,
    #[serde(with = "crate::decimal_string")]
    pub allowance_used: u128,
    #[serde(with = "crate::decimal_string")]
    pub allowance_left: u128,
    #[serde(with = "crate::decimal_string")]
    pub cap_used: u128,
    #[serde(with = "crate::decimal_string")]
    pub cap_left: u128,
    #[serde(with = "crate::decimal_string")]
    pub window_end_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FtMint {
    pub owner_id: String,
    #[serde(with = "crate::decimal_string")]
    pub amount: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FtTransfer {
    pub old_owner_id: String,
    pub new_owner_id: String,
    #[serde(with = "crate::decimal_string")]
    pub amount: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

/// A send (`ft_transfer`). Marked tokens leave first; until marked tokens exist both marked
/// amounts are 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcSend {
    pub from: String,
    pub to: String,
    #[serde(with = "crate::decimal_string")]
    pub amount: u128,
    #[serde(with = "crate::decimal_string")]
    pub marked_amount: u128,
    #[serde(with = "crate::decimal_string")]
    pub to_marked_amount: u128,
}

/// The buy that takes an account to T. `large_threshold` is T in token base units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcBecameLarge {
    pub account: String,
    #[serde(with = "crate::decimal_string")]
    pub balance: u128,
    #[serde(with = "crate::decimal_string")]
    pub large_threshold: u128,
}

/// The sell that took the real NEAR cap from "has room" to "full" in its window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FcCapFilled {
    #[serde(with = "crate::decimal_string")]
    pub window_start_ns: u64,
    #[serde(with = "crate::decimal_string")]
    pub window_end_ns: u64,
    /// Real NEAR left after the sell.
    #[serde(with = "crate::decimal_string")]
    pub real_near: u128,
    #[serde(with = "crate::decimal_string")]
    pub cap_total: u128,
    #[serde(with = "crate::decimal_string")]
    pub cap_used: u128,
    pub by_account: String,
    /// 1, 2, ... in the window.
    pub fill_index: u32,
}
