//! A probe of the node's transaction cost rule, on a fresh sandbox. An account with 0.1 NEAR signs
//! the buy's shape (storage_deposit 0.005 NEAR and buy 0.5 NEAR, the prepaid gas split in two) at
//! 300, 150 and 30 Tgas. The node refuses each with NotEnoughBalance and states its exact cost,
//! which the recording fake RPC keeps. The three costs must fit one rule:
//! cost = deposit + (prepaid gas + send fee gas) x price. It prints the costs, the rule, the
//! node's gas price and, when FC_PROBE_CONFIG_OUT names a file, writes the protocol config there.
//!
//! Heavy: it starts its own sandbox. Needs NEAR_SANDBOX_BIN_PATH. Sandbox only.

mod common;

use fair_chance_sandbox_tests::buyer_funding::gas_price;
use fair_chance_sandbox_tests::World;
use near_api::types::transaction::actions::FunctionCallAction;
use near_api::types::{Action, NearGas, NearToken};
use serde_json::{json, Value};

const DEPOSIT: u128 = 505_000_000_000_000_000_000_000;

fn buy_shape(total: NearGas) -> Vec<Action> {
    let half = NearGas::from_gas(total.as_gas() / 2);
    [
        ("storage_deposit", NearToken::from_millinear(5)),
        ("buy", NearToken::from_millinear(500)),
    ]
    .into_iter()
    .map(|(method, deposit)| {
        let args = if method == "buy" {
            json!({ "min_tokens_out": "0" })
        } else {
            json!({})
        };
        Action::FunctionCall(Box::new(FunctionCallAction {
            method_name: method.to_string(),
            args: serde_json::to_vec(&args).unwrap(),
            gas: half,
            deposit,
        }))
    })
    .collect()
}

#[tokio::test]
async fn the_node_charges_prepaid_gas_by_one_rule() {
    let world = World::launch().await.unwrap();
    let node = common::host_port(&world.network);
    let fake = common::start(common::Mode::Forward(node.clone())).await;
    let mut send = world.network.clone();
    send.rpc_endpoints[0].url = fake.url.parse().unwrap();

    let probe: near_api::types::AccountId = format!("probe.{}", world.bob.id).parse().unwrap();
    let public_key = world.bob.signer.get_public_key().await.unwrap();
    near_api::Account::create_account(probe.clone())
        .fund_myself(world.bob.id.clone(), NearToken::from_millinear(100))
        .with_public_key(public_key)
        .with_signer(world.bob.signer.clone())
        .send_to(&world.network)
        .await
        .unwrap()
        .assert_success();

    let price = gas_price(&world.network).await.unwrap();
    println!("probe: gas_price RPC {price} yoctoNEAR per gas");

    let mut points = Vec::new();
    for tgas in [300, 150, 30] {
        let sent = near_api::Transaction::construct(probe.clone(), world.factory.id.clone())
            .add_actions(buy_shape(NearGas::from_tgas(tgas)))
            .with_signer(world.bob.signer.clone())
            .send_to(&send)
            .await;
        assert!(
            sent.is_err(),
            "the node took a {tgas} Tgas buy from 0.1 NEAR"
        );
        let answers = fake.answers();
        let (_, answer) = answers
            .iter()
            .rev()
            .find(|(request, _)| {
                request.contains("\"send_tx\"") || request.contains("broadcast_tx")
            })
            .expect("a recorded send");
        let cost = common::not_enough_balance_cost(answer)
            .unwrap_or_else(|| panic!("no NotEnoughBalance cost in {answer}"));
        println!("probe: {tgas} Tgas prepaid: cost {cost} yoctoNEAR");
        points.push((u128::from(tgas) * 1_000_000_000_000, cost));
    }

    let rule = common::fit_cost_rule(DEPOSIT, &points).unwrap();
    println!(
        "probe: rule cost = deposit + (prepaid gas + {} gas) x {} yoctoNEAR per gas",
        rule.fee_gas, rule.price
    );
    for (gas, cost) in &points {
        assert!(
            common::fits(DEPOSIT, &rule, *gas, *cost),
            "{gas} {cost} off {rule:?}"
        );
    }

    let body = json!({ "jsonrpc": "2.0", "id": "0", "method": "EXPERIMENTAL_protocol_config",
                       "params": { "finality": "final" } });
    let (_, out) = common::post_raw(&node, body.to_string().as_bytes())
        .await
        .unwrap();
    let config: Value = serde_json::from_slice(&out).unwrap();
    let result = &config["result"];
    println!(
        "probe: protocol {}, min_gas_price {}, runtime_config.min_gas_purchase_price {}",
        result["protocol_version"],
        result["min_gas_price"],
        result["runtime_config"]["min_gas_purchase_price"]
    );
    if let Ok(path) = std::env::var("FC_PROBE_CONFIG_OUT") {
        std::fs::write(path, &out).unwrap();
    }
}
