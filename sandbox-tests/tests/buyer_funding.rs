//! Each runner account signs its own transactions, and the node refuses a transaction whose
//! deposits plus prepaid gas at the current gas price are more than the signer holds
//! (NotEnoughBalance). The sandbox answered that for 0.7 NEAR buyers at a cost of
//! 0.806719159630831 NEAR per buy. The funding check counts, as if every prepaid gas unit burns:
//! an early buyer's buy (with its storage deposit), early sell and sell after the window; the
//! later buyer's buy and sell; 5 Tgas of send fees per transaction, a 25 percent rise of the gas
//! price and 0.01 NEAR for the account itself. All buyers get the same extra, and the later
//! buyer sends one transaction fewer, so the early buyer's shortfall sets the extra.

use fair_chance_sandbox_tests::buyer_funding::{
    buyer_extra_for, buyer_need, check_buyer_funding, later_buyer_need,
};
use near_api::types::{NearGas, NearToken};

const SANDBOX_GAS_PRICE: u128 = 1_000_000_000;
const TESTNET_GAS_PRICE: u128 = 100_000_000;
const BUYER_GAS: NearGas = NearGas::from_tgas(300);

fn millinear(n: u128) -> NearToken {
    NearToken::from_millinear(n)
}

#[test]
fn an_early_buyer_needs_its_three_transactions_at_the_raised_gas_price() {
    // 0.5 + 0.005 + 665 Tgas x 1.25e9 + 0.01 NEAR + 2 yocto of sell deposits.
    assert_eq!(
        buyer_need(millinear(500), BUYER_GAS, SANDBOX_GAS_PRICE).as_yoctonear(),
        1_346_250_000_000_000_000_000_002
    );
    // 0.5 + 0.005 + 665 Tgas x 1.25e8 + 0.01 NEAR + 2 yocto.
    assert_eq!(
        buyer_need(millinear(500), BUYER_GAS, TESTNET_GAS_PRICE).as_yoctonear(),
        598_125_000_000_000_000_000_002
    );
}

#[test]
fn the_need_covers_the_cost_the_sandbox_refused() {
    assert!(
        buyer_need(millinear(500), BUYER_GAS, SANDBOX_GAS_PRICE).as_yoctonear()
            > 806_719_159_630_831_000_000_000
    );
}

#[test]
fn the_later_buyer_needs_its_buy_and_its_sell() {
    // 0.3 + 0.005 + 610 Tgas x 1.25e9 + 0.01 NEAR + 1 yocto.
    assert_eq!(
        later_buyer_need(millinear(300), BUYER_GAS, SANDBOX_GAS_PRICE).as_yoctonear(),
        1_077_500_000_000_000_000_000_001
    );
}

#[test]
fn the_extra_is_the_larger_shortfall_rounded_up_to_a_millinear() {
    assert_eq!(
        buyer_extra_for(millinear(500), millinear(300), BUYER_GAS, SANDBOX_GAS_PRICE),
        millinear(847)
    );
    assert_eq!(
        buyer_extra_for(millinear(500), millinear(300), BUYER_GAS, TESTNET_GAS_PRICE),
        millinear(99)
    );
    // No later buyer: only the early buyer counts.
    assert_eq!(
        buyer_extra_for(millinear(500), millinear(0), BUYER_GAS, TESTNET_GAS_PRICE),
        millinear(99)
    );
}

#[test]
fn the_testnet_plan_passes_at_the_testnet_gas_price() {
    check_buyer_funding(
        millinear(500),
        millinear(300),
        millinear(200),
        BUYER_GAS,
        TESTNET_GAS_PRICE,
    )
    .unwrap();
}

#[test]
fn the_same_plan_stops_at_the_sandbox_gas_price_with_both_amounts() {
    let error = check_buyer_funding(
        millinear(500),
        millinear(300),
        millinear(200),
        BUYER_GAS,
        SANDBOX_GAS_PRICE,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("1.346250000000000000000002 NEAR"), "{error}");
    assert!(error.contains("0.7 NEAR"), "{error}");
    assert!(error.contains("1000000000 yoctoNEAR per gas"), "{error}");
    assert!(error.contains("--buyer-extra-millinear 847"), "{error}");
    assert!(error.contains("nothing was signed"), "{error}");
}
