//! Option T takes the network as a parameter. Sandbox is the default. Testnet signs real
//! transactions, so it runs only when the builder sets the yes flag for that run.

use fair_chance_sandbox_tests::option_t::{choose_network, DemoNetwork, TESTNET_YES_FLAG};

#[test]
fn no_network_argument_means_sandbox() {
    assert_eq!(choose_network(None, None).unwrap(), DemoNetwork::Sandbox);
    assert_eq!(
        choose_network(Some("sandbox"), None).unwrap(),
        DemoNetwork::Sandbox
    );
}

#[test]
fn testnet_without_the_yes_flag_is_refused_and_names_the_flag() {
    let error = choose_network(Some("testnet"), None).unwrap_err();
    assert!(error.to_string().contains(TESTNET_YES_FLAG), "{error}");
}

#[test]
fn testnet_with_any_flag_value_but_yes_is_refused() {
    for value in ["", "1", "true", "YES", "yes "] {
        assert!(
            choose_network(Some("testnet"), Some(value)).is_err(),
            "{value:?}"
        );
    }
}

#[test]
fn testnet_with_the_yes_flag_is_testnet() {
    assert_eq!(
        choose_network(Some("testnet"), Some("yes")).unwrap(),
        DemoNetwork::Testnet
    );
}

#[test]
fn any_other_network_is_refused() {
    for network in ["mainnet", "localnet", ""] {
        assert!(
            choose_network(Some(network), Some("yes")).is_err(),
            "{network}"
        );
    }
}
