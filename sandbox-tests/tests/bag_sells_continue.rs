//! The continue step after a sells run that stopped part way: it reads the existing plan file
//! (with its sha256), skips the sellers that are done, and sells the rest in order. A done seller
//! holds 0 BAG and so does every seller before it; every seller after the first one with BAG must
//! still hold exactly its planned amount. Anything else stops the step before any signature.

use fair_chance_sandbox_tests::option_t::parse_arguments;
use fair_chance_sandbox_tests::sells_only::{
    continue_from, continue_request, fixed_accounts, plan_from_balances, SellPlan,
};
use near_api::types::AccountId;

fn id(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn plan() -> SellPlan {
    let funder = id("fairchance.testnet");
    let (sellers, _) = fixed_accounts("BAG", &funder).unwrap();
    let b: Vec<_> = sellers
        .into_iter()
        .enumerate()
        .map(|(i, a)| (a, 1_000 + i as u128))
        .collect();
    plan_from_balances(&id("bag.factory.fairchance.testnet"), "BAG", &funder, &b).unwrap()
}

/// The chain now: sellers before `done` hold 0 BAG, the rest hold their planned amount.
fn now(done: usize) -> Vec<(AccountId, u128)> {
    plan()
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| (s.account.clone(), if i < done { 0 } else { s.amount }))
        .collect()
}

#[test]
fn the_step_continues_after_the_done_sellers() {
    assert_eq!(continue_from(&plan(), &now(3)).unwrap(), 3); // the stopped run: 01..03 sold
    assert_eq!(continue_from(&plan(), &now(0)).unwrap(), 0);
    assert_eq!(continue_from(&plan(), &now(17)).unwrap(), 17);
    assert_eq!(continue_from(&plan(), &now(18)).unwrap(), 18); // nothing left to sell
}

#[test]
fn any_other_chain_state_stops_the_step_before_any_signature() {
    let mut gap = now(3);
    gap[6].1 = 0; // bag-buyer-07 holds 0 after a seller that still holds BAG
    let mut partial = now(3);
    partial[3].1 = 500; // bag-buyer-04 holds less than planned
    let mut more = now(3);
    more[10].1 += 1; // bag-buyer-11 holds more than planned
    let mut partial_first = now(0);
    partial_first[0].1 = 1; // bag-buyer-01 sold part
    let mut reordered = now(3);
    reordered.swap(4, 5);
    let mut short = now(3);
    short.pop();
    let mut keeper = now(3);
    keeper[17].0 = id("bag-buyer-19.fairchance.testnet");
    for (name, bad) in [
        ("gap", gap),
        ("partial", partial),
        ("more", more),
        ("partial first", partial_first),
        ("reordered", reordered),
        ("short", short),
        ("keeper", keeper),
    ] {
        let error = continue_from(&plan(), &bad).unwrap_err();
        assert!(
            error.to_string().contains("nothing was signed"),
            "{name}: {error}"
        );
    }
}

#[test]
fn the_continue_flag_needs_the_sells_only_step_and_the_plan_sha256() {
    let args = |l: &[&str]| -> Vec<String> { l.iter().map(|s| s.to_string()).collect() };
    let sha = "cb36667b628fa837fb30f8cc9a5a1b834a440b0aeb766f43b84652339f538bc8";
    let ok = [
        "--network",
        "testnet",
        "--ticker",
        "BAG",
        "--sells-only",
        "--sell-plan",
        "/tmp/p.json",
        "--continue",
        "--plan-sha256",
        sha,
    ];
    assert!(continue_request(&parse_arguments(&args(&ok)).unwrap()).unwrap());
    assert!(!continue_request(&parse_arguments(&args(&ok[..7])).unwrap()).unwrap());
    for bad in [
        args(&["--continue", "--plan-sha256", sha]),
        args(&ok[..8]), // no --plan-sha256
        args(&[&ok[..], &["--return-to-funder"]].concat()),
        args(&[
            "--network",
            "testnet",
            "--ticker",
            "FOO",
            "--sells-only",
            "--sell-plan",
            "/tmp/p.json",
            "--continue",
            "--plan-sha256",
            sha,
        ]),
    ] {
        let error = parse_arguments(&bad)
            .and_then(|p| continue_request(&p))
            .unwrap_err();
        assert!(
            error.to_string().contains("nothing was run"),
            "{bad:?}: {error}"
        );
    }
    assert!(parse_arguments(&args(&[&ok[..], &["--continue"]].concat())).is_err());
}
