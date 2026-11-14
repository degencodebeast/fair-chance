//! The test harness reads a node's raw JSON-RPC answers. near-api keeps only `cause.info` of an
//! error, and nearcore leaves the detail of INVALID_TRANSACTION out of it, so the detail is only
//! in the top-level `data`. The harness keeps both, and finds the presign-time access key answer
//! of an account among the recorded requests and answers.

mod common;

use serde_json::json;

const INVALID_TX_ANSWER: &str = r#"{"jsonrpc":"2.0","error":{"name":"HANDLER_ERROR","cause":{"name":"INVALID_TRANSACTION","info":{}},"code":-32000,"message":"Server error","data":{"TxExecutionError":{"InvalidTxError":"Expired"}}},"id":"0"}"#;

#[test]
fn the_cause_and_the_data_of_a_node_error_are_kept() {
    let (cause, data) = common::rpc_error_parts(INVALID_TX_ANSWER).unwrap();
    assert_eq!(cause, json!({ "name": "INVALID_TRANSACTION", "info": {} }));
    assert_eq!(
        data,
        json!({ "TxExecutionError": { "InvalidTxError": "Expired" } })
    );
}

#[test]
fn an_answer_with_a_result_or_no_json_has_no_error_parts() {
    assert_eq!(
        common::rpc_error_parts(r#"{"jsonrpc":"2.0","result":{},"id":"0"}"#),
        None
    );
    assert_eq!(common::rpc_error_parts("not json"), None);
}

#[test]
fn the_access_key_answer_of_one_account_is_found_among_the_recorded_pairs() {
    let request = |account: &str| {
        json!({
            "jsonrpc": "2.0", "id": "0", "method": "query",
            "params": { "request_type": "view_access_key", "account_id": account,
                        "public_key": "ed25519:abc", "finality": "final" }
        })
        .to_string()
    };
    let answer = |nonce: u64, height: u64| {
        json!({
            "jsonrpc": "2.0", "id": "0",
            "result": { "nonce": nonce, "permission": "FullAccess",
                        "block_height": height, "block_hash": format!("hash{height}") }
        })
        .to_string()
    };
    let pairs = vec![
        (request("a.sandbox"), answer(7, 100)),
        (
            r#"{"method":"block","params":{"finality":"final"}}"#.to_string(),
            "{}".to_string(),
        ),
        (request("b.sandbox"), answer(9, 104)),
    ];
    assert_eq!(
        common::access_key_answer(&pairs, "b.sandbox"),
        Some(common::KeyAnswer {
            nonce: 9,
            block_height: 104,
            block_hash: "hash104".to_string()
        })
    );
    assert_eq!(common::access_key_answer(&pairs, "c.sandbox"), None);
}

#[test]
fn the_first_failed_send_skips_reads_and_sends_that_passed() {
    let send = |n: u64| {
        json!({ "jsonrpc": "2.0", "id": "0", "method": "send_tx",
                "params": { "signed_tx_base64": format!("tx{n}"), "wait_until": "FINAL" } })
        .to_string()
    };
    let read = json!({ "jsonrpc": "2.0", "id": "0", "method": "query", "params": {} }).to_string();
    let passed = r#"{"jsonrpc":"2.0","result":{"final_execution_status":"FINAL"},"id":"0"}"#;
    let pairs = vec![
        (send(1), passed.to_string()),
        (read.clone(), INVALID_TX_ANSWER.to_string()),
        (send(2), INVALID_TX_ANSWER.to_string()),
        (send(3), INVALID_TX_ANSWER.to_string()),
    ];
    assert_eq!(common::first_failed_send(&pairs), Some(&pairs[2]));
    assert_eq!(common::first_failed_send(&pairs[..2]), None);
}
