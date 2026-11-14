//! One rule for every read error, in this order: any 4xx but 429 stops at once (a 403 first,
//! wherever it is in the text); then a 429 waits; then a 5xx waits; anything else stops. A 429 and
//! a 5xx take the Retry-After from the same header, and every wait stays inside the run's budget.

use std::cell::Cell;
use std::time::Duration;

use anyhow::anyhow;
use fair_chance_sandbox_tests::rate_limit::{
    rate_limit_wait, read_fault, with_rate_limit_backoff, Budget, ReadFault, Waits,
};

#[test]
fn a_4xx_other_than_429_stops_even_with_a_later_429_or_5xx_in_the_text() {
    for text in [
        r#"Response { status: 403, headers: {} } then Response { status: 429, headers: {"retry-after": "1"} }"#,
        r#"Response { status: 403, headers: {} } then Response { status: 503, headers: {} }"#,
        r#"Response { status: 503, headers: {} } then Response { status: 403, headers: {} }"#,
        r#"Response { status: 404, headers: {} }"#,
        r#"Response { status: 400, headers: {} }"#,
    ] {
        assert_eq!(read_fault(text), ReadFault::Stop, "{text}");
    }
    let mixed = r#"status: 403 Forbidden; status: 429 Too Many Requests"#;
    assert_eq!(rate_limit_wait(mixed), None);
}

#[test]
fn a_429_or_a_5xx_waits_with_the_retry_after_of_its_header() {
    assert_eq!(
        read_fault(r#"Response { status: 429, headers: {"retry-after": "7"} }"#),
        ReadFault::Busy(Some(Duration::from_secs(7)))
    );
    assert_eq!(
        read_fault(r#"Response { status: 503, headers: {"retry-after": "9"} }"#),
        ReadFault::Busy(Some(Duration::from_secs(9)))
    );
    assert_eq!(
        read_fault("HTTP error: status: 502 Bad Gateway"),
        ReadFault::Busy(None)
    );
}

#[test]
fn a_code_that_only_starts_like_a_known_one_or_no_code_stops() {
    for text in [
        "status: 4290",
        "status: 5000",
        "status: 42",
        "TransportError(timeout)",
        "UnknownAccount",
    ] {
        assert_eq!(read_fault(text), ReadFault::Stop, "{text}");
    }
}

fn small_budget(total: Duration) -> Budget {
    Budget::new(Waits::new(
        Duration::from_millis(1),
        Duration::from_millis(4),
        total,
    ))
}

#[tokio::test]
async fn a_shared_read_now_waits_on_a_5xx_and_then_gives_its_answer() {
    let calls = Cell::new(0);
    let budget = small_budget(Duration::from_secs(1));
    let got = with_rate_limit_backoff("a read", &budget, || {
        calls.set(calls.get() + 1);
        let first = calls.get() == 1;
        async move {
            if first {
                Err(anyhow!("Response {{ status: 503, headers: {{}} }}"))
            } else {
                Ok(5)
            }
        }
    })
    .await
    .unwrap();
    assert_eq!((got, calls.get()), (5, 2));
}

#[tokio::test]
async fn a_shared_read_stops_at_once_on_a_403_with_a_later_429() {
    let calls = Cell::new(0);
    let budget = small_budget(Duration::from_secs(1));
    let error = with_rate_limit_backoff("a read", &budget, || {
        calls.set(calls.get() + 1);
        async {
            Err::<(), _>(anyhow!(
                r#"Response {{ status: 403, headers: {{}} }} Response {{ status: 429, headers: {{}} }}"#
            ))
        }
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("no retry"), "{error}");
    assert_eq!(calls.get(), 1);
}

#[tokio::test]
async fn a_5xx_retry_after_past_the_budget_stops_with_no_sleep() {
    let budget = small_budget(Duration::from_secs(1));
    let started = std::time::Instant::now();
    let error = with_rate_limit_backoff("a read", &budget, || async {
        Err::<(), _>(anyhow!(
            r#"Response {{ status: 503, headers: {{"retry-after": "600"}} }}"#
        ))
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("wait limit"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(1));
}
