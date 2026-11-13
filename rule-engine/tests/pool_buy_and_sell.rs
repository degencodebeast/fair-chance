//! Pool rules through the engine interface: constant product on virtual NEAR P = 100 plus real
//! NEAR, a 1% swap fee that stays in the pool, slippage, 1 yoctoNEAR and the NEAR reserve
//! minimum. Expected values are worked examples computed outside this code.

use rule_engine::{
    apply, launch, quote_buy, view_pool, Action, At, CoinState, Event, FcBuy, FcSell, FixedValues,
    Holders, LaunchRequest,
};

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const SUPPLY: u128 = 1_000_000_000_000_000_000_000_000_000;
const ALICE: &str = "alice.test";

/// Block `block` of the test, at `block` seconds. Actions after the launch at block 100 run 18
/// blocks later, after the early window (blocks 0 to 17), so their buys are unmarked.
fn at(block: u64) -> At {
    At {
        block: if block > 100 { block + 18 } else { block },
        ns: block * 1_000_000_000,
    }
}

fn launched() -> CoinState {
    launch(
        FixedValues::fixed(),
        LaunchRequest {
            coin: "coin.factory.test",
            caller: "factory.test",
            creator: "creator.test",
            code_hash: "hash",
            name: "Test Coin",
            symbol: "TEST",
            icon: None,
            description: None,
        },
        at(100),
    )
    .expect("launch")
    .state
}

fn buy_action(account: &str, near_in: u128, min_tokens_out: u128) -> Action {
    Action::Buy {
        account: account.into(),
        near_in,
        min_tokens_out,
    }
}

fn sell_action(account: &str, amount: u128, min_near_out: u128) -> Action {
    Action::Sell {
        account: account.into(),
        amount,
        min_near_out,
        attached: 1,
    }
}

fn fc_buy(events: &[Event]) -> &FcBuy {
    events
        .iter()
        .find_map(|e| match e {
            Event::FcBuy(b) => Some(b),
            _ => None,
        })
        .expect("fc_buy")
}

fn fc_sell(events: &[Event]) -> &FcSell {
    events
        .iter()
        .find_map(|e| match e {
            Event::FcSell(s) => Some(s),
            _ => None,
        })
        .expect("fc_sell")
}

#[test]
fn a_buy_of_one_near_pays_a_one_percent_fee_that_stays_in_the_pool() {
    let state = launched();
    let applied = apply(
        &state,
        Holders::new(),
        &buy_action(ALICE, ONE_NEAR, 0),
        at(101),
    )
    .unwrap();
    let event = fc_buy(&applied.events);
    assert_eq!(event.near_in, ONE_NEAR);
    assert_eq!(event.fee_near, 10_000_000_000_000_000_000_000);
    assert_eq!(event.tokens_out, 9_802_950_787_206_654_124_170_709);
    assert_eq!(event.creator_fee_near, 0);
    assert_eq!(applied.holders[ALICE].balance, event.tokens_out);

    let pool = view_pool(&applied.state, at(101));
    assert_eq!(pool.real_near, ONE_NEAR);
    assert_eq!(pool.near_reserve, 101 * ONE_NEAR);
    assert_eq!(pool.token_reserve, SUPPLY - event.tokens_out);
}

#[test]
fn selling_the_bought_tokens_back_pays_the_fee_again_and_leaves_both_fees_in_the_pool() {
    let state = launched();
    let bought = apply(
        &state,
        Holders::new(),
        &buy_action(ALICE, ONE_NEAR, 0),
        at(101),
    )
    .unwrap();
    let tokens = fc_buy(&bought.events).tokens_out;
    let sold = apply(
        &bought.state,
        bought.holders,
        &sell_action(ALICE, tokens, 0),
        at(102),
    )
    .unwrap();
    let event = fc_sell(&sold.events);
    assert_eq!(event.tokens_sold, tokens);
    assert_eq!(event.fee_near, 9_900_980_295_078_720_665_413);
    assert_eq!(event.near_out, 980_197_049_212_793_345_875_828);
    assert_eq!(sold.payout.as_ref().unwrap().amount, event.near_out);

    let pool = view_pool(&sold.state, at(102));
    assert_eq!(pool.token_reserve, SUPPLY);
    assert_eq!(pool.real_near, 19_802_950_787_206_654_124_172);
}

#[test]
fn each_buy_raises_the_price_and_each_sell_lowers_it() {
    let mut state = launched();
    let mut holders = Holders::new();
    let mut last_tokens = u128::MAX;
    for block in 101..106 {
        let applied = apply(&state, holders, &buy_action(ALICE, ONE_NEAR, 0), at(block)).unwrap();
        let tokens = fc_buy(&applied.events).tokens_out;
        assert!(
            tokens < last_tokens,
            "one NEAR buys fewer tokens after each buy"
        );
        last_tokens = tokens;
        state = applied.state;
        holders = applied.holders;
    }
    let chunk = holders[ALICE].balance / 5;
    let mut last_near = u128::MAX;
    for block in 106..111 {
        let applied = apply(&state, holders, &sell_action(ALICE, chunk, 0), at(block)).unwrap();
        let near = fc_sell(&applied.events).near_out;
        assert!(
            near < last_near,
            "the same tokens pay less NEAR after each sell"
        );
        last_near = near;
        state = applied.state;
        holders = applied.holders;
    }
}

#[test]
fn a_buy_below_min_tokens_out_is_refused_with_fc_slippage_and_never_fills_in_part() {
    let state = launched();
    let exact = 9_802_950_787_206_654_124_170_709;
    let refusal = apply(
        &state,
        Holders::new(),
        &buy_action(ALICE, ONE_NEAR, exact + 1),
        at(101),
    )
    .unwrap_err();
    assert_eq!(refusal.to_string(), "FC_SLIPPAGE");
    let applied = apply(
        &state,
        Holders::new(),
        &buy_action(ALICE, ONE_NEAR, exact),
        at(101),
    )
    .unwrap();
    assert_eq!(applied.holders[ALICE].balance, exact);
}

#[test]
fn a_sell_below_min_near_out_is_refused_with_fc_slippage() {
    let state = launched();
    let bought = apply(
        &state,
        Holders::new(),
        &buy_action(ALICE, ONE_NEAR, 0),
        at(101),
    )
    .unwrap();
    let tokens = fc_buy(&bought.events).tokens_out;
    let near_out = 980_197_049_212_793_345_875_828;
    let refusal = apply(
        &bought.state,
        bought.holders.clone(),
        &sell_action(ALICE, tokens, near_out + 1),
        at(102),
    )
    .unwrap_err();
    assert_eq!(refusal.to_string(), "FC_SLIPPAGE");
    let sold = apply(
        &bought.state,
        bought.holders,
        &sell_action(ALICE, tokens, near_out),
        at(102),
    )
    .unwrap();
    assert_eq!(fc_sell(&sold.events).near_out, near_out);
}

#[test]
fn a_sell_without_exactly_one_yocto_is_refused_with_fc_one_yocto() {
    let state = launched();
    let bought = apply(
        &state,
        Holders::new(),
        &buy_action(ALICE, ONE_NEAR, 0),
        at(101),
    )
    .unwrap();
    for attached in [0, 2, ONE_NEAR] {
        let refusal = apply(
            &bought.state,
            bought.holders.clone(),
            &Action::Sell {
                account: ALICE.into(),
                amount: 1,
                min_near_out: 0,
                attached,
            },
            at(102),
        )
        .unwrap_err();
        assert_eq!(refusal.to_string(), "FC_ONE_YOCTO", "attached {attached}");
    }
}

#[test]
fn fc_quote_buy_equals_the_real_buy() {
    let state = launched();
    let bought = apply(
        &state,
        Holders::new(),
        &buy_action(ALICE, 3 * ONE_NEAR, 0),
        at(101),
    )
    .unwrap();
    for near_in in [1, ONE_NEAR / 7, 2 * ONE_NEAR, 50 * ONE_NEAR] {
        let quote = quote_buy(
            &bought.state,
            bought.holders.clone(),
            "bob.test",
            near_in,
            at(102),
        )
        .expect("quote");
        let real = apply(
            &bought.state,
            bought.holders.clone(),
            &buy_action("bob.test", near_in, 0),
            at(102),
        )
        .unwrap();
        let event = fc_buy(&real.events);
        assert_eq!(quote.tokens_out, event.tokens_out, "near_in {near_in}");
        assert_eq!(quote.fee_near, event.fee_near);
        assert_eq!(quote.creator_fee_near, event.creator_fee_near);
        assert_eq!(quote.marked_tokens, event.marked_tokens);
        assert_eq!(quote.class_after, event.class_after);
    }
}

/// Many buyers in a fixed pseudo-random order, then every holder sells everything back in a
/// different order. No sell may pay out NEAR that buyers did not put in, and the NEAR reserve
/// never goes below virtual P.
#[test]
fn a_sell_never_pays_out_near_that_buyers_did_not_put_in() {
    let accounts: Vec<String> = (0..12).map(|i| format!("buyer{i}.test")).collect();
    let mut state = launched();
    let mut holders = Holders::new();
    let mut put_in = 0u128;
    let mut paid_out = 0u128;
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut block = 101;
    for _ in 0..60 {
        let who = &accounts[(next() % accounts.len() as u64) as usize];
        let near_in = (next() as u128 % (20 * ONE_NEAR)) + 1;
        let applied = apply(&state, holders, &buy_action(who, near_in, 0), at(block)).unwrap();
        put_in += near_in;
        state = applied.state;
        holders = applied.holders;
        block += 1;
        // Sometimes a holder sells part of its bag in between.
        if next() % 3 == 0 {
            let seller = &accounts[(next() % accounts.len() as u64) as usize];
            let balance = holders.get(seller).map_or(0, |h| h.balance);
            if balance > 0 {
                let amount = (next() as u128 % balance) + 1;
                let applied =
                    apply(&state, holders, &sell_action(seller, amount, 0), at(block)).unwrap();
                paid_out += applied.payout.unwrap().amount;
                state = applied.state;
                holders = applied.holders;
                block += 1;
            }
        }
        assert!(paid_out <= put_in);
        assert!(view_pool(&state, at(block)).near_reserve >= 100 * ONE_NEAR);
    }
    for who in accounts.iter().rev() {
        let balance = holders.get(who).map_or(0, |h| h.balance);
        if balance == 0 {
            continue;
        }
        let applied = apply(&state, holders, &sell_action(who, balance, 0), at(block)).unwrap();
        paid_out += applied.payout.unwrap().amount;
        state = applied.state;
        holders = applied.holders;
        block += 1;
        assert!(paid_out <= put_in);
        assert!(view_pool(&state, at(block)).near_reserve >= 100 * ONE_NEAR);
    }
    let pool = view_pool(&state, at(block));
    assert_eq!(
        pool.token_reserve, SUPPLY,
        "every token is back in the pool"
    );
    assert_eq!(pool.real_near, put_in - paid_out);
    assert!(pool.real_near > 0, "the fees stay in the pool");
}
