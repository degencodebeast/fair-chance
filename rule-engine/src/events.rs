use serde::Serialize;
use serde_json::json;

use crate::INTERFACE_VERSION;

/// Every event the coin emits, in the order the rule step produced them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    FcBuy(FcBuy),
    FcSell(FcSell),
    FtMint(FtMint),
    FtTransfer(FtTransfer),
}

impl Event {
    /// The NEP-297 log line, `EVENT_JSON:` included.
    pub fn log_line(&self) -> String {
        let (standard, version, event, data) = match self {
            Self::FcBuy(e) => ("fair_chance", INTERFACE_VERSION, "fc_buy", json!([e])),
            Self::FcSell(e) => ("fair_chance", INTERFACE_VERSION, "fc_sell", json!([e])),
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
}
