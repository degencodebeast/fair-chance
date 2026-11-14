//! `--coin <account>`: the run does not launch; it waits for a coin the app launches. The coin must
//! be a direct sub-account of the factory, and the flags that only make sense for a run that
//! launches (or for the BAG steps) are refused with it, before anything is read or signed.

use fair_chance_sandbox_tests::app_coin::app_coin_request;
use fair_chance_sandbox_tests::option_t::{parse_arguments, OptionTArguments};

fn parse(args: &[&str]) -> anyhow::Result<OptionTArguments> {
    parse_arguments(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
}

fn factory() -> near_api::types::AccountId {
    "factory.fairchance.testnet".parse().unwrap()
}

#[test]
fn the_coin_flag_parses_once() {
    let parsed = parse(&["--coin", "cabal3.factory.fairchance.testnet"]).unwrap();
    assert_eq!(
        parsed.coin.as_deref(),
        Some("cabal3.factory.fairchance.testnet")
    );
    assert!(parse(&[
        "--coin",
        "a.factory.fairchance.testnet",
        "--coin",
        "b.factory.fairchance.testnet"
    ])
    .is_err());
    assert!(parse(&["--coin"]).is_err());
}

#[test]
fn no_coin_flag_is_the_launching_run() {
    let parsed = parse(&["--buyers", "10"]).unwrap();
    assert!(app_coin_request(&parsed, &factory()).unwrap().is_none());
}

#[test]
fn a_coin_under_the_factory_gives_its_label_as_the_account_ticker() {
    let parsed = parse(&[
        "--coin",
        "cabal3.factory.fairchance.testnet",
        "--buyers",
        "10",
        "--later-buy-millinear",
        "300",
    ])
    .unwrap();
    let request = app_coin_request(&parsed, &factory()).unwrap().unwrap();
    assert_eq!(request.coin.as_str(), "cabal3.factory.fairchance.testnet");
    assert_eq!(request.label, "cabal3");
}

#[test]
fn a_coin_that_is_not_a_direct_sub_account_of_the_factory_is_refused() {
    for coin in [
        "cabal3.fairchance.testnet",
        "cabal3.other-factory.testnet",
        "x.cabal3.factory.fairchance.testnet",
        "factory.fairchance.testnet",
        "Cabal3.factory.fairchance.testnet",
        "not an account",
    ] {
        let parsed = parse(&["--coin", coin]).unwrap();
        let refused = app_coin_request(&parsed, &factory())
            .expect_err(coin)
            .to_string();
        assert!(refused.contains("nothing was"), "{coin}: {refused}");
    }
}

#[test]
fn the_flags_of_a_launching_run_or_a_bag_step_are_refused_with_the_coin_flag() {
    for extra in [
        &["--ticker", "CABAL3"][..],
        &["--name", "Cabal Test 3"],
        &["--description", "x"],
        &["--launch-deposit-millinear", "1000"],
        &["--sells-only"],
        &["--delete-buyers"],
        &["--continue"],
        &["--return-to-funder"],
        &["--sell-plan", "plan.json"],
    ] {
        let mut args = vec!["--coin", "cabal3.factory.fairchance.testnet"];
        args.extend_from_slice(extra);
        let parsed = parse(&args).unwrap();
        let refused = app_coin_request(&parsed, &factory())
            .expect_err(extra[0])
            .to_string();
        assert!(
            refused.contains(extra[0]) && refused.contains("nothing was"),
            "{extra:?}: {refused}"
        );
    }
}

#[test]
fn the_app_coin_run_reserves_100_tgas_per_buy_unless_told_otherwise() {
    use fair_chance_sandbox_tests::app_coin::buyer_gas;
    use near_api::types::NearGas;
    let coin = parse(&["--coin", "cabal3.factory.fairchance.testnet"]).unwrap();
    assert_eq!(buyer_gas(&coin), NearGas::from_tgas(100));
    let told = parse(&[
        "--coin",
        "cabal3.factory.fairchance.testnet",
        "--buyer-tgas",
        "300",
    ])
    .unwrap();
    assert_eq!(buyer_gas(&told), NearGas::from_tgas(300));
    // The run that launches keeps its own default.
    assert_eq!(
        buyer_gas(&parse(&["--buyers", "10"]).unwrap()),
        NearGas::from_tgas(300)
    );
}
