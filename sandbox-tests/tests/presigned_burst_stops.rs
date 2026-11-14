//! The burst stops on a 403 or any unknown result: when a buy send fails, no early sell is sent,
//! and the run stops with the failed transaction named. Nothing reaches a chain here: the fake RPC
//! answers every request with HTTP 403, and the keys are throwaway in-memory keys.

mod common;

use std::time::Duration;

use fair_chance_sandbox_tests::burst::{send_burst, BurstSettings, SniperTxs};
use near_api::types::{CryptoHash, NearGas, NearToken};
use near_api::{Contract, Signer};
use serde_json::json;

async fn sniper(name: &str) -> SniperTxs {
    let secret = near_api::signer::generate_secret_key().unwrap();
    let public_key = secret.public_key();
    let signer = Signer::from_secret_key(secret).unwrap();
    let id: near_api::types::AccountId = format!("{name}.funder.testnet").parse().unwrap();
    let coin: near_api::types::AccountId = "c.factory.testnet".parse().unwrap();
    let signed = |method: &str, args: serde_json::Value, deposit: NearToken, nonce: u64| {
        Contract(coin.clone())
            .call_function(method, args)
            .transaction()
            .deposit(deposit)
            .gas(NearGas::from_tgas(100))
            .with_signer(id.clone(), signer.clone())
            .presign_offline(public_key, CryptoHash::default(), nonce)
    };
    SniperTxs {
        buyer: id.clone(),
        buy: signed("buy", json!({}), NearToken::from_near(1), 1)
            .await
            .unwrap(),
        early_sell: signed("sell", json!({}), NearToken::from_yoctonear(1), 2)
            .await
            .unwrap(),
    }
}

#[tokio::test]
async fn a_403_on_a_buy_stops_the_burst_before_any_early_sell() {
    let fake = common::start(common::Mode::Always403).await;
    let network = common::network(&fake.url);
    let snipers = vec![sniper("c-buyer-01").await, sniper("c-buyer-02").await];
    let settings = BurstSettings {
        early_sell_gap: Duration::from_millis(500),
        send_delay: Duration::ZERO,
    };
    let error = send_burst(&network, &network, snipers, settings)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("c-buyer-01.funder.testnet"), "{error}");
    assert!(error.contains("no early sell was sent"), "{error}");
    assert!(
        !error.contains(&fake.url),
        "the error carries the RPC URL: {error}"
    );
    // Two buys reached the RPC; the two early sells never did.
    assert_eq!(fake.sends(), 2, "{:?}", fake.methods());
}
