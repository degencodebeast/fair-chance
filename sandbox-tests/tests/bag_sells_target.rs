//! The sells-only step runs on one coin only: bag.factory.fairchance.testnet, made by
//! factory.fairchance.testnet, on testnet. Any other ticker, factory or network is refused before
//! any read or signature. Each sell is checked against the plan read back from the plan file.

use fair_chance_sandbox_tests::option_t::parse_arguments;
use fair_chance_sandbox_tests::sells_only::{
    check_sells_only_factory, check_step, fixed_accounts, plan_from_balances, plan_from_file,
    plan_json, sells_only_request, SELLS_ONLY_COIN, SELLS_ONLY_FACTORY,
};
use near_api::types::AccountId;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn id(s: &str) -> AccountId {
    s.parse().unwrap()
}

#[test]
fn only_the_bag_ticker_on_testnet_is_taken() {
    let ok = parse_arguments(&args(&[
        "--network",
        "testnet",
        "--ticker",
        "BAG",
        "--sells-only",
        "--sell-plan",
        "/tmp/p.json",
    ]))
    .unwrap();
    assert!(sells_only_request(&ok).unwrap().is_some());
    for bad in [
        args(&[
            "--network",
            "testnet",
            "--ticker",
            "FOO",
            "--sells-only",
            "--sell-plan",
            "/tmp/p.json",
        ]),
        args(&[
            "--network",
            "testnet",
            "--ticker",
            "DOGE",
            "--sells-only",
            "--sell-plan",
            "/tmp/p.json",
        ]),
        args(&[
            "--network",
            "testnet",
            "--ticker",
            "bag",
            "--sells-only",
            "--sell-plan",
            "/tmp/p.json",
        ]),
        args(&[
            "--network",
            "testnet",
            "--ticker",
            "BAGS",
            "--sells-only",
            "--sell-plan",
            "/tmp/p.json",
        ]),
        args(&[
            "--ticker",
            "BAG",
            "--sells-only",
            "--sell-plan",
            "/tmp/p.json",
        ]),
        args(&[
            "--network",
            "sandbox",
            "--ticker",
            "BAG",
            "--sells-only",
            "--sell-plan",
            "/tmp/p.json",
        ]),
    ] {
        let error = sells_only_request(&parse_arguments(&bad).unwrap()).unwrap_err();
        assert!(
            error.to_string().contains("nothing was run"),
            "{bad:?}: {error}"
        );
    }
}

#[test]
fn only_the_fairchance_testnet_factory_is_taken() {
    assert_eq!(SELLS_ONLY_FACTORY, "factory.fairchance.testnet");
    assert_eq!(SELLS_ONLY_COIN, "bag.factory.fairchance.testnet");
    assert_eq!(
        check_sells_only_factory(Some("factory.fairchance.testnet"))
            .unwrap()
            .as_str(),
        "bag.factory.fairchance.testnet"
    );
    for bad in [
        None,
        Some(""),
        Some("other-factory.testnet"),
        Some("factory.fairchance.testnet.evil.testnet"),
        Some("x.factory.fairchance.testnet"),
        Some(" factory.fairchance.testnet"),
        Some("factory.fairchance.near"),
    ] {
        let error = check_sells_only_factory(bad).unwrap_err();
        assert!(
            error.to_string().contains("nothing was run"),
            "{bad:?}: {error}"
        );
    }
}

#[test]
fn each_sell_is_checked_against_the_plan_read_back_from_the_file() {
    let funder = id("fairchance.testnet");
    let (sellers, _) = fixed_accounts("BAG", &funder).unwrap();
    let balances: Vec<_> = sellers
        .into_iter()
        .enumerate()
        .map(|(i, a)| (a, 1_000 + i as u128))
        .collect();
    let plan = plan_from_balances(&id(SELLS_ONLY_COIN), "BAG", &funder, &balances).unwrap();

    let text = serde_json::to_string_pretty(&plan_json(&plan)).unwrap();
    let read_back = plan_from_file(&text).unwrap();
    assert_eq!(read_back, plan);

    // A file whose step 4 says another amount: the in-memory step 4 no longer passes.
    let mut edited = plan_json(&plan);
    edited["sells"][3]["amount"] = "9999".into();
    let from_edited = plan_from_file(&edited.to_string()).unwrap();
    let step = &plan.steps[3];
    let error = check_step(&from_edited, 3, &step.account, step.amount, step.amount).unwrap_err();
    assert!(
        error.to_string().contains("nothing more was signed"),
        "{error}"
    );

    // A file whose step 1 names another account.
    let mut swapped = plan_json(&plan);
    swapped["sells"][0]["account"] = "bag-buyer-19.fairchance.testnet".into();
    let from_swapped = plan_from_file(&swapped.to_string()).unwrap();
    let step = &plan.steps[0];
    assert!(check_step(&from_swapped, 0, &step.account, step.amount, step.amount).is_err());

    for bad in [
        "not json".to_string(),
        r#"{"coin":"bag.factory.fairchance.testnet","sells":[{"order":1,"account":"a.testnet","amount":"x"}],"keep":[]}"#.to_string(),
        r#"{"coin":"bag.factory.fairchance.testnet","sells":[{"order":2,"account":"a.testnet","amount":"1"}],"keep":[]}"#.to_string(),
    ] {
        assert!(plan_from_file(&bad).is_err(), "{bad}");
    }
}
