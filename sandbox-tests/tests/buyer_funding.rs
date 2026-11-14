//! Each runner account signs its own transactions, and the node refuses one whose deposits plus
//! its cost of gas are more than the signer holds (NotEnoughBalance). The node buys the prepaid
//! gas and the send fees at min_gas_purchase_price (1e9 yoctoNEAR per gas on the sandbox and on
//! testnet), not at the block gas price (1e8): the sandbox probe measured
//! cost = deposit + (prepaid gas + 1.719 Tgas) x 1e9 at 300, 150 and 30 Tgas, and testnet charged
//! a CABAL2 buy (100 Tgas, 0.505 NEAR attached) 0.606719159630831 NEAR.
//!
//! The check uses the higher of the node's gas price with 25 percent headroom and
//! min_gas_purchase_price, 5 Tgas of send fees per transaction and 0.01 NEAR for the account. It
//! counts the two moments an early buyer signs at: the burst (buy and early sell, both prepaid in
//! full) and the sell after the window (refunds back, 10 Tgas burnt per earlier transaction; a
//! CABAL2 buy burnt 5.04 Tgas). The later buyer the same, one transaction fewer.

use fair_chance_sandbox_tests::buyer_funding::{
    buyer_extra_for, buyer_need, check_buyer_funding, later_buyer_need, price_used,
};
use near_api::types::{NearGas, NearToken};

const PURCHASE_PRICE: u128 = 1_000_000_000;
const GAS_100: NearGas = NearGas::from_tgas(100);
const GAS_300: NearGas = NearGas::from_tgas(300);

fn millinear(n: u128) -> NearToken {
    NearToken::from_millinear(n)
}

#[test]
fn the_price_is_the_higher_of_the_raised_gas_price_and_the_purchase_price() {
    assert_eq!(price_used(100_000_000, PURCHASE_PRICE), 1_000_000_000);
    assert_eq!(price_used(1_000_000_000, PURCHASE_PRICE), 1_250_000_000);
    assert_eq!(price_used(2_000_000_000, PURCHASE_PRICE), 2_500_000_000);
    assert_eq!(price_used(100_000_000, 0), 125_000_000);
}

#[test]
fn an_early_buyer_needs_the_larger_of_its_two_moments() {
    // Burst: 0.5 + 0.005 + (100 + 50 + 2 x 5) Tgas x 1e9 + 0.01 + 1 yocto.
    assert_eq!(
        buyer_need(millinear(500), GAS_100, PURCHASE_PRICE).as_yoctonear(),
        675_000_000_000_000_000_000_001
    );
    // Burst: 0.5 + 0.005 + (300 + 50 + 2 x 5) Tgas x 1e9 + 0.01 + 1 yocto.
    assert_eq!(
        buyer_need(millinear(500), GAS_300, PURCHASE_PRICE).as_yoctonear(),
        875_000_000_000_000_000_000_001
    );
}

#[test]
fn the_need_covers_what_the_node_charges_at_the_burst() {
    // The probe's buy at 100 Tgas, plus an early sell of 50 Tgas with the same send fees.
    let node = 606_719_159_630_831_000_000_000u128 + (50_000 + 1_719) * 1_000_000_000_000_000_000;
    assert!(buyer_need(millinear(500), GAS_100, PURCHASE_PRICE).as_yoctonear() > node);
}

#[test]
fn the_later_buyer_needs_its_sell_after_its_buy_burnt() {
    // Sell moment: 0.3 + 0.005 + 10 Tgas burnt + (100 + 5) Tgas x 1e9 + 0.01 + 1 yocto.
    assert_eq!(
        later_buyer_need(millinear(300), GAS_100, PURCHASE_PRICE).as_yoctonear(),
        430_000_000_000_000_000_000_001
    );
}

#[test]
fn the_extra_is_the_larger_shortfall_rounded_up_to_a_millinear() {
    assert_eq!(
        buyer_extra_for(millinear(500), millinear(300), GAS_100, PURCHASE_PRICE),
        millinear(176)
    );
    assert_eq!(
        buyer_extra_for(millinear(500), millinear(300), GAS_300, PURCHASE_PRICE),
        millinear(376)
    );
    assert_eq!(
        buyer_extra_for(millinear(500), millinear(0), GAS_100, PURCHASE_PRICE),
        millinear(176)
    );
}

#[test]
fn the_testnet_plan_passes_with_100_tgas() {
    check_buyer_funding(
        millinear(500),
        millinear(300),
        millinear(200),
        GAS_100,
        PURCHASE_PRICE,
    )
    .unwrap();
}

#[test]
fn the_same_plan_with_300_tgas_stops_with_both_amounts() {
    let error = check_buyer_funding(
        millinear(500),
        millinear(300),
        millinear(200),
        GAS_300,
        PURCHASE_PRICE,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("0.875000000000000000000001 NEAR"), "{error}");
    assert!(error.contains("0.7 NEAR"), "{error}");
    assert!(error.contains("1000000000 yoctoNEAR per gas"), "{error}");
    assert!(error.contains("--buyer-extra-millinear 376"), "{error}");
    assert!(error.contains("nothing was signed"), "{error}");
}

#[test]
fn the_sandbox_rehearsal_uses_the_testnet_amounts_and_they_are_enough() {
    let script = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../scripts/rehearse-app-coin.sh"
    ))
    .unwrap();
    assert!(
        script.contains(
            "--buy-millinear 500 \\\n    --buyer-extra-millinear 200 --later-buy-millinear 300"
        ),
        "the script must pass the testnet amounts"
    );
    assert!(
        !script.contains("--buyer-tgas"),
        "the app coin default must apply"
    );
    assert!(
        buyer_extra_for(millinear(500), millinear(300), GAS_100, PURCHASE_PRICE) <= millinear(200)
    );
}
