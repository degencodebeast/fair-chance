//! Signing reads the nonce and a block hash from the send RPC. When that read fails, nothing was
//! sent, so the step reads them once from the first read RPC (FastNEAR) instead. When that fails
//! too, it stops: "nothing was sent". No test here sends a transaction.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use fair_chance_sandbox_tests::presign_once;
use near_api::{NetworkConfig, RPCEndpoint, Signer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A JSON-RPC node that answers every request with an access key at nonce 41. It counts requests.
async fn access_key_node(count: Arc<AtomicUsize>) -> String {
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
                let id = serde_json::from_str::<serde_json::Value>(&body).unwrap()["id"].clone();
                let answer = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "nonce": 41,
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

/// Nothing listens on this port: a read there fails at once.
const DEAD: &str = "http://127.0.0.1:9";

fn transfer() -> (
    near_api::advanced::ExecuteSignedTransaction,
    near_api::types::AccountId,
) {
    let alice: near_api::types::AccountId = "alice.test.near".parse().unwrap();
    let signer = Signer::from_secret_key(near_api::signer::generate_secret_key().unwrap()).unwrap();
    let tx = near_api::Tokens::account(alice.clone())
        .send_to("bob.test.near".parse().unwrap())
        .near(near_api::types::NearToken::from_yoctonear(1))
        .with_signer(signer);
    (tx, alice)
}

#[tokio::test]
async fn a_failed_nonce_read_on_the_send_rpc_is_read_once_on_the_read_rpc() {
    let count = Arc::new(AtomicUsize::new(0));
    let read = network(&access_key_node(count.clone()).await);
    let (tx, alice) = transfer();

    let signed = presign_once(tx, &alice, &read, &network(DEAD))
        .await
        .unwrap();

    assert!(matches!(
        signed.transaction,
        near_api::advanced::TransactionableOrSigned::Signed(_)
    ));
    assert_eq!(count.load(Ordering::SeqCst), 1, "one read on the read RPC");
}

#[tokio::test]
async fn the_send_rpc_is_read_first_and_the_read_rpc_is_not_touched_when_it_answers() {
    let send_count = Arc::new(AtomicUsize::new(0));
    let read_count = Arc::new(AtomicUsize::new(0));
    let send = network(&access_key_node(send_count.clone()).await);
    let read = network(&access_key_node(read_count.clone()).await);
    let (tx, alice) = transfer();

    presign_once(tx, &alice, &read, &send).await.unwrap();

    assert_eq!(send_count.load(Ordering::SeqCst), 1);
    assert_eq!(read_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn when_both_reads_fail_it_stops_and_says_nothing_was_sent() {
    let (tx, alice) = transfer();
    let error = presign_once(tx, &alice, &network(DEAD), &network("http://127.0.0.1:7"))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("nothing was sent"), "{error}");
    assert!(
        !error.contains("127.0.0.1"),
        "an RPC URL is in the error: {error}"
    );
}
