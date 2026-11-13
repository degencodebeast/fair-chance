//! The Option T delete step removes only the buyer sub-accounts, never the creator, the coin or the
//! factory, and on testnet it runs only with its own yes flag from the builder.

use fair_chance_sandbox_tests::option_t::{
    buyer_account_ids, choose_buyer_deletion, creator_account_id, DemoNetwork,
    DELETE_BUYERS_YES_FLAG, TESTNET_YES_FLAG,
};
use near_api::types::AccountId;

fn id(text: &str) -> AccountId {
    text.parse().unwrap()
}

#[test]
fn the_buyer_accounts_are_the_numbered_sub_accounts_of_the_funder() {
    let ids = buyer_account_ids("bag", &id("fairchance.testnet"), 20).unwrap();
    assert_eq!(ids.len(), 20);
    assert_eq!(ids[0], id("bag-buyer-01.fairchance.testnet"));
    assert_eq!(ids[19], id("bag-buyer-20.fairchance.testnet"));
}

#[test]
fn the_buyer_accounts_never_hold_the_creator_the_coin_or_the_factory() {
    let funder = id("fairchance.testnet");
    let factory = id("factory.fairchance.testnet");
    let creator = creator_account_id("bag", &funder).unwrap();
    let coin = id("bag.factory.fairchance.testnet");
    let ids = buyer_account_ids("bag", &funder, 20).unwrap();
    for protected in [&creator, &coin, &factory, &funder] {
        assert!(!ids.contains(protected), "{protected}");
    }
}

#[test]
fn deleting_testnet_buyers_needs_its_own_yes_flag() {
    let error = choose_buyer_deletion(DemoNetwork::Testnet, None).unwrap_err();
    assert!(
        error.to_string().contains(DELETE_BUYERS_YES_FLAG),
        "{error}"
    );
    for value in ["", "1", "YES", "yes "] {
        assert!(choose_buyer_deletion(DemoNetwork::Testnet, Some(value)).is_err());
    }
    assert!(choose_buyer_deletion(DemoNetwork::Testnet, Some("yes")).is_ok());
    assert_ne!(DELETE_BUYERS_YES_FLAG, TESTNET_YES_FLAG);
}

#[test]
fn deleting_sandbox_buyers_needs_no_flag() {
    assert!(choose_buyer_deletion(DemoNetwork::Sandbox, None).is_ok());
}
