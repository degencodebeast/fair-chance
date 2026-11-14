//! The BAG coin on testnet belongs to the sells-only step. Every other testnet path refuses the
//! ticker BAG in any case, before any read or account step; a run with --ticker BAG
//! --delete-buyers would otherwise delete bag-buyer-01 to -20 (found by the adversarial review).

use fair_chance_sandbox_tests::option_t::{check_ticker, DemoNetwork};

#[test]
fn bag_on_testnet_is_refused_for_every_path_but_sells_only() {
    for ticker in ["BAG", "bag", "Bag", "bAg"] {
        let error = check_ticker(ticker, DemoNetwork::Testnet, false)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("BAG")
                && error.contains("--sells-only")
                && error.contains("nothing was run"),
            "{ticker}: {error}"
        );
        assert!(
            check_ticker(ticker, DemoNetwork::Testnet, true).is_ok(),
            "{ticker}"
        );
    }
}

#[test]
fn other_tickers_and_the_sandbox_pass() {
    assert!(check_ticker("CABAL", DemoNetwork::Testnet, false).is_ok());
    assert!(check_ticker("bags", DemoNetwork::Testnet, false).is_ok());
    assert!(check_ticker("BAG", DemoNetwork::Sandbox, false).is_ok());
}
