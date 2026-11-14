//! Whether each runner account holds enough NEAR for the transactions it signs, at the node's
//! gas price. The node refuses a transaction whose deposits plus prepaid gas at the current gas
//! price are more than the signer holds (NotEnoughBalance), before it runs. The sandbox charges
//! ten times the testnet gas price, so a plan that is enough on testnet can be short there.
//!
//! The need counts every prepaid gas unit as burnt: an early buyer signs its buy (with the
//! storage deposit), its early sell and its sell after the window; the later buyer signs a buy
//! and a sell. Each transaction adds [`SEND_FEE_GAS`] for its send fees, the gas price may rise
//! by [`GAS_PRICE_HEADROOM_PERCENT`], and each account keeps [`ACCOUNT_RESERVE`] for itself.

use anyhow::{bail, Context, Result};
use near_api::types::{NearGas, NearToken};
use near_api::NetworkConfig;
use near_openapi_client::types;

use crate::option_t::{BUY_STORAGE_DEPOSIT, EARLY_SELL_GAS};
use crate::rate_limit::{run_budget, with_rate_limit_backoff, without_urls};

/// The send fees of one transaction, with room: the sandbox buy paid about 1.72 Tgas.
pub const SEND_FEE_GAS: NearGas = NearGas::from_tgas(5);
/// The gas price used is the node's price times this, over 100: nearcore lets the price rise at
/// most 1 percent a block, so 25 percent is about 22 full blocks.
pub const GAS_PRICE_HEADROOM_PERCENT: u128 = 125;
/// NEAR each runner account keeps for its own storage.
pub const ACCOUNT_RESERVE: NearToken = NearToken::from_millinear(10);
/// The yoctoNEAR each sell attaches.
const SELL_DEPOSIT: u128 = 1;

fn gas_cost(gas: u128, gas_price: u128) -> u128 {
    let raised = (gas_price * GAS_PRICE_HEADROOM_PERCENT).div_ceil(100);
    gas * raised
}

fn need(buy: NearToken, gas: u128, sells: u128, gas_price: u128) -> NearToken {
    NearToken::from_yoctonear(
        buy.as_yoctonear()
            + BUY_STORAGE_DEPOSIT.as_yoctonear()
            + gas_cost(gas, gas_price)
            + ACCOUNT_RESERVE.as_yoctonear()
            + sells * SELL_DEPOSIT,
    )
}

/// What an early buyer must hold: its buy, early sell and sell after the window.
pub fn buyer_need(buy: NearToken, buyer_gas: NearGas, gas_price: u128) -> NearToken {
    let gas = 2 * u128::from(buyer_gas.as_gas())
        + u128::from(EARLY_SELL_GAS.as_gas())
        + 3 * u128::from(SEND_FEE_GAS.as_gas());
    need(buy, gas, 2, gas_price)
}

/// What the later buyer must hold: its buy and its sell.
pub fn later_buyer_need(later_buy: NearToken, buyer_gas: NearGas, gas_price: u128) -> NearToken {
    let gas = 2 * u128::from(buyer_gas.as_gas()) + 2 * u128::from(SEND_FEE_GAS.as_gas());
    need(later_buy, gas, 1, gas_price)
}

/// The smallest extra, in whole milliNEAR, that covers both kinds of buyer. A zero `later_buy`
/// means no later buyer.
pub fn buyer_extra_for(
    buy: NearToken,
    later_buy: NearToken,
    buyer_gas: NearGas,
    gas_price: u128,
) -> NearToken {
    let short = |need: NearToken, holds: NearToken| {
        need.as_yoctonear().saturating_sub(holds.as_yoctonear())
    };
    let mut extra = short(buyer_need(buy, buyer_gas, gas_price), buy);
    if !later_buy.is_zero() {
        extra = extra.max(short(
            later_buyer_need(later_buy, buyer_gas, gas_price),
            later_buy,
        ));
    }
    let milli = NearToken::from_millinear(1).as_yoctonear();
    NearToken::from_millinear(extra.div_ceil(milli))
}

/// Stops, before any signature, when a buyer (buy + extra) or the later buyer (later buy +
/// extra) would hold less than its need at `gas_price` (yoctoNEAR per gas).
pub fn check_buyer_funding(
    buy: NearToken,
    later_buy: NearToken,
    extra: NearToken,
    buyer_gas: NearGas,
    gas_price: u128,
) -> Result<()> {
    let fix = buyer_extra_for(buy, later_buy, buyer_gas, gas_price);
    let mut short = vec![(
        "each early buyer",
        buyer_need(buy, buyer_gas, gas_price),
        buy.saturating_add(extra),
    )];
    if !later_buy.is_zero() {
        short.push((
            "the later buyer",
            later_buyer_need(later_buy, buyer_gas, gas_price),
            later_buy.saturating_add(extra),
        ));
    }
    for (who, need, holds) in short {
        if holds < need {
            bail!(
                "{who} needs {} at the node's gas price of {gas_price} yoctoNEAR per gas \
                 (with {GAS_PRICE_HEADROOM_PERCENT} percent headroom), but the plan gives it {}; \
                 nothing was signed. Use --buyer-extra-millinear {} or more",
                need.exact_amount_display(),
                holds.exact_amount_display(),
                fix.as_millinear()
            );
        }
    }
    Ok(())
}

/// The node's current gas price in yoctoNEAR per gas. A read only; it backs off on a 429 or a
/// 5xx and stops on anything else (the shared read fault rule).
pub async fn gas_price(network: &NetworkConfig) -> Result<u128> {
    let endpoint = network
        .rpc_endpoints
        .first()
        .context("the read network has no RPC")?;
    let client = near_openapi_client::Client::new(endpoint.url.as_str().trim_end_matches('/'));
    let request = types::JsonRpcRequestForGasPrice {
        id: "fc".to_string(),
        jsonrpc: "2.0".to_string(),
        method: types::JsonRpcRequestForGasPriceMethod::GasPrice,
        params: types::RpcGasPriceRequest { block_id: None },
    };
    let answer =
        with_rate_limit_backoff("the gas price", run_budget(), || client.gas_price(&request))
            .await?
            .into_inner();
    match answer {
        types::JsonRpcResponseForRpcGasPriceResponseAndRpcGasPriceError::Variant0 {
            result,
            ..
        } => Ok(result.gas_price.as_yoctonear()),
        types::JsonRpcResponseForRpcGasPriceResponseAndRpcGasPriceError::Variant1 {
            error, ..
        } => bail!(
            "the gas price read gave an error: {}; stopped, nothing was signed",
            without_urls(&format!("{error:?}"))
        ),
    }
}
