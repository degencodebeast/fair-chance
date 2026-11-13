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
