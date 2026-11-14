//! Option T refuses any argument it does not know, in any form, so a typo never falls back to
//! the sandbox run or skips a guard.

use fair_chance_sandbox_tests::option_t::parse_arguments;

fn parse(args: &[&str]) -> anyhow::Result<fair_chance_sandbox_tests::option_t::OptionTArguments> {
    parse_arguments(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
}

#[test]
fn the_known_flags_parse() {
    let parsed = parse(&[
        "--network",
        "testnet",
        "--ticker",
        "BAG",
        "--name",
        "Bag Holder",
        "--description",
        "A coin that holds its bags.",
        "--buy-millinear",
        "500",
        "--buyer-extra-millinear",
        "200",
        "--buyer-tgas",
        "100",
        "--launch-deposit-millinear",
        "500",
        "--delete-buyers",
    ])
    .unwrap();
    assert_eq!(parsed.network.as_deref(), Some("testnet"));
    assert_eq!(parsed.ticker.as_deref(), Some("BAG"));
    assert_eq!(parsed.name.as_deref(), Some("Bag Holder"));
    assert_eq!(
        parsed.description.as_deref(),
        Some("A coin that holds its bags.")
    );
    assert_eq!(parsed.buy_millinear, Some(500));
    assert_eq!(parsed.buyer_extra_millinear, Some(200));
    assert_eq!(parsed.buyer_tgas, Some(100));
    assert_eq!(parsed.launch_deposit_millinear, Some(500));
    assert!(parsed.delete_buyers);
    assert!(parse(&[]).unwrap().network.is_none());
}

#[test]
fn every_unknown_form_is_refused_and_named() {
    for args in [
        vec!["--network=testnet"],
        vec!["--network"],
        vec!["--delete-buyer"],
        vec!["testnet"],
        vec!["--buy-millinear", "abc"],
        vec!["--buy-millinear", "--delete-buyers"],
        vec!["--ticker", "a", "--ticker", "b"],
        vec!["--name", "a", "--name", "b"],
        vec!["--description"],
        vec!["--name", "--ticker", "bag"],
        vec!["--delete-buyers", "--delete-buyers"],
    ] {
        let error = parse(&args)
            .err()
            .unwrap_or_else(|| panic!("{args:?} was accepted"));
        assert!(
            error
                .to_string()
                .contains(args[0].split('=').next().unwrap()),
            "{args:?}: {error}"
        );
    }
}

#[test]
fn the_relaunch_flags_parse() {
    let parsed = parse(&[
        "--buyers",
        "5",
        "--later-buy-millinear",
        "300",
        "--sandbox-block-ms",
        "600",
        "--sandbox-send-delay-ms",
        "3300",
    ])
    .unwrap();
    assert_eq!(parsed.buyers, Some(5));
    assert_eq!(parsed.later_buy_millinear, Some(300));
    assert_eq!(parsed.sandbox_block_ms, Some(600));
    assert_eq!(parsed.sandbox_send_delay_ms, Some(3300));
    for args in [
        vec!["--buyers"],
        vec!["--buyers", "five"],
        vec!["--buyers", "5", "--buyers", "6"],
        vec!["--later-buy-millinear", "--buyers", "5"],
        vec!["--sandbox-block-ms=600"],
    ] {
        let error = parse(&args)
            .err()
            .unwrap_or_else(|| panic!("{args:?} was accepted"));
        assert!(
            error
                .to_string()
                .contains(args[0].split('=').next().unwrap()),
            "{args:?}: {error}"
        );
    }
}

#[test]
fn the_sandbox_rehearsal_flags_are_refused_on_testnet() {
    use fair_chance_sandbox_tests::option_t::{check_sandbox_only_flags, DemoNetwork};
    let rehearsal = parse(&["--sandbox-block-ms", "600"]).unwrap();
    let error = check_sandbox_only_flags(&rehearsal, DemoNetwork::Testnet)
        .unwrap_err()
        .to_string();
    assert!(error.contains("--sandbox-block-ms"), "{error}");
    let delay = parse(&["--sandbox-send-delay-ms", "3300"]).unwrap();
    assert!(check_sandbox_only_flags(&delay, DemoNetwork::Testnet).is_err());
    assert!(check_sandbox_only_flags(&rehearsal, DemoNetwork::Sandbox).is_ok());
    assert!(check_sandbox_only_flags(&parse(&[]).unwrap(), DemoNetwork::Testnet).is_ok());
}
