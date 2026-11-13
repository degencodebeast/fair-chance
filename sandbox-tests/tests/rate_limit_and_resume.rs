//! After the first testnet Option T run stopped on a public RPC 429: every read backs off on a
//! 429 and honours Retry-After within a total wait limit, and still stops on a 403 or an unknown
//! answer; a rerun reuses the creator and buyer accounts that exist with the funder's key and the
//! planned NEAR, and never funds an existing account twice.

use std::time::Duration;

use anyhow::anyhow;
use fair_chance_sandbox_tests::option_t::{decide_account, AccountPlan, ExistingAccount};
use fair_chance_sandbox_tests::rate_limit::{rate_limit_wait, with_rate_limit_backoff, Waits};
use near_api::types::NearToken;

/// The error text the first testnet run printed (headers trimmed).
const SEEN_429: &str = r#"Query error: RetriesExhausted(TransportError(Unexpected Response: Response { url: "https://rpc.testnet.near.org/", status: 429, headers: {"date": "Fri, 13 Nov 2026 19:13:47 GMT", "content-type": "application/json", "retry-after": "561", "server": "cloudflare"} }))"#;

#[test]
fn the_seen_429_is_a_rate_limit_with_its_retry_after() {
    assert_eq!(
        rate_limit_wait(SEEN_429),
        Some(Some(Duration::from_secs(561)))
    );
    let no_header = r#"TransportError(Unexpected Response: Response { status: 429, headers: {} })"#;
    assert_eq!(rate_limit_wait(no_header), Some(None));
}

#[test]
fn a_403_an_unknown_account_or_a_timeout_is_not_a_rate_limit() {
    for text in [
        r#"Response { status: 403, headers: {"retry-after": "5"} }"#,
        "UnknownAccount { requested_account_id: x }",
        "TransportError(timeout)",
        "status: 4290",
    ] {
        assert_eq!(rate_limit_wait(text), None, "{text}");
    }
}

#[test]
fn the_waits_double_take_a_longer_retry_after_and_end_at_the_total_limit() {
    let mut w = Waits::new(
        Duration::from_secs(2),
        Duration::from_secs(60),
        Duration::from_secs(15 * 60),
    );
    assert_eq!(w.next(None), Some(Duration::from_secs(2)));
    assert_eq!(w.next(None), Some(Duration::from_secs(4)));
    assert_eq!(
        w.next(Some(Duration::from_secs(561))),
        Some(Duration::from_secs(561))
    );
    // 567 s waited; 561 more would pass 900 s.
    assert_eq!(w.next(Some(Duration::from_secs(561))), None);
}

#[tokio::test]
async fn a_read_waits_on_429s_and_then_gives_its_answer() {
    let mut calls = 0;
    let got = with_rate_limit_backoff(
        "a read",
        Waits::new(
            Duration::from_millis(1),
            Duration::from_millis(4),
            Duration::from_secs(5),
        ),
        || {
            calls += 1;
            let n = calls;
            async move {
                if n < 3 {
                    Err(anyhow!(
                        r#"Response {{ status: 429, headers: {{"retry-after": "0"}} }}"#
                    ))
                } else {
                    Ok(7)
                }
            }
        },
    )
    .await
    .unwrap();
    assert_eq!((got, calls), (7, 3));
}

#[tokio::test]
async fn a_403_or_an_unknown_error_stops_the_read_at_once() {
    for text in [
        r#"Response { status: 403, headers: {} }"#,
        "TransportError(timeout)",
    ] {
        let mut calls = 0;
        let error = with_rate_limit_backoff(
            "a read",
            Waits::new(
                Duration::from_millis(1),
                Duration::from_millis(4),
                Duration::from_secs(5),
            ),
            || {
                calls += 1;
                async move { Err::<(), _>(anyhow!("{text}")) }
            },
        )
        .await
        .unwrap_err()
        .to_string();
        assert_eq!(calls, 1, "{text}");
        assert!(error.contains("a read"), "{error}");
    }
}

#[tokio::test]
async fn a_read_that_stays_rate_limited_past_the_total_limit_stops() {
    let mut calls = 0;
    let error = with_rate_limit_backoff(
        "the balance read",
        Waits::new(
            Duration::from_millis(5),
            Duration::from_millis(5),
            Duration::from_millis(12),
        ),
        || {
            calls += 1;
            async move { Err::<(), _>(anyhow!("Response {{ status: 429, headers: {{}} }}")) }
        },
    )
    .await
    .unwrap_err()
    .to_string();
    assert_eq!(calls, 3, "two waits of 5 ms fit in 12 ms, a third does not");
    assert!(
        error.contains("429") && error.contains("the balance read"),
        "{error}"
    );
}

fn planned() -> NearToken {
    NearToken::from_millinear(1500)
}

#[test]
fn a_missing_account_is_created() {
    assert_eq!(
        decide_account(None, planned()).unwrap(),
        AccountPlan::Create
    );
}

#[test]
fn an_account_with_the_funder_key_and_the_planned_near_is_reused() {
    let existing = ExistingAccount {
        balance: planned(),
        funder_key_full_access: true,
    };
    assert_eq!(
        decide_account(Some(existing), planned()).unwrap(),
        AccountPlan::Reuse
    );
}

#[test]
fn an_account_without_the_funder_key_or_with_less_near_stops_the_run() {
    let no_key = ExistingAccount {
        balance: planned(),
        funder_key_full_access: false,
    };
    let error = decide_account(Some(no_key), planned())
        .unwrap_err()
        .to_string();
    assert!(error.contains("full-access"), "{error}");
    let short = ExistingAccount {
        balance: NearToken::from_millinear(1499),
        funder_key_full_access: true,
    };
    let error = decide_account(Some(short), planned())
        .unwrap_err()
        .to_string();
    assert!(error.contains("1.499"), "{error}");
}
