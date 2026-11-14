//! A transaction signed before the launch (the burst) goes out through send_once with no read
//! before its send: a nonce read there costs one slow RPC call (about 3.4 s on testnet) per buy.
//! A transaction that is not signed yet still reads the nonce of its own signer first. A
//! transaction named for another account stops before any call. The fake RPCs answer HTTP 403, so
//! nothing reaches a chain; the keys are throwaway in-memory keys.

mod common;

use fair_chance_sandbox_tests::send_once;
use near_api::types::{AccountId, CryptoHash, NearToken};
use near_api::{Contract, Signer};
use serde_json::json;

fn buyer() -> AccountId {
    "cabal-buyer-01.fairchance.testnet".parse().unwrap()
}

fn buy_tx() -> near_api::advanced::ExecuteSignedTransaction {
    let signer = Signer::from_secret_key(near_api::signer::generate_secret_key().unwrap()).unwrap();
    Contract("cabal.factory.fairchance.testnet".parse().unwrap())
        .call_function("buy", json!({ "min_tokens_out": "0" }))
        .transaction()
        .deposit(NearToken::from_millinear(1900))
        .with_signer(buyer(), signer)
}

async fn presigned_buy_tx() -> near_api::advanced::ExecuteSignedTransaction {
    let secret = near_api::signer::generate_secret_key().unwrap();
    let public_key = secret.public_key();
    let signer = Signer::from_secret_key(secret).unwrap();
    Contract("cabal.factory.fairchance.testnet".parse().unwrap())
        .call_function("buy", json!({ "min_tokens_out": "0" }))
        .transaction()
        .deposit(NearToken::from_millinear(1900))
        .with_signer(buyer(), signer)
        .presign_offline(public_key, CryptoHash::default(), 7)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_presigned_transaction_goes_out_with_no_read_before_its_send() {
    let send = common::start(common::Mode::Always403).await;
    let read = common::start(common::Mode::Always403).await;
    let _ = send_once(
        presigned_buy_tx().await,
        &buyer(),
        &common::network(&read.url),
        &common::network(&send.url),
    )
    .await;
    assert_eq!(
        send.methods().first().map(String::as_str),
        Some("send_tx"),
        "the send RPC got {:?} before the send",
        send.methods()
    );
    assert!(
        !read.methods().iter().any(|m| m == "query"),
        "the read RPC got a nonce read: {:?}",
        read.methods()
    );
}

#[tokio::test]
async fn an_unsigned_transaction_reads_the_nonce_of_its_own_signer_first() {
    let send = common::start(common::Mode::Always403).await;
    let read = common::start(common::Mode::Always403).await;
    let error = send_once(
        buy_tx(),
        &buyer(),
        &common::network(&read.url),
        &common::network(&send.url),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("nothing was sent"), "{error}");
    assert_eq!(send.methods().first().map(String::as_str), Some("query"));
    assert!(
        send.bodies()[0].contains("view_access_key")
            && send.bodies()[0].contains("cabal-buyer-01.fairchance.testnet"),
        "{:?}",
        send.bodies()
    );
    assert_eq!(send.sends(), 0, "{:?}", send.methods());
}

#[tokio::test]
async fn a_presigned_transaction_named_for_another_account_stops_before_any_call() {
    let send = common::start(common::Mode::Always403).await;
    let read = common::start(common::Mode::Always403).await;
    let funder: AccountId = "fairchance.testnet".parse().unwrap();
    let error = send_once(
        presigned_buy_tx().await,
        &funder,
        &common::network(&read.url),
        &common::network(&send.url),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("nothing was sent") && error.contains("cabal-buyer-01.fairchance.testnet"),
        "{error}"
    );
    assert!(send.methods().is_empty(), "{:?}", send.methods());
    assert!(read.methods().is_empty(), "{:?}", read.methods());
}
