//! The rehearsal joins the runner and the stand-in. A runner that stops (for example before it
//! starts to wait, so the stand-in never gets its go) must be reported with the runner's own
//! reason, not the stand-in's "stopped before it waited" text. No sandbox: the stand-in's network
//! is a closed local port, and it never gets far enough to use it.

use std::time::Duration;

use fair_chance_sandbox_tests::app_coin::{join_app_coin_run, stand_in_launch};
use fair_chance_sandbox_tests::Account;
use near_api::types::CryptoHash;

fn launcher() -> Account {
    let signer =
        near_api::Signer::from_secret_key(near_api::signer::generate_secret_key().unwrap())
            .unwrap();
    Account {
        id: "alice.sandbox".parse().unwrap(),
        signer,
    }
}

#[tokio::test]
async fn a_runner_that_stops_before_the_wait_is_reported_with_its_own_reason() {
    let (ready, go) = tokio::sync::oneshot::channel::<()>();
    let network =
        near_api::NetworkConfig::from_rpc_url("sandbox", "http://127.0.0.1:9".parse().unwrap());
    let stand_in = tokio::spawn(stand_in_launch(
        network,
        "factory.sandbox".parse().unwrap(),
        launcher(),
        "cabal3".to_string(),
        Duration::ZERO,
        go,
    ));
    // run_app_coin returned before ready() ran, so its closure (and the sender) is dropped.
    drop(ready);
    let runner: anyhow::Result<u32> = Err(anyhow::anyhow!(
        "REAL REASON: the coin exists already; nothing was signed"
    ));
    let text = format!(
        "{:#}",
        join_app_coin_run(runner, stand_in).await.unwrap_err()
    );
    assert!(text.contains("REAL REASON"), "{text}");
    assert!(!text.contains("stopped before it waited"), "{text}");
}

#[tokio::test]
async fn a_runner_and_a_stand_in_that_both_end_well_give_both_answers() {
    let hash = CryptoHash::default();
    let stand_in = tokio::spawn(async move { Ok(hash) });
    let (run, launched) = join_app_coin_run(Ok(7u32), stand_in).await.unwrap();
    assert_eq!((run, launched), (7, hash));
}

#[tokio::test]
async fn a_stand_in_that_fails_after_a_good_run_is_reported() {
    let stand_in = tokio::spawn(async { Err(anyhow::anyhow!("STAND-IN FAILED")) });
    let text = format!(
        "{:#}",
        join_app_coin_run(Ok(7u32), stand_in).await.unwrap_err()
    );
    assert!(text.contains("STAND-IN FAILED"), "{text}");
}
