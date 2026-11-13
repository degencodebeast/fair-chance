//! Plays a list of trades against the coin contract in the near-sdk in-process runner, with exact
//! blocks and times, and writes what each trade did. The same coin code as the wasm; every limit on.
//!
//! cargo run -p fair-chance-coin --example replay-trades-in-process -- rules <out.json>
//! cargo run -p fair-chance-coin --example replay-trades-in-process -- play <plan.json> <out.json>
//!
//! `rules` writes `fc_rules` and `ft_total_supply` of a fresh coin. `play` reads a plan:
//! `{"trades": [{"account", "blocks_after_launch", "seconds_from_launch", "side",
//! "buy_sol_lamports", "buy_venue_sol_depth" (buy), "sell_numerator", "sell_denominator" (sell)}]}`.
//! A buy pays floor(pool NEAR depth x buy_sol_lamports / buy_venue_sol_depth), the pool depth
//! read live from `fc_pool` (the script's `near_for_buy`). A sell sells
//! floor(balance x numerator / denominator) of the account's live balance, or all of it when the
//! numerator is not below the denominator. Amounts are decimal strings. A refused trade records
//! its refusal and changes nothing.

use std::panic::{catch_unwind, AssertUnwindSafe};

use fair_chance_coin::Coin;
use near_sdk::json_types::U128;
use near_sdk::test_utils::{get_logs, VMContextBuilder};
use near_sdk::{testing_env, AccountId, NearToken};
use serde_json::{json, Value};

const LAUNCH_BLOCK: u64 = 100;
const LAUNCH_NS: u64 = 1_000_000_000_000;
const COIN: &str = "smdd.factory.test";
const FACTORY: &str = "factory.test";
const CREATOR: &str = "creator.test";

fn account(name: &str) -> AccountId {
    name.parse().expect("account id")
}

fn holder(name: &str) -> String {
    if name == "creator" {
        CREATOR.to_string()
    } else {
        format!("{name}.test")
    }
}

fn context(predecessor: &str, deposit: u128, blocks_after_launch: u64, seconds: u64) {
    testing_env!(VMContextBuilder::new()
        .current_account_id(account(COIN))
        .predecessor_account_id(account(predecessor))
        .signer_account_id(account(predecessor))
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .account_balance(NearToken::from_near(1_000_000))
        .block_height(LAUNCH_BLOCK + blocks_after_launch)
        .block_timestamp(LAUNCH_NS + seconds * 1_000_000_000)
        .build());
}

fn launched() -> Coin {
    context(FACTORY, 0, 0, 0);
    Coin::new(
        account(CREATOR),
        "SMDD replay".into(),
        "SMDD".into(),
        None,
        None,
        "replay".into(),
    )
}

fn event(name: &str) -> Option<Value> {
    get_logs()
        .iter()
        .filter_map(|line| line.strip_prefix("EVENT_JSON:"))
        .filter_map(|text| serde_json::from_str::<Value>(text).ok())
        .find(|e| e["standard"] == "fair_chance" && e["event"] == name)
        .map(|e| e["data"][0].clone())
}

fn amount(value: &Value, field: &str) -> u128 {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} must be a decimal string"))
        .parse()
        .unwrap_or_else(|_| panic!("{field} must be a decimal string"))
}

/// floor(a x numerator / denominator) without a 256-bit product: split `a` by the denominator.
/// Exact while (a / denominator) x numerator and denominator x numerator fit in u128.
fn mul_div_floor(a: u128, numerator: u128, denominator: u128) -> u128 {
    (a / denominator) * numerator + (a % denominator) * numerator / denominator
}

/// The sell size: floor(balance x numerator / denominator), or the whole balance.
fn share_of(balance: u128, numerator: u128, denominator: u128) -> u128 {
    if numerator >= denominator {
        return balance;
    }
    mul_div_floor(balance, numerator, denominator)
}

/// Runs one coin call; a panic (a refusal) comes back as its message.
fn attempt(call: impl FnOnce()) -> Result<(), String> {
    catch_unwind(AssertUnwindSafe(call)).map_err(|panic| {
        panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "refused".into())
    })
}

fn play(coin: &mut Coin, trade: &Value) -> Value {
    let who = holder(trade["account"].as_str().expect("account"));
    let b = trade["blocks_after_launch"]
        .as_u64()
        .expect("blocks_after_launch");
    let s = trade["seconds_from_launch"]
        .as_u64()
        .expect("seconds_from_launch");
    let side = trade["side"].as_str().expect("side");
    let mut result = json!({ "side": side });
    let outcome = if side == "buy" {
        let near_in = mul_div_floor(
            coin.fc_pool().near_reserve,
            amount(trade, "buy_sol_lamports"),
            amount(trade, "buy_venue_sol_depth"),
        );
        if coin.storage_balance_of(account(&who)).is_none() {
            let min = coin.storage_balance_bounds().min.as_yoctonear();
            context(&who, min, b, s);
            coin.storage_deposit(None, None);
        }
        context(&who, near_in, b, s);
        attempt(|| coin.buy(U128(0))).map(|()| {
            let buy = event("fc_buy").expect("fc_buy");
            result["near"] = buy["near_in"].clone();
            result["tokens"] = buy["tokens_out"].clone();
            result["tokens_asked"] = buy["tokens_out"].clone();
            result["fee_near"] = buy["fee_near"].clone();
            result["class"] = buy["class_after"].clone();
        })
    } else {
        let balance = coin.ft_balance_of(account(&who)).0;
        let tokens = share_of(
            balance,
            amount(trade, "sell_numerator"),
            amount(trade, "sell_denominator"),
        );
        result["tokens_asked"] = json!(tokens.to_string());
        if tokens == 0 {
            result["near"] = json!("0");
            result["tokens"] = json!("0");
            result["fee_near"] = json!("0");
            Ok(())
        } else {
            context(&who, 1, b, s);
            attempt(|| {
                coin.sell(U128(tokens), U128(0));
            })
            .map(|()| {
                let sell = event("fc_sell").expect("fc_sell");
                result["near"] = sell["near_out"].clone();
                result["tokens"] = sell["tokens_sold"].clone();
                result["fee_near"] = sell["fee_near"].clone();
                result["class"] = sell["class"].clone();
                result["fill"] = sell["fill"].clone();
                result["limit_reason"] = sell["limit_reason"].clone();
            })
        }
    };
    if let Err(refusal) = outcome {
        result["refusal"] = json!(refusal);
        result["near"] = json!("0");
        result["tokens"] = json!("0");
    }
    let pool = coin.fc_pool();
    result["token_reserve"] = json!(pool.token_reserve.to_string());
    result["near_reserve"] = json!(pool.near_reserve.to_string());
    result
}

fn main() {
    // Refusals are panics; their messages go into the result, not to stderr.
    std::panic::set_hook(Box::new(|_| {}));
    let args: Vec<String> = std::env::args().collect();
    let out = match args.get(1).map(String::as_str) {
        Some("rules") if args.len() == 3 => {
            let coin = launched();
            let answer = json!({
                "fc_rules": serde_json::to_value(coin.fc_rules()).expect("fc_rules"),
                "total_supply": coin.ft_total_supply().0.to_string(),
            });
            (answer, &args[2])
        }
        Some("play") if args.len() == 4 => {
            let plan: Value =
                serde_json::from_str(&std::fs::read_to_string(&args[2]).expect("read the plan"))
                    .expect("the plan is JSON");
            let mut coin = launched();
            let results: Vec<Value> = plan["trades"]
                .as_array()
                .expect("trades")
                .iter()
                .map(|trade| play(&mut coin, trade))
                .collect();
            (json!({ "results": results }), &args[3])
        }
        _ => {
            eprintln!(
                "usage: replay-trades-in-process rules <out.json> | play <plan.json> <out.json>"
            );
            std::process::exit(2);
        }
    };
    std::fs::write(out.1, serde_json::to_string(&out.0).expect("JSON")).expect("write the result");
}
