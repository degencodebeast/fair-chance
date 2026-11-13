//! Opus review round 2, findings 2 and 3: an RPC URL that could hold a key in its path is refused,
//! no RPC URL appears in any error text, and every read of a run shares one 429 budget (two waits,
//! 15 minutes in all), not one budget per read.

mod common;

use std::time::Duration;

use anyhow::anyhow;
use fair_chance_sandbox_tests::account_view;
use fair_chance_sandbox_tests::rate_limit::{with_rate_limit_backoff, Budget, Waits};
use fair_chance_sandbox_tests::rpcs::testnet_rpcs;

const FAKE_KEY: &str = "FAKEKEY0123456789";

#[test]
fn a_path_that_could_hold_a_key_is_refused_and_not_echoed() {
    for (read, public) in [
        (
            "https://rpc.testnet.invalid/v3/FAKEKEY0123456789",
            "https://rpc.testnet.near.org",
        ),
        (
            "https://rpc.testnet.fastnear.com",
            "https://rpc.testnet.near.org/FAKEKEY0123456789",
        ),
    ] {
        let error = format!("{:#}", testnet_rpcs(read, public).unwrap_err());
        assert!(!error.contains(FAKE_KEY), "{error}");
    }
    testnet_rpcs(
        "https://rpc.testnet.fastnear.com/",
        "https://rpc.testnet.near.org",
    )
    .expect("a bare host with / is fine");
}

#[tokio::test]
async fn a_read_error_never_prints_the_rpc_url() {
    // Nothing listens on port 1, so the read fails at once with a transport error.
    let network = common::network(&format!("http://127.0.0.1:1/{FAKE_KEY}"));
    let error = account_view(&network, &"fairchance.testnet".parse().unwrap())
        .await
        .unwrap_err();
    for text in [format!("{error:#}"), format!("{error:?}")] {
        assert!(!text.contains(FAKE_KEY), "{text}");
        assert!(!text.contains("127.0.0.1"), "{text}");
    }
}

#[tokio::test]
async fn a_429_stop_never_prints_the_rpc_url() {
    let seen = r#"Query error: RetriesExhausted(TransportError(Unexpected Response: Response { url: "https://rpc.testnet.near.org/FAKEKEY0123456789", status: 429, headers: {"retry-after": "561"} }))"#;
    let budget = Budget::new(Waits::new(
        Duration::from_millis(1),
        Duration::from_millis(1),
        Duration::from_secs(1),
    ));
    let error = with_rate_limit_backoff("a read", &budget, || async {
        Err::<(), _>(anyhow!("{seen}"))
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("429"), "{error}");
    assert!(!error.contains(FAKE_KEY), "{error}");
    assert!(!error.contains("rpc.testnet.near.org"), "{error}");
}

#[tokio::test]
async fn two_reads_share_one_budget_of_two_waits() {
    let budget = Budget::new(Waits::new(
        Duration::from_millis(1),
        Duration::from_millis(4),
        Duration::from_secs(5),
    ));
    let limited = || async {
        Err::<u8, _>(anyhow::Error::msg(
            r#"Response { status: 429, headers: {} }"#,
        ))
    };
    let mut first = 0;
    let got = with_rate_limit_backoff("the first read", &budget, || {
        first += 1;
        let n = first;
        async move {
            if n < 3 {
                limited().await
            } else {
                Ok(1)
            }
        }
    })
    .await
    .unwrap();
    assert_eq!((got, first), (1, 3));
    let mut second = 0;
    with_rate_limit_backoff("the second read", &budget, || {
        second += 1;
        limited()
    })
    .await
    .unwrap_err();
    assert_eq!(
        second, 1,
        "the second read waited, but the run had used its two waits"
    );
}

#[tokio::test]
async fn the_library_reads_share_the_run_budget() {
    // Every answer is 429 with Retry-After 0. The run budget waits 2 s, then 4 s, then stops; a
    // later read of the same run gets no wait at all.
    let rpc = common::start(common::Mode::Always429).await;
    let network = common::network(&rpc.url);
    let id = "fairchance.testnet".parse().unwrap();
    account_view(&network, &id).await.unwrap_err();
    assert_eq!(
        rpc.methods().len(),
        3,
        "the first read: one try and two waits"
    );
    account_view(&network, &id).await.unwrap_err();
    assert_eq!(
        rpc.methods().len(),
        4,
        "the second read waited after the run used its two waits"
    );
}
