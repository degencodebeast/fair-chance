//! Short text for token and NEAR amounts in the demo output. Every number is cut, not rounded.

use anyhow::{Context, Result};
use serde_json::Value;

/// Token base units in one whole token (18 decimals).
pub const TOKEN_UNITS: u128 = 1_000_000_000_000_000_000;
/// The fixed supply in whole tokens.
pub const SUPPLY_WHOLE_TOKENS: u128 = 1_000_000_000;
const YOCTO_PER_NEAR: u128 = 1_000_000_000_000_000_000_000_000;

/// A decimal-string amount from an event or a view.
pub fn amount(value: &Value) -> Result<u128> {
    Ok(value
        .as_str()
        .with_context(|| format!("{value} is not a decimal string"))?
        .parse()?)
}

/// Tokens as 38.4M, 429.8K or 12.
pub fn tokens(units: u128) -> String {
    let whole = units / TOKEN_UNITS;
    if whole >= 1_000_000 {
        let tenths = units / (TOKEN_UNITS * 100_000);
        format!("{}.{}M", tenths / 10, tenths % 10)
    } else if whole >= 1_000 {
        let tenths = units / (TOKEN_UNITS * 100);
        format!("{}.{}K", tenths / 10, tenths % 10)
    } else {
        whole.to_string()
    }
}

/// yoctoNEAR as NEAR with 4 decimals: 1.7458 NEAR.
pub fn near(yocto: u128) -> String {
    let part = yocto % YOCTO_PER_NEAR / 100_000_000_000_000_000_000;
    format!("{}.{part:04} NEAR", yocto / YOCTO_PER_NEAR)
}

/// Share of the fixed supply with two decimals: 3.84%.
pub fn supply_share(units: u128) -> String {
    let bps = units * 10_000 / (SUPPLY_WHOLE_TOKENS * TOKEN_UNITS);
    format!("{}.{:02}%", bps / 100, bps % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_are_cut_not_rounded() {
        assert_eq!(tokens(38_469_999 * TOKEN_UNITS), "38.4M");
        assert_eq!(tokens(429_866 * TOKEN_UNITS + TOKEN_UNITS / 2), "429.8K");
        assert_eq!(tokens(12 * TOKEN_UNITS), "12");
        assert_eq!(near(1_745_899_999_999_999_999_999_999), "1.7458 NEAR");
        assert_eq!(near(57_788_733_756_831_323_908_335), "0.0577 NEAR");
        assert_eq!(supply_share(38_469_999 * TOKEN_UNITS), "3.84%");
    }
}
