//! Signing reads the nonce of the account that signs the transaction, never of another account.
//! A delete of a buyer account is signed by the buyer (with the funder's key): its nonce is the
//! buyer's. The node below answers nonce 900 for fairchance.testnet and 41 for any other account.

use fair_chance_sandbox_tests::{presign_once, send_once};
use near_api::{NetworkConfig, RPCEndpoint, Signer};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn node(count: Arc<AtomicUsize>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let count = count.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let body = loop {
                    let n = socket.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(at) = text.find("\r\n\r\n") {
                        let len: usize = text
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap())
                            })
                            .unwrap_or(0);
                        if buf.len() >= at + 4 + len {
                            break text[at + 4..at + 4 + len].to_string();
                        }
                    }
                };
                count.fetch_add(1, Ordering::SeqCst);
                let req = serde_json::from_str::<serde_json::Value>(&body).unwrap();
                let id = req["id"].clone();
                let nonce = if body.contains("fairchance.testnet\"") && !body.contains("bag-buyer")
                {
                    900
                } else {
                    41
                };
                let answer = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "nonce": nonce,
                        "permission": "FullAccess",
                        "block_height": 100,
                        "block_hash": "11111111111111111111111111111111"
                    }
                })
                .to_string();
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                    answer.len()
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
            });
        }
    });
    url
}

/// A network with one endpoint and one try.
fn network(url: &str) -> NetworkConfig {
    let mut endpoint = RPCEndpoint::new(url.parse().unwrap());
    endpoint.retries = 1;
    let mut network = NetworkConfig::from_rpc_url("sandbox", url.parse().unwrap());
    network.rpc_endpoints = vec![endpoint];
    network
}

fn delete_tx() -> near_api::advanced::ExecuteSignedTransaction {
    let buyer: near_api::types::AccountId = "bag-buyer-03.fairchance.testnet".parse().unwrap();
    let funder: near_api::types::AccountId = "fairchance.testnet".parse().unwrap();
    let signer = Signer::from_secret_key(near_api::signer::generate_secret_key().unwrap()).unwrap();
    near_api::Account(buyer)
        .delete_account_with_beneficiary(funder)
        .with_signer(signer)
}

fn nonce_of(s: &near_api::advanced::ExecuteSignedTransaction) -> (String, u64) {
    match &s.transaction {
        near_api::advanced::TransactionableOrSigned::Signed((tx, _)) => {
            let t = &tx.transaction;
            (t.signer_id().to_string(), t.nonce())
        }
        _ => panic!("not signed"),
    }
}

#[tokio::test]
async fn the_nonce_is_read_for_the_account_that_signs_the_transaction() {
    let count = Arc::new(AtomicUsize::new(0));
    let net = network(&node(count.clone()).await);

    let signed = presign_once(delete_tx(), &net, &net).await.unwrap();

    assert_eq!(
        nonce_of(&signed),
        ("bag-buyer-03.fairchance.testnet".to_string(), 42)
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_send_named_for_another_account_stops_before_any_read() {
    let count = Arc::new(AtomicUsize::new(0));
    let net = network(&node(count.clone()).await);
    let funder: near_api::types::AccountId = "fairchance.testnet".parse().unwrap();

    let error = send_once(delete_tx(), &funder, &net, &net)
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("nothing was sent"), "{error}");
    assert!(error.contains("bag-buyer-03.fairchance.testnet"), "{error}");
    assert_eq!(count.load(Ordering::SeqCst), 0, "a read went out");
}
