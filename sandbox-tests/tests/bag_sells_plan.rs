//! The BAG sells-only step (builder decision 14 Nov 00:25 Lagos): bag-buyer-01 to -18 each sell
//! their full balance, one after another in that order, with 1 yoctoNEAR and min_near_out 0.
//! bag-buyer-19 and -20 never sell. The code refuses any other order or size, and every step line
//! carries a timestamp.

use fair_chance_sandbox_tests::option_t::parse_arguments;
use fair_chance_sandbox_tests::sells_only::{
    check_plan_file, check_step, fixed_accounts, plan_from_balances, plan_json, sells_only_request,
    RULE, SELLERS,
};
use fair_chance_sandbox_tests::stamp_at;
use near_api::types::AccountId;

fn id(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn funder() -> AccountId {
    id("fairchance.testnet")
}

fn coin() -> AccountId {
    id("bag.factory.fairchance.testnet")
}

fn balances() -> Vec<(AccountId, u128)> {
    let (sellers, _) = fixed_accounts("BAG", &funder()).unwrap();
    sellers
        .into_iter()
        .enumerate()
        .map(|(i, a)| (a, 1_000 + i as u128))
        .collect()
}

#[test]
fn the_sellers_are_buyers_01_to_18_in_order_and_19_and_20_keep() {
    let (sellers, keepers) = fixed_accounts("BAG", &funder()).unwrap();
    assert_eq!(SELLERS, 18);
    assert_eq!(sellers.len(), 18);
    for (i, a) in sellers.iter().enumerate() {
        assert_eq!(
            a.as_str(),
            format!("bag-buyer-{:02}.fairchance.testnet", i + 1)
        );
    }
    assert_eq!(
        keepers,
        vec![
            id("bag-buyer-19.fairchance.testnet"),
            id("bag-buyer-20.fairchance.testnet")
        ]
    );
    assert!(RULE.contains("bag-buyer-01 to bag-buyer-18") && RULE.contains("never sell"));
}

#[test]
fn a_plan_with_another_order_a_keeper_a_gap_or_an_empty_balance_is_refused() {
    let good = plan_from_balances(&coin(), "BAG", &funder(), &balances()).unwrap();
    assert_eq!(good.steps.len(), 18);
    assert_eq!(good.steps[4].amount, 1_004);

    let mut swapped = balances();
    swapped.swap(0, 1);
    let mut keeper = balances();
    keeper[17].0 = id("bag-buyer-19.fairchance.testnet");
    let mut short = balances();
    short.pop();
    let mut empty = balances();
    empty[6].1 = 0;
    for (name, bad) in [
        ("swapped", swapped),
        ("keeper", keeper),
        ("short", short),
        ("empty", empty),
    ] {
        let error = plan_from_balances(&coin(), "BAG", &funder(), &bad).unwrap_err();
        assert!(
            error.to_string().contains("nothing was signed"),
            "{name}: {error}"
        );
    }
}

#[test]
fn an_edited_plan_file_is_refused() {
    let plan = plan_from_balances(&coin(), "BAG", &funder(), &balances()).unwrap();
    let text = serde_json::to_string_pretty(&plan_json(&plan)).unwrap();
    check_plan_file(&text, &plan).unwrap();
    let file = plan_json(&plan);
    assert_eq!(file["rule"], RULE);
    assert_eq!(file["coin"], "bag.factory.fairchance.testnet");
    assert_eq!(file["sells"].as_array().unwrap().len(), 18);
    assert_eq!(file["sells"][0]["order"], 1);
    assert_eq!(file["sells"][0]["amount"], "1000");
    assert_eq!(file["keep"][1], "bag-buyer-20.fairchance.testnet");

    let mut bigger = file.clone();
    bigger["sells"][3]["amount"] = "9999".into();
    let mut reordered = file.clone();
    let first = reordered["sells"][0].clone();
    reordered["sells"][0] = reordered["sells"][1].clone();
    reordered["sells"][1] = first;
    for bad in [bigger, reordered] {
        let error = check_plan_file(&bad.to_string(), &plan).unwrap_err();
        assert!(error.to_string().contains("nothing was signed"), "{error}");
    }
}

#[test]
fn each_sell_must_be_the_next_account_with_the_planned_full_balance() {
    let plan = plan_from_balances(&coin(), "BAG", &funder(), &balances()).unwrap();
    let a = |i: usize| plan.steps[i].account.clone();
    check_step(&plan, 2, &a(2), 1_002, 1_002).unwrap();
    for (index, account, amount, on_chain) in [
        (2, a(3), 1_003, 1_003),                           // not the next account
        (2, a(2), 500, 1_002),                             // not the full balance
        (2, a(2), 1_002, 1_500),                           // the chain balance moved
        (18, id("bag-buyer-19.fairchance.testnet"), 1, 1), // a keeper
    ] {
        let error = check_step(&plan, index, &account, amount, on_chain).unwrap_err();
        assert!(
            error.to_string().contains("nothing more was signed"),
            "{error}"
        );
    }
}

#[test]
fn the_sells_only_flags_need_each_other_and_a_ticker() {
    let args = |list: &[&str]| -> Vec<String> { list.iter().map(|s| s.to_string()).collect() };
    let parsed = parse_arguments(&args(&[
        "--sells-only",
        "--sell-plan",
        "/tmp/p.json",
        "--ticker",
        "BAG",
    ]))
    .unwrap();
    assert_eq!(
        sells_only_request(&parsed).unwrap().as_deref(),
        Some(std::path::Path::new("/tmp/p.json"))
    );
    assert_eq!(
        sells_only_request(&parse_arguments(&[]).unwrap()).unwrap(),
        None
    );
    for bad in [
        args(&["--sells-only", "--ticker", "BAG"]),
        args(&["--sell-plan", "/tmp/p.json", "--ticker", "BAG"]),
        args(&["--sells-only", "--sell-plan", "/tmp/p.json"]),
        args(&[
            "--sells-only",
            "--sell-plan",
            "/tmp/p.json",
            "--ticker",
            "BAG",
            "--delete-buyers",
        ]),
    ] {
        let parsed = parse_arguments(&bad).unwrap();
        assert!(sells_only_request(&parsed).is_err(), "{bad:?}");
    }
}

#[test]
fn every_step_line_starts_with_a_utc_timestamp() {
    assert_eq!(stamp_at(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(stamp_at(1_794_609_615_288), "2026-11-13T22:40:15.288Z");
}
