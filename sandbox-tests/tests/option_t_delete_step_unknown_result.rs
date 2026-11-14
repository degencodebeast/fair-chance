//! The delete step counts an account as absent only when the RPC answers that it does not exist.
//! Any other error is an unknown result: the step stops and names the account. Nothing signs
//! here: the RPC address does not answer.

use fair_chance_sandbox_tests::option_t::{delete_buyers, OptionTSetup};
use fair_chance_sandbox_tests::Account;
use near_api::types::{NearGas, NearToken};
use near_api::{NetworkConfig, Signer};

#[tokio::test]
async fn an_unreachable_rpc_is_an_unknown_result_not_a_missing_account() {
    // A throwaway in-memory key; the RPC does not answer, so nothing is signed or sent.
    let secret = near_api::signer::generate_secret_key().unwrap();
    let setup = OptionTSetup {
        network: NetworkConfig::from_rpc_url("unreachable", "http://127.0.0.1:1".parse().unwrap()),
        send_network: NetworkConfig::from_rpc_url(
            "unreachable",
            "http://127.0.0.1:1".parse().unwrap(),
        ),
        funder: Account {
            id: "funder.sandbox".parse().unwrap(),
            signer: Signer::from_secret_key(secret).unwrap(),
        },
        factory: "factory.sandbox".parse().unwrap(),
        ticker: "optiont".into(),
        name: "Option T".into(),
        description: "Option T demo coin.".into(),
        buyers: 20,
        buy_near: NearToken::from_near(1),
        buyer_extra: NearToken::from_millinear(500),
        buyer_gas: NearGas::from_tgas(300),
        launch_deposit: NearToken::from_near(5),
        limits: None,
        later_buy: NearToken::from_yoctonear(0),
        send_delay: std::time::Duration::ZERO,
    };
    let error = delete_buyers(&setup).await.unwrap_err().to_string();
    assert!(
        error.contains("optiont-buyer-01.funder.sandbox") && error.contains("unknown"),
        "{error}"
    );
}
