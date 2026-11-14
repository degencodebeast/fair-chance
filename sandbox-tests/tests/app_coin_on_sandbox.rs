//! The run on a coin the app launches, on a fresh sandbox. A stand-in for the app launches the
//! coin through the factory from another account (alice), at a random delay after the runner
//! starts to wait. The runner never launches: it finds the coin, checks it, sends the burst, finds
//! the launch transaction on chain and passes the same seven checks as the cabal run. A coin the
//! funder itself launched gets no buy.
//!
//! Heavy: each test starts its own sandbox. Needs NEAR_SANDBOX_BIN_PATH.
//!
//! The first test sends through the local fake RPC of `tests/common`, which records each node
//! answer. When the run stops, it prints what the node said about the signed sends (the error
//! `cause` and `data`, which near-api does not keep), the presign-time key answer, the heights and
//! the actions of one buy, and sends that one recorded buy again, raw, once. Sandbox only.

mod common;

use std::collections::BTreeMap;
use std::time::Duration;

use base64::Engine;

use fair_chance_sandbox_tests::app_coin::{
    join_app_coin_run, run_app_coin, stand_in_launch, AppCoinRequest, APP_COIN_BUYER_GAS,
};
use fair_chance_sandbox_tests::option_t::OptionTSetup;
use fair_chance_sandbox_tests::pass_list::PassList;
use fair_chance_sandbox_tests::World;
use near_api::types::NearToken;
use serde_json::{json, Value};

const STAND_IN_MAX_DELAY: Duration = Duration::from_secs(8);
/// About one block every 577 ms, as testnet on 14 Nov, so the burst guard sees testnet timing.
const TESTNET_BLOCK_MS: u64 = 577;

/// The testnet plan: 0.5 NEAR buys, 0.2 NEAR extra, a 0.3 NEAR later buy, 100 Tgas per buy.
fn setup(world: &World, label: &str) -> (OptionTSetup, AppCoinRequest) {
    let mut setup = OptionTSetup::sandbox_defaults(world, label, 10);
    setup.buy_near = NearToken::from_millinear(500);
    setup.buyer_extra = NearToken::from_millinear(200);
    setup.later_buy = NearToken::from_millinear(300);
    setup.buyer_gas = APP_COIN_BUYER_GAS;
    let request = AppCoinRequest {
        coin: format!("{label}.{}", world.factory.id).parse().unwrap(),
        label: label.to_string(),
    };
    (setup, request)
}

#[tokio::test]
async fn a_coin_launched_from_another_account_gets_the_burst_and_passes_the_seven_checks() {
    let world = World::launch_with_block_ms(TESTNET_BLOCK_MS).await.unwrap();
    let (mut setup, request) = setup(&world, "cabal3");
    let fake = common::start(common::Mode::Forward(common::host_port(&world.network))).await;
    setup.send_network.rpc_endpoints[0].url = fake.url.parse().unwrap();
    let (ready, go) = tokio::sync::oneshot::channel();
    let stand_in = tokio::spawn(stand_in_launch(
        world.network.clone(),
        world.factory.id.clone(),
        world.alice.clone(),
        request.label.clone(),
        STAND_IN_MAX_DELAY,
        go,
    ));
    let mut list = PassList::default();
    let known = world.coin_code_hash.to_string();
    let report = run_app_coin(&setup, &request, &known, &mut list, move || {
        let _ = ready.send(());
    })
    .await;
    let (report, launched) = match join_app_coin_run(report, stand_in).await {
        Ok(done) => done,
        Err(e) => {
            explain_the_sends(&world, &fake).await;
            panic!("{e:#}\n{list:?}")
        }
    };
    assert_eq!(list.lines().len(), 7, "{list:?}");
    assert!(
        list.lines().iter().all(|l| l.starts_with("PASS ")),
        "{list:?}"
    );
    assert!(report.run.timing.last_block <= 17, "{report:?}");
    // The runner found the launch transaction and the creator on chain: the stand-in's own.
    assert_eq!(report.launch_tx, launched, "{report:?}");
    assert_eq!(report.creator, world.alice.id, "{report:?}");
}

#[tokio::test]
async fn a_coin_the_funder_launched_gets_no_buy() {
    let world = World::launch_with_block_ms(TESTNET_BLOCK_MS).await.unwrap();
    let (setup, request) = setup(&world, "cabal3f");
    let (ready, go) = tokio::sync::oneshot::channel();
    // The funder of the sandbox run is bob: a coin bob launches is refused.
    let stand_in = tokio::spawn(stand_in_launch(
        world.network.clone(),
        world.factory.id.clone(),
        world.bob.clone(),
        request.label.clone(),
        Duration::ZERO,
        go,
    ));
    let mut list = PassList::default();
    let known = world.coin_code_hash.to_string();
    let refused = run_app_coin(&setup, &request, &known, &mut list, move || {
        let _ = ready.send(());
    })
    .await
    .expect_err("a coin the funder launched must be refused");
    stand_in.await.unwrap().unwrap();
    assert!(
        format!("{refused:#}").contains("no buy was sent"),
        "{refused:#}"
    );
    assert!(list.lines().is_empty(), "{list:?}");
}

/// One JSON-RPC call to the sandbox node, raw; the whole answer.
async fn rpc(node: &str, method: &str, params: Value) -> Value {
    let body = json!({ "jsonrpc": "2.0", "id": "0", "method": method, "params": params });
    match common::post_raw(node, body.to_string().as_bytes()).await {
        Ok((_, out)) => serde_json::from_slice(&out).unwrap_or(Value::Null),
        Err(_) => Value::Null,
    }
}

/// Every value under a key named `key`, with its path.
fn find_key(v: &Value, key: &str, path: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, child) in m {
                let here = format!("{path}.{k}");
                if k == key {
                    out.push(format!("{here} = {child}"));
                }
                find_key(child, key, &here, out);
            }
        }
        Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                find_key(child, key, &format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

/// Prints what the node said about the signed sends, and the facts of the first one. Read only,
/// except one raw send of the same recorded buy bytes again (sandbox only).
async fn explain_the_sends(world: &World, fake: &common::FakeRpc) {
    let say = |text: String| println!("diagnostic: {text}");
    let node = common::host_port(&world.network);
    let answers = fake.answers();
    let method = |request: &str| {
        serde_json::from_str::<Value>(request)
            .ok()
            .and_then(|v| v["method"].as_str().map(str::to_string))
            .unwrap_or_default()
    };
    let sends: Vec<&(String, String)> = answers
        .iter()
        .filter(|(request, _)| common::is_send(&method(request)))
        .collect();
    say(format!("{} signed sends reached the node", sends.len()));
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for (_, answer) in &sends {
        let kind = match common::rpc_error_parts(answer) {
            Some((cause, data)) => format!("error.cause {cause} error.data {data}"),
            None => format!("no error: {}", answer.chars().take(300).collect::<String>()),
        };
        *kinds.entry(kind).or_default() += 1;
    }
    for (kind, count) in &kinds {
        say(format!("{count} x {kind}"));
    }
    let Some((request, _)) = common::first_failed_send(&answers) else {
        say("no signed send was refused".to_string());
        return;
    };
    let request_json: Value = serde_json::from_str(request).unwrap_or_default();
    say(format!(
        "first refused send: method {}, wait_until {}",
        request_json["method"], request_json["params"]["wait_until"]
    ));
    let config = rpc(
        &node,
        "EXPERIMENTAL_protocol_config",
        json!({ "finality": "final" }),
    )
    .await;
    let mut limits = Vec::new();
    find_key(&config, "max_total_prepaid_gas", "config", &mut limits);
    find_key(&config, "protocol_version", "config", &mut limits);
    say(format!("protocol config: {}", limits.join(", ")));
    // The same recorded bytes once more, raw, on the sandbox only.
    let out = match common::post_raw(&node, request.as_bytes()).await {
        Ok((_, out)) => String::from_utf8_lossy(&out).to_string(),
        Err(e) => format!("no answer: {e}"),
    };
    match common::rpc_error_parts(&out) {
        Some((cause, data)) => say(format!("raw resend: error.cause {cause} error.data {data}")),
        None => say(format!(
            "raw resend: no error: {}",
            out.chars().take(300).collect::<String>()
        )),
    }
    let Some(b64) = request_json["params"]["signed_tx_base64"].as_str() else {
        say("the first refused send has no signed_tx_base64".to_string());
        return;
    };
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| e.to_string())
        .and_then(|bytes| {
            borsh::from_slice::<near_api::types::transaction::SignedTransaction>(&bytes)
                .map(|signed| (bytes.len(), signed))
                .map_err(|e| e.to_string())
        });
    let (size, signed) = match decoded {
        Ok(done) => done,
        Err(e) => {
            say(format!("the first refused send does not decode: {e}"));
            return;
        }
    };
    let tx = &signed.transaction;
    let mut hashes = Vec::new();
    if let Ok(value) = serde_json::to_value(tx) {
        find_key(&value, "block_hash", "tx", &mut hashes);
    }
    say(format!(
        "first refused send: {} to {}, nonce {}, {size} bytes, {}",
        tx.signer_id(),
        tx.receiver_id(),
        tx.nonce(),
        hashes.join(", ")
    ));
    let mut total_gas: u64 = 0;
    for (i, action) in tx.actions().iter().enumerate() {
        match action {
            near_api::types::Action::FunctionCall(f) => {
                total_gas += f.gas.as_gas();
                say(format!(
                    "action {i}: FunctionCall {} gas {} Tgas deposit {} yocto args {}",
                    f.method_name,
                    f.gas.as_tgas(),
                    f.deposit.as_yoctonear(),
                    String::from_utf8_lossy(&f.args)
                ));
            }
            other => say(format!("action {i}: {other:?}")),
        }
    }
    say(format!(
        "total prepaid gas {total_gas} ({} Tgas)",
        total_gas / 1_000_000_000_000
    ));
    match common::access_key_answer(&answers, tx.signer_id().as_str()) {
        Some(k) => say(format!(
            "presign key answer: nonce {}, block height {}, block hash {}",
            k.nonce, k.block_height, k.block_hash
        )),
        None => say("no presign key answer recorded".to_string()),
    }
    for (name, hash) in hashes.iter().filter_map(|h| h.split_once(" = ")) {
        let hash = hash.trim_matches('"');
        let block = rpc(&node, "block", json!({ "block_id": hash })).await;
        say(format!(
            "{name} {hash}: height {}",
            block["result"]["header"]["height"]
        ));
    }
    for finality in ["final", "optimistic"] {
        let block = rpc(&node, "block", json!({ "finality": finality })).await;
        let key = rpc(
            &node,
            "query",
            json!({ "request_type": "view_access_key", "finality": finality,
                    "account_id": tx.signer_id().to_string(),
                    "public_key": tx.public_key().to_string() }),
        )
        .await;
        say(format!(
            "now at {finality}: head {}, key nonce {}",
            block["result"]["header"]["height"], key["result"]["nonce"]
        ));
    }
}
