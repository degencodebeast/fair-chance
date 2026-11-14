//! The return step after the BAG sells: bag-buyer-01 to -18 each send their NEAR, minus a fixed
//! keep, back to the funder, once. bag-buyer-19 and -20 are never touched. The step runs only with
//! --sells-only on the BAG coin of the testnet factory, and it reads the existing plan file.

use fair_chance_sandbox_tests::option_t::parse_arguments;
use fair_chance_sandbox_tests::sells_only::{
    check_return_step, fixed_accounts, plan_from_balances, plan_returns, return_request, SellPlan,
    SellerState, RETURN_KEEP_YOCTO,
};
use near_api::types::AccountId;

fn id(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn funder() -> AccountId {
    id("fairchance.testnet")
}

const NEAR: u128 = 1_000_000_000_000_000_000_000_000;

/// The sells plan: seller i was planned to sell 1,000 + i BAG units.
fn sells_plan() -> SellPlan {
    let (sellers, _) = fixed_accounts("BAG", &funder()).unwrap();
    let balances: Vec<_> = sellers
        .into_iter()
        .enumerate()
        .map(|(i, a)| (a, 1_000 + i as u128))
        .collect();
    plan_from_balances(
        &id("bag.factory.fairchance.testnet"),
        "BAG",
        &funder(),
        &balances,
    )
    .unwrap()
}

/// After the sells: every seller holds 0 BAG and 2.4 NEAR, and uses 182 bytes of storage.
fn sold_states() -> Vec<SellerState> {
    sells_plan()
        .steps
        .iter()
        .map(|s| SellerState {
            account: s.account.clone(),
            bag: 0,
            near: 24 * NEAR / 10,
            storage_bytes: 182,
        })
        .collect()
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

const SELLS_ONLY: [&str; 7] = [
    "--network",
    "testnet",
    "--ticker",
    "BAG",
    "--sells-only",
    "--sell-plan",
    "/tmp/p.json",
];

#[test]
fn the_return_flag_runs_only_with_the_sells_only_step() {
    let mut with_return = SELLS_ONLY.to_vec();
    with_return.extend([
        "--return-to-funder",
        "--plan-sha256",
        "ae04e7eeb1c003d8550b3a15eb2243ebf5cd3201467600456581161431b03aa7",
    ]);
    assert!(return_request(&parse_arguments(&args(&with_return)).unwrap()).unwrap());
    assert!(!return_request(&parse_arguments(&args(&SELLS_ONLY)).unwrap()).unwrap());
    assert!(!return_request(&parse_arguments(&[]).unwrap()).unwrap());

    for bad in [
        args(&["--return-to-funder"]),
        args(&[
            "--network",
            "testnet",
            "--ticker",
            "BAG",
            "--return-to-funder",
        ]),
        args(&[
            "--network",
            "testnet",
            "--ticker",
            "FOO",
            "--sells-only",
            "--sell-plan",
            "/tmp/p.json",
            "--return-to-funder",
        ]),
        args(&[
            "--ticker",
            "BAG",
            "--sells-only",
            "--sell-plan",
            "/tmp/p.json",
            "--return-to-funder",
        ]),
    ] {
        let error = parse_arguments(&bad)
            .and_then(|parsed| return_request(&parsed))
            .unwrap_err();
        assert!(
            error.to_string().contains("nothing was run"),
            "{bad:?}: {error}"
        );
    }
    let mut twice = with_return.clone();
    twice.push("--return-to-funder");
    assert!(parse_arguments(&args(&twice)).is_err());
}

#[test]
fn each_seller_returns_its_near_minus_the_fixed_keep() {
    assert_eq!(RETURN_KEEP_YOCTO, 50_000_000_000_000_000_000_000); // 0.05 NEAR
    let plan = plan_returns(&sells_plan(), &funder(), &sold_states()).unwrap();
    assert_eq!(plan.steps.len(), 18);
    assert_eq!(plan.steps[0].account, id("bag-buyer-01.fairchance.testnet"));
    assert_eq!(
        plan.steps[17].account,
        id("bag-buyer-18.fairchance.testnet")
    );
    for step in &plan.steps {
        assert_eq!(step.amount, 2_350_000_000_000_000_000_000_000); // 2.4 - 0.05 NEAR
        assert_eq!(step.near_before, 2_400_000_000_000_000_000_000_000);
    }
    assert!(plan.skipped.is_empty());
}

#[test]
fn a_partly_filled_sell_still_returns_but_a_sell_that_did_not_run_stops_the_step() {
    let mut states = sold_states();
    states[10].bag = 390; // buyer-11 planned 1,010 and sold 620: it sold, so it returns
    let plan = plan_returns(&sells_plan(), &funder(), &states).unwrap();
    assert_eq!(plan.steps.len(), 18);

    states[13].bag = 1_013; // buyer-14 still holds its whole planned amount: its sell did not run
    let error = plan_returns(&sells_plan(), &funder(), &states).unwrap_err();
    assert!(error.to_string().contains("bag-buyer-14"), "{error}");
    assert!(error.to_string().contains("nothing was signed"), "{error}");
}

#[test]
fn buyers_19_and_20_or_any_other_list_are_refused() {
    let (_, keepers) = fixed_accounts("BAG", &funder()).unwrap();
    let mut with_keeper = sold_states();
    with_keeper[17].account = keepers[0].clone();
    let mut reordered = sold_states();
    reordered.swap(0, 1);
    let mut short = sold_states();
    short.pop();
    let mut extra = sold_states();
    extra.push(SellerState {
        account: keepers[1].clone(),
        bag: 0,
        near: NEAR,
        storage_bytes: 182,
    });
    for bad in [with_keeper, reordered, short, extra] {
        let error = plan_returns(&sells_plan(), &funder(), &bad).unwrap_err();
        assert!(error.to_string().contains("nothing was signed"), "{error}");
    }
}

#[test]
fn a_seller_at_or_below_the_keep_is_skipped_and_too_much_storage_stops_the_step() {
    let mut states = sold_states();
    states[2].near = RETURN_KEEP_YOCTO; // returned already, or nothing to return
    states[3].near = RETURN_KEEP_YOCTO + 1;
    let plan = plan_returns(&sells_plan(), &funder(), &states).unwrap();
    assert_eq!(plan.steps.len(), 17);
    assert_eq!(plan.skipped.len(), 1);
    assert_eq!(plan.skipped[0].0, id("bag-buyer-03.fairchance.testnet"));
    assert_eq!(plan.steps[2].account, id("bag-buyer-04.fairchance.testnet"));
    assert_eq!(plan.steps[2].amount, 1);

    // 2,600 bytes cost 0.026 NEAR: more than half the keep, so the keep may not cover deletion.
    let mut heavy = sold_states();
    heavy[5].storage_bytes = 2_600;
    let error = plan_returns(&sells_plan(), &funder(), &heavy).unwrap_err();
    assert!(error.to_string().contains("bag-buyer-06"), "{error}");
}

#[test]
fn a_transfer_goes_out_only_while_the_seller_still_holds_the_near_read_before() {
    let plan = plan_returns(&sells_plan(), &funder(), &sold_states()).unwrap();
    let step = &plan.steps[4];
    check_return_step(step, 2_400_000_000_000_000_000_000_000).unwrap();
    for now in [
        2_399_999_999_999_999_999_999_999,
        2_400_000_000_000_000_000_000_001,
        RETURN_KEEP_YOCTO,
    ] {
        let error = check_return_step(step, now).unwrap_err();
        assert!(
            error.to_string().contains("nothing more was signed"),
            "{error}"
        );
        assert!(error.to_string().contains("bag-buyer-05"), "{error}");
    }
}

#[test]
fn the_plan_lists_for_each_seller_the_bag_left_and_the_near_returned() {
    let mut states = sold_states();
    states[10].bag = 390;
    states[2].near = RETURN_KEEP_YOCTO;
    let plan = plan_returns(&sells_plan(), &funder(), &states).unwrap();
    let lines = plan.lines();
    assert_eq!(lines.len(), 18);
    assert_eq!(
        lines[0],
        "bag-buyer-01.fairchance.testnet: BAG left 0, NEAR returned 2350000000000000000000000 yocto (keeps 50000000000000000000000)"
    );
    assert_eq!(
        lines[2],
        "bag-buyer-03.fairchance.testnet: BAG left 0, NEAR returned 0 (skipped: holds 50000000000000000000000 yoctoNEAR, not more than the keep)"
    );
    assert_eq!(
        lines[10],
        "bag-buyer-11.fairchance.testnet: BAG left 390, NEAR returned 2350000000000000000000000 yocto (keeps 50000000000000000000000)"
    );
}
