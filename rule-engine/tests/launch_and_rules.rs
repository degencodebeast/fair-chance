//! `new` through the engine: the caller check, the icon check, the one `ft_mint`, and the
//! `fc_rules` view with every fixed value of the spec table.

use rule_engine::{
    apply, launch, view_rules, Action, At, Event, FixedValues, Holders, LaunchRequest, Launched,
    Refusal,
};
use serde_json::{json, Value};

const SUPPLY: u128 = 1_000_000_000_000_000_000_000_000_000;
const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const WEBP_PREFIX: &str = "data:image/webp;base64,";

fn at(block: u64) -> At {
    At {
        block,
        ns: block * 1_000_000_000,
    }
}

fn request<'a>(caller: &'a str, icon: Option<&'a str>) -> LaunchRequest<'a> {
    LaunchRequest {
        coin: "walk.factory.test",
        caller,
        creator: "creator.test",
        code_hash: "4reLvkAWfqk5fsqio1KLudk46cqRz9erQdaHkWZKMJDZ",
        name: "Walk Coin",
        symbol: "WALK",
        icon,
        description: Some("A coin that walks."),
    }
}

fn launched(caller: &str, icon: Option<&str>) -> Result<Launched, Refusal> {
    launch(FixedValues::fixed(), request(caller, icon), at(100))
}

/// A data URL of exactly `len` bytes with a WebP prefix and base64 text.
fn webp_data_url(len: usize) -> String {
    let mut url = WEBP_PREFIX.to_string();
    while url.len() < len {
        url.push('A');
    }
    url
}

#[test]
fn new_called_by_the_parent_account_mints_the_whole_supply_once_into_the_pool() {
    let launched = launched("factory.test", None).expect("launch");
    let mints: Vec<_> = launched
        .events
        .iter()
        .filter(|e| matches!(e, Event::FtMint(_)))
        .collect();
    assert_eq!(mints.len(), 1);
    let line = mints[0].log_line();
    let event: Value = serde_json::from_str(line.strip_prefix("EVENT_JSON:").unwrap()).unwrap();
    assert_eq!(event["standard"], "nep141");
    assert_eq!(
        event["data"],
        json!([{ "owner_id": "walk.factory.test", "amount": SUPPLY.to_string() }])
    );
    assert_eq!(launched.state.token_reserve, SUPPLY);
    assert_eq!(launched.state.real_near, 0);
}

#[test]
fn new_called_by_any_account_but_the_parent_is_refused() {
    for caller in [
        "creator.test",
        "alice.test",
        "test",
        "walk.factory.test",
        "xfactory.test",
        "sub.factory.test",
    ] {
        let refusal = launched(caller, None).unwrap_err();
        assert_eq!(
            refusal.to_string(),
            "new must be called by the parent account of the coin",
            "caller {caller}"
        );
    }
}

#[test]
fn a_top_level_coin_account_has_no_parent_so_new_is_always_refused() {
    let mut req = request("near", None);
    req.coin = "walk";
    let refusal = launch(FixedValues::fixed(), req, at(100)).unwrap_err();
    assert_eq!(
        refusal.to_string(),
        "new must be called by the parent account of the coin"
    );
}

#[test]
fn the_icon_must_be_a_webp_data_url_under_8_kb() {
    let largest = webp_data_url(8191);
    let launched_ok = launched("factory.test", Some(&largest)).expect("8191 bytes is under 8 KB");
    assert_eq!(
        launched_ok.state.metadata.icon.as_deref(),
        Some(largest.as_str())
    );
    assert!(
        launched("factory.test", None).is_ok(),
        "NEP-148 allows no icon"
    );

    let too_large = webp_data_url(8192);
    let png = "data:image/png;base64,iVBORw0KGgo=".to_string();
    let not_base64 = format!("{WEBP_PREFIX}<svg>");
    let empty = WEBP_PREFIX.to_string();
    for icon in [too_large, png, not_base64, empty] {
        let refusal = launched("factory.test", Some(&icon)).unwrap_err();
        assert_eq!(
            refusal.to_string(),
            "icon must be a WebP data URL (data:image/webp;base64,...) under 8192 bytes",
            "icon {}",
            &icon[..icon.len().min(40)]
        );
    }
}

#[test]
fn fc_rules_returns_every_fixed_value_with_brake_drop_bps_null() {
    let launched = launched("factory.test", None).expect("launch");
    let rules = serde_json::to_value(view_rules(&launched.state)).unwrap();
    let expected = json!({
        "creator": "creator.test",
        "launch_block": 100,
        "launch_ns": "100000000000",
        "p_bps": 200,
        "window_unmarked_s": 3600,
        "window_early_s": 86400,
        "early_blocks": 17,
        "large_threshold_bps": 200,
        "creator_cap_bps": 400,
        "real_cap_c_bps": 500,
        "floor_bps": 10,
        "penalty_bps": 1000,
        "virtual_near": (100 * ONE_NEAR).to_string(),
        "swap_fee_bps": 100,
        "early_fee_start_bps": 5000,
        "brake_drop_bps": null,
        "creator_fee_share_bps": 3000,
        "claim_min_real_near": (10 * ONE_NEAR).to_string(),
        "claim_depth_hold_s": 86400,
        "unclaimed_to_pool_s": null,
        "code_hash": "4reLvkAWfqk5fsqio1KLudk46cqRz9erQdaHkWZKMJDZ",
        "description": "A coin that walks.",
    });
    assert_eq!(rules, expected);
}

#[test]
fn fc_rules_returns_a_null_description_when_the_creator_gave_none() {
    let mut req = request("factory.test", None);
    req.description = None;
    let state = launch(FixedValues::fixed(), req, at(100)).unwrap().state;
    let rules = serde_json::to_value(view_rules(&state)).unwrap();
    assert_eq!(rules["description"], Value::Null);
}

#[test]
fn buys_and_sells_emit_one_ft_transfer_with_the_coin_as_the_other_party_and_no_ft_mint() {
    let state = launched("factory.test", None).unwrap().state;
    let bought = apply(
        &state,
        Holders::new(),
        &Action::Buy {
            account: "alice.test".into(),
            near_in: ONE_NEAR,
            min_tokens_out: 0,
        },
        at(101),
    )
    .unwrap();
    let tokens = bought.holders["alice.test"].balance;
    let sold = apply(
        &bought.state,
        bought.holders.clone(),
        &Action::Sell {
            account: "alice.test".into(),
            amount: tokens,
            min_near_out: 0,
            attached: 1,
        },
        at(102),
    )
    .unwrap();
    for (events, from, to) in [
        (&bought.events, "walk.factory.test", "alice.test"),
        (&sold.events, "alice.test", "walk.factory.test"),
    ] {
        assert!(!events.iter().any(|e| matches!(e, Event::FtMint(_))));
        let transfers: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::FtTransfer(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0].old_owner_id, from);
        assert_eq!(transfers[0].new_owner_id, to);
        assert_eq!(transfers[0].amount, tokens);
    }
}
