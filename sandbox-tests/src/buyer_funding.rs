//! Whether each runner account holds enough NEAR for the transactions it signs. The node refuses
//! a transaction whose deposits plus its cost of gas are more than the signer holds
//! (NotEnoughBalance), before it runs. The node buys the prepaid gas and the send fees at
//! `min_gas_purchase_price` of its runtime config when that is above the block gas price: 1e9
//! yoctoNEAR per gas on the sandbox (protocol 86) and on testnet (protocol 88), where the block
//! gas price is 1e8. A sandbox probe measured cost = deposit + (prepaid gas + 1.719 Tgas) x 1e9.
//!
//! The price used is the higher of the node's gas price with [`GAS_PRICE_HEADROOM_PERCENT`] and
//! `min_gas_purchase_price` ([`price_used`]). An early buyer signs at two moments: the burst (its
//! buy and early sell, both prepaid in full before any refund) and the sell after the window
//! (the refunds are back; each earlier transaction burnt at most [`BURNT_GAS_PER_TX`]). The need
//! is the larger. Each transaction adds [`SEND_FEE_GAS`], and each account keeps
//! [`ACCOUNT_RESERVE`] for itself.

use anyhow::{bail, Context, Result};
use near_api::types::{NearGas, NearToken};
use near_api::NetworkConfig;
use near_openapi_client::{types, ClientInfo};
use serde_json::{json, Value};

use crate::option_t::{BUY_STORAGE_DEPOSIT, EARLY_SELL_GAS};
use crate::rate_limit::{run_budget, with_rate_limit_backoff, without_urls};

/// The send fees of one transaction, with room: the sandbox probe measured 1.719 Tgas.
pub const SEND_FEE_GAS: NearGas = NearGas::from_tgas(5);
/// The gas one earlier transaction burnt, with room: a CABAL2 buy burnt 5.04 Tgas.
pub const BURNT_GAS_PER_TX: NearGas = NearGas::from_tgas(10);
/// The node's gas price is raised by this, over 100, before it is compared with the purchase
/// price: nearcore lets the gas price rise at most 1 percent a block, so 25 percent is about 22
/// full blocks.
pub const GAS_PRICE_HEADROOM_PERCENT: u128 = 125;
/// NEAR each runner account keeps for its own storage.
pub const ACCOUNT_RESERVE: NearToken = NearToken::from_millinear(10);
/// The yoctoNEAR each sell attaches.
const SELL_DEPOSIT: u128 = 1;

/// The price, in yoctoNEAR per gas, the check uses: the higher of `gas_price` with headroom and
/// `min_purchase` (the node's `min_gas_purchase_price`).
pub fn price_used(gas_price: u128, min_purchase: u128) -> u128 {
    (gas_price * GAS_PRICE_HEADROOM_PERCENT)
        .div_ceil(100)
        .max(min_purchase)
}

fn gas(g: NearGas) -> u128 {
    u128::from(g.as_gas())
}

/// buy + storage deposit + gas x price + reserve + sell deposits.
fn need(buy: NearToken, total_gas: u128, sells: u128, price: u128) -> u128 {
    buy.as_yoctonear()
        + BUY_STORAGE_DEPOSIT.as_yoctonear()
        + total_gas * price
        + ACCOUNT_RESERVE.as_yoctonear()
        + sells * SELL_DEPOSIT
}

/// What an early buyer must hold at `price`: the larger of the burst and the sell after the
/// window.
pub fn buyer_need(buy: NearToken, buyer_gas: NearGas, price: u128) -> NearToken {
    let fee = gas(SEND_FEE_GAS);
    let burst = need(
        buy,
        gas(buyer_gas) + gas(EARLY_SELL_GAS) + 2 * fee,
        1,
        price,
    );
    let window = need(
        buy,
        2 * gas(BURNT_GAS_PER_TX) + gas(buyer_gas) + fee,
        2,
        price,
    );
    NearToken::from_yoctonear(burst.max(window))
}

/// What the later buyer must hold at `price`: the larger of its buy and its sell after it.
pub fn later_buyer_need(later_buy: NearToken, buyer_gas: NearGas, price: u128) -> NearToken {
    let fee = gas(SEND_FEE_GAS);
    let buy = need(later_buy, gas(buyer_gas) + fee, 0, price);
    let sell = need(
        later_buy,
        gas(BURNT_GAS_PER_TX) + gas(buyer_gas) + fee,
        1,
        price,
    );
    NearToken::from_yoctonear(buy.max(sell))
}

/// The smallest extra, in whole milliNEAR, that covers both kinds of buyer. A zero `later_buy`
/// means no later buyer.
pub fn buyer_extra_for(
    buy: NearToken,
    later_buy: NearToken,
    buyer_gas: NearGas,
    price: u128,
) -> NearToken {
    let short = |need: NearToken, holds: NearToken| {
        need.as_yoctonear().saturating_sub(holds.as_yoctonear())
    };
    let mut extra = short(buyer_need(buy, buyer_gas, price), buy);
    if !later_buy.is_zero() {
        extra = extra.max(short(
            later_buyer_need(later_buy, buyer_gas, price),
            later_buy,
        ));
    }
    let milli = NearToken::from_millinear(1).as_yoctonear();
    NearToken::from_millinear(extra.div_ceil(milli))
}

/// Stops, before any signature, when a buyer (buy + extra) or the later buyer (later buy +
/// extra) would hold less than its need at `price` (yoctoNEAR per gas, from [`price_used`]).
pub fn check_buyer_funding(
    buy: NearToken,
    later_buy: NearToken,
    extra: NearToken,
    buyer_gas: NearGas,
    price: u128,
) -> Result<()> {
    let fix = buyer_extra_for(buy, later_buy, buyer_gas, price);
    let mut buyers = vec![(
        "each early buyer",
        buyer_need(buy, buyer_gas, price),
        buy.saturating_add(extra),
    )];
    if !later_buy.is_zero() {
        buyers.push((
            "the later buyer",
            later_buyer_need(later_buy, buyer_gas, price),
            later_buy.saturating_add(extra),
        ));
    }
    for (who, need, holds) in buyers {
        if holds < need {
            bail!(
                "{who} needs {} at {price} yoctoNEAR per gas (the higher of the node's gas \
                 price with {GAS_PRICE_HEADROOM_PERCENT} percent headroom and its \
                 min_gas_purchase_price) with {} Tgas per buy, but the plan gives it {}; \
                 nothing was signed. Use --buyer-extra-millinear {} or more, or less --buyer-tgas",
                need.exact_amount_display(),
                buyer_gas.as_tgas(),
                holds.exact_amount_display(),
                fix.as_millinear()
            );
        }
    }
    Ok(())
}

/// The node's gas price and `min_gas_purchase_price`, and the price the check uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GasPrices {
    pub gas_price: u128,
    pub min_purchase: u128,
    pub used: u128,
}

/// Reads both prices from the node. Reads only; each backs off on a 429 or a 5xx and stops on
/// anything else (the shared read fault rule). A config with no `min_gas_purchase_price` stops
/// the run: the check would not know the price the node charges.
pub async fn gas_prices(network: &NetworkConfig) -> Result<GasPrices> {
    let gas_price = gas_price(network).await?;
    let config = protocol_config(network).await?;
    let min_purchase: u128 = config["runtime_config"]["min_gas_purchase_price"]
        .as_str()
        .context("the protocol config has no runtime_config.min_gas_purchase_price; stopped, nothing was signed")?
        .parse()
        .context("runtime_config.min_gas_purchase_price is not a number")?;
    Ok(GasPrices {
        gas_price,
        min_purchase,
        used: price_used(gas_price, min_purchase),
    })
}

fn client(network: &NetworkConfig) -> Result<near_openapi_client::Client> {
    let endpoint = network
        .rpc_endpoints
        .first()
        .context("the read network has no RPC")?;
    Ok(near_openapi_client::Client::new(
        endpoint.url.as_str().trim_end_matches('/'),
    ))
}

/// The node's current gas price in yoctoNEAR per gas.
pub async fn gas_price(network: &NetworkConfig) -> Result<u128> {
    let client = client(network)?;
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

/// An HTTP or transport fault in the form the read fault rule reads: "status: NNN" and the
/// Retry-After header, with no URL.
struct RawFault(String);

impl std::fmt::Debug for RawFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// `EXPERIMENTAL_protocol_config` at final, as raw JSON: the typed client has no
/// `min_gas_purchase_price`.
async fn protocol_config(network: &NetworkConfig) -> Result<Value> {
    let client = client(network)?;
    let url = client.baseurl().to_string();
    let body = json!({ "jsonrpc": "2.0", "id": "fc", "method": "EXPERIMENTAL_protocol_config",
                       "params": { "finality": "final" } })
    .to_string();
    let text = with_rate_limit_backoff("the protocol config", run_budget(), || async {
        let answer = client
            .client()
            .post(&url)
            .header("content-type", "application/json")
            .body(body.clone())
            .send()
            .await
            .map_err(|e| RawFault(without_urls(&format!("{e:?}"))))?;
        let status = answer.status().as_u16();
        if status != 200 {
            let retry_after = answer
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            return Err(RawFault(format!(
                "status: {status}, headers: {{\"retry-after\": \"{retry_after}\"}}"
            )));
        }
        answer
            .text()
            .await
            .map_err(|e| RawFault(without_urls(&format!("{e:?}"))))
    })
    .await?;
    let config: Value =
        serde_json::from_str(&text).context("the protocol config is not JSON; stopped")?;
    if let Some(error) = config.get("error") {
        bail!(
            "the protocol config read gave an error: {}; stopped, nothing was signed",
            without_urls(&error.to_string())
        );
    }
    Ok(config["result"].clone())
}
