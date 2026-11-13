use fc_rules::{apply, launch, Action, At, CoinState, Event, Holders, Rules};

const BUYER: &str = "alice.test";
const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;

/// Field names of `fc_sell` in INTERFACE.md 1.0.0-rc.4, copied from the document.
const FC_SELL_FIELDS: [&str; 16] = [
    "account",
    "class",
    "tokens_asked",
    "tokens_sold",
    "marked_sold",
    "near_out",
    "fee_near",
    "creator_fee_near",
    "penalty_near",
    "fill",
    "limit_reason",
    "allowance_used",
    "allowance_left",
    "cap_used",
    "cap_left",
    "window_end_ns",
];

fn launched() -> CoinState {
    launch(
        Rules::fixed(),
        "coin.test",
        "creator.test",
        "hash",
        At {
            block: 100,
            ns: 1_000,
        },
    )
    .state
}

fn buy(state: &CoinState, holders: Holders, near_in: u128, at: At) -> (CoinState, Holders, u128) {
    let applied = apply(
        state,
        holders,
        &Action::Buy {
            account: BUYER.into(),
            near_in,
            min_tokens_out: 0,
        },
        at,
    )
    .expect("buy");
    let tokens = applied.holders[BUYER].balance;
    (applied.state, applied.holders, tokens)
}

#[test]
fn a_sell_after_a_buy_emits_fc_sell_with_the_interface_field_names() {
    let state = launched();
    let (state, holders, tokens) = buy(
        &state,
        Holders::new(),
        ONE_NEAR,
        At {
            block: 101,
            ns: 2_000,
        },
    );
    assert!(tokens > 0);

    let sold = apply(
        &state,
        holders,
        &Action::Sell {
            account: BUYER.into(),
            amount: tokens / 2,
            min_near_out: 0,
            attached: 1,
        },
        At {
            block: 102,
            ns: 3_000,
        },
    )
    .expect("sell");

    let fc_sell = sold
        .events
        .iter()
        .find_map(|e| match e {
            Event::FcSell(s) => Some(s),
            _ => None,
        })
        .expect("fc_sell event");
    let json = serde_json::to_value(fc_sell).unwrap();
    let mut names: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    names.sort_unstable();
    let mut expected = FC_SELL_FIELDS.to_vec();
    expected.sort_unstable();
    assert_eq!(names, expected);
    assert_eq!(json["account"], BUYER);
    assert_eq!(json["tokens_asked"], (tokens / 2).to_string());
}

#[test]
fn a_sell_never_pays_out_more_near_than_the_buy_put_in() {
    let state = launched();
    let (state, holders, tokens) = buy(
        &state,
        Holders::new(),
        ONE_NEAR,
        At {
            block: 101,
            ns: 2_000,
        },
    );
    let sold = apply(
        &state,
        holders,
        &Action::Sell {
            account: BUYER.into(),
            amount: tokens,
            min_near_out: 0,
            attached: 1,
        },
        At {
            block: 102,
            ns: 3_000,
        },
    )
    .expect("sell");
    let payout = sold.payout.expect("payout");
    assert_eq!(payout.account, BUYER);
    assert!(payout.amount > 0 && payout.amount <= ONE_NEAR);
}

#[test]
fn a_sell_without_one_yocto_is_refused_with_fc_one_yocto() {
    let state = launched();
    let (state, holders, tokens) = buy(
        &state,
        Holders::new(),
        ONE_NEAR,
        At {
            block: 101,
            ns: 2_000,
        },
    );
    let refusal = apply(
        &state,
        holders,
        &Action::Sell {
            account: BUYER.into(),
            amount: tokens,
            min_near_out: 0,
            attached: 0,
        },
        At {
            block: 102,
            ns: 3_000,
        },
    )
    .unwrap_err();
    assert!(refusal.to_string().starts_with("FC_ONE_YOCTO"));
}

#[test]
fn the_input_state_does_not_change_when_an_action_applies() {
    let state = launched();
    let before = state.clone();
    let _ = buy(
        &state,
        Holders::new(),
        ONE_NEAR,
        At {
            block: 101,
            ns: 2_000,
        },
    );
    assert_eq!(state, before);
}
