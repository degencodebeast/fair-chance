//! The sandbox probe reads the node's own cost of one transaction from its NotEnoughBalance
//! answer, for three prepaid gas amounts, and fits cost = deposit + (prepaid gas + send fee gas)
//! x price. The expected values come from the node: 0.806719159630831 NEAR for 300 Tgas and
//! 0.505 NEAR attached (sandbox, twice), and testnet's CABAL2 buy, charged 0.606719159630831 NEAR
//! for 100 Tgas and 0.505 NEAR attached.

mod common;

const DEPOSIT: u128 = 505_000_000_000_000_000_000_000;
const TGAS: u128 = 1_000_000_000_000;

#[test]
fn the_cost_is_read_from_a_not_enough_balance_answer() {
    let answer = r#"{"jsonrpc":"2.0","error":{"name":"HANDLER_ERROR","cause":{"name":"INVALID_TRANSACTION","info":{}},"code":-32000,"message":"Server error","data":{"TxExecutionError":{"InvalidTxError":{"NotEnoughBalance":{"balance":"599000000000000000000000","cost":"806719159630831000000000","signer_id":"cabal3-buyer-01.bob.sandbox"}}}}},"id":"0"}"#;
    assert_eq!(
        common::not_enough_balance_cost(answer),
        Some(806_719_159_630_831_000_000_000)
    );
    assert_eq!(
        common::not_enough_balance_cost(r#"{"jsonrpc":"2.0","result":{},"id":"0"}"#),
        None
    );
}

#[test]
fn the_rule_fits_the_sandbox_and_the_testnet_charges() {
    // The sandbox's 300 Tgas answer and the testnet charge for 100 Tgas, same deposit.
    let points = [
        (300 * TGAS, 806_719_159_630_831_000_000_000),
        (100 * TGAS, 606_719_159_630_831_000_000_000),
    ];
    let rule = common::fit_cost_rule(DEPOSIT, &points).unwrap();
    assert_eq!(rule.price, 1_000_000_000);
    // 1.719159630831 Tgas of send fees, to within one gas unit of rounding.
    assert!(rule.fee_gas.abs_diff(1_719_159_630_831) <= 1, "{rule:?}");
    assert!(common::fits(
        DEPOSIT,
        &rule,
        30 * TGAS,
        536_719_159_630_831_000_000_000
    ));
    assert!(!common::fits(
        DEPOSIT,
        &rule,
        30 * TGAS,
        536_819_159_630_831_000_000_000
    ));
}

#[test]
fn a_rule_needs_two_different_gas_amounts() {
    assert!(common::fit_cost_rule(DEPOSIT, &[(300 * TGAS, 1), (300 * TGAS, 1)]).is_none());
}
