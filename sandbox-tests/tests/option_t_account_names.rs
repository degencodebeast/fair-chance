//! The factory makes the coin account from the lowercase ticker and keeps the ticker as given for
//! the symbol, so Option T builds every account name from the lowercase ticker too. A ticker such
//! as BAG then gives the symbol BAG and the accounts bag.<factory>, bag-creator.<funder> and
//! bag-buyer-01.<funder>.

use fair_chance_sandbox_tests::option_t::{
    buyer_account_ids, coin_account_id, creator_account_id,
};
use near_api::types::AccountId;

fn id(s: &str) -> AccountId {
    s.parse().unwrap()
}

#[test]
fn an_uppercase_ticker_gives_lowercase_account_names() {
    let funder = id("fairchance.testnet");
    let factory = id("factory.fairchance.testnet");
    assert_eq!(
        coin_account_id("BAG", &factory).unwrap(),
        id("bag.factory.fairchance.testnet")
    );
    assert_eq!(
        creator_account_id("BAG", &funder).unwrap(),
        id("bag-creator.fairchance.testnet")
    );
    assert_eq!(
        buyer_account_ids("BAG", &funder, 2).unwrap(),
        vec![
            id("bag-buyer-01.fairchance.testnet"),
            id("bag-buyer-02.fairchance.testnet")
        ]
    );
}

#[test]
fn a_lowercase_ticker_gives_the_same_names_as_before() {
    let funder = id("bob.sandbox");
    assert_eq!(
        creator_account_id("optiont", &funder).unwrap(),
        id("optiont-creator.bob.sandbox")
    );
    assert_eq!(
        coin_account_id("optiont", &id("factory.sandbox")).unwrap(),
        id("optiont.factory.sandbox")
    );
}
