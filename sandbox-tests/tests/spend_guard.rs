//! Review finding 1: before each step that signs or spends, the run reads the funder balance and
//! refuses when the spend from the start balance would go above the cap, or when the balance after
//! the step would fall below the amount to keep. These tests use fake balances.

use fair_chance_sandbox_tests::factory_deploy::{parse_deploy_arguments, DeployMode};
use fair_chance_sandbox_tests::option_t::{choose_spend_limits, parse_arguments, DemoNetwork};
use fair_chance_sandbox_tests::spend_guard::{parse_near, SpendLimits};
use near_api::types::NearToken;

fn near(text: &str) -> NearToken {
    parse_near(text).unwrap()
}

fn limits() -> SpendLimits {
    SpendLimits {
        start: near("69.89859448325"),
        cap: near("65"),
        keep: near("2"),
    }
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|a| a.to_string()).collect()
}

#[test]
fn near_amounts_parse_exactly_and_bad_text_is_refused() {
    assert_eq!(
        near("69.89859448325").as_yoctonear(),
        69_898_594_483_250_000_000_000_000
    );
    assert_eq!(near("65"), NearToken::from_near(65));
    assert_eq!(near("0.000000000000000000000001").as_yoctonear(), 1);
    for bad in [
        "",
        "abc",
        "-1",
        "1.",
        ".5",
        "1.0000000000000000000000001",
        "1e3",
        " 1",
    ] {
        assert!(parse_near(bad).is_err(), "{bad:?} parsed");
    }
}

#[test]
fn a_step_inside_the_cap_and_above_the_floor_passes() {
    let l = limits();
    l.check(l.start, near("26.44361"), "global deploy").unwrap();
    // Exactly at the cap is still inside it.
    let balance = l.start.saturating_sub(near("60"));
    l.check(balance, near("5"), "buyer funding").unwrap();
}

#[test]
fn a_step_that_would_pass_the_cap_is_refused_and_named() {
    let l = limits();
    let balance = l.start.saturating_sub(near("60"));
    let error = l
        .check(balance, near("5.1"), "buyer funding bag-buyer-07")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("bag-buyer-07") && error.contains("65"),
        "{error}"
    );
}

#[test]
fn a_step_that_would_leave_less_than_the_floor_is_refused() {
    let l = SpendLimits {
        start: near("10"),
        cap: near("65"),
        keep: near("2"),
    };
    let error = l
        .check(near("3"), near("1.5"), "launch")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("launch") && error.contains("2 NEAR"),
        "{error}"
    );
    // A balance already below the floor refuses even a step that costs the funder nothing.
    assert!(l.check(near("1.9"), near("0"), "sell").is_err());
}

#[test]
fn a_top_up_above_the_start_counts_as_nothing_spent() {
    let l = limits();
    l.check(
        l.start.saturating_add(near("5")),
        near("65"),
        "global deploy",
    )
    .unwrap();
}

#[test]
fn the_testnet_deploy_needs_all_three_limits() {
    let full = [
        "--network",
        "testnet",
        "--start-balance",
        "69.89859448325",
        "--spend-cap",
        "65",
        "--keep",
        "2",
    ];
    assert_eq!(
        parse_deploy_arguments(&strings(&full), Some("yes")).unwrap(),
        DeployMode::Testnet(limits())
    );
    for cut in [2, 4, 6] {
        let mut list = full.to_vec();
        list.drain(cut..cut + 2);
        assert!(
            parse_deploy_arguments(&strings(&list), Some("yes")).is_err(),
            "{list:?} accepted"
        );
    }
}

#[test]
fn option_t_on_testnet_needs_all_three_limits_and_sandbox_needs_none() {
    let full = strings(&[
        "--start-balance",
        "69.89859448325",
        "--spend-cap",
        "65",
        "--keep",
        "2",
    ]);
    let args = parse_arguments(&full).unwrap();
    assert_eq!(
        choose_spend_limits(&args, DemoNetwork::Testnet).unwrap(),
        Some(limits())
    );
    let none = parse_arguments(&[]).unwrap();
    assert!(choose_spend_limits(&none, DemoNetwork::Testnet).is_err());
    assert_eq!(
        choose_spend_limits(&none, DemoNetwork::Sandbox).unwrap(),
        None
    );
    let part = parse_arguments(&strings(&["--spend-cap", "65"])).unwrap();
    assert!(choose_spend_limits(&part, DemoNetwork::Testnet).is_err());
}
