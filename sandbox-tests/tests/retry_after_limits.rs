//! A Retry-After the runner reads must never panic the run, and one it cannot read stops the run:
//! an answer the runner does not understand stops. A Retry-After counts only as whole seconds
//! (ASCII digits, within u64). A missing header keeps the default wait. A wait that would pass the
//! run's wait limit, even one too large to add, stops with "past the run's wait limit".

use std::time::Duration;

use fair_chance_sandbox_tests::rate_limit::{
    read_fault, with_rate_limit_backoff, Budget, ReadFault, Waits, READ_WAIT_LIMIT,
};

/// An error whose Debug is its text as it is, like near-api's Debug of a response.
struct Raw(String);
impl std::fmt::Debug for Raw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn answer(status: u16, retry_after: &str) -> String {
    format!(r#"Response {{ status: {status}, headers: {{"retry-after": "{retry_after}"}} }}"#)
}

#[test]
fn a_wait_too_large_to_add_is_past_the_limit_not_a_panic() {
    let mut waits = Waits::for_reads();
    assert_eq!(waits.next(None), Some(Duration::from_secs(2)));
    assert_eq!(waits.next(Some(Duration::from_secs(u64::MAX))), None);
}

#[tokio::test]
async fn a_huge_retry_after_after_one_wait_stops_with_the_wait_limit() {
    // One whole second waited first: 1 s + u64::MAX s does not fit in a Duration.
    let budget = Budget::new(Waits::new(
        Duration::from_secs(1),
        Duration::from_secs(1),
        READ_WAIT_LIMIT,
    ));
    let calls = std::cell::Cell::new(0);
    let outcome = with_rate_limit_backoff("a read", &budget, || {
        calls.set(calls.get() + 1);
        let first = calls.get() == 1;
        async move {
            if first {
                Err::<u8, _>(Raw("Response { status: 429, headers: {} }".to_string()))
            } else {
                Err(Raw(answer(429, "18446744073709551615")))
            }
        }
    })
    .await;
    let text = format!("{:#}", outcome.unwrap_err());
    assert!(text.contains("past the run's wait limit"), "{text}");
    assert_eq!(calls.get(), 2);
}

#[test]
fn a_retry_after_that_is_not_whole_seconds_stops() {
    for value in [
        "Sat, 14 Nov 2026 10:05:00 GMT",
        "-1",
        "1e3",
        "+5",
        " 5",
        "",
        "99999999999999999999",
    ] {
        for status in [429, 503] {
            assert_eq!(
                read_fault(&answer(status, value)),
                ReadFault::Stop,
                "{status} {value:?}"
            );
        }
    }
}

#[test]
fn whole_seconds_wait_and_a_missing_header_keeps_the_default() {
    for (value, secs) in [("0", 0), ("5", 5), ("18446744073709551615", u64::MAX)] {
        assert_eq!(
            read_fault(&answer(429, value)),
            ReadFault::Busy(Some(Duration::from_secs(secs))),
            "{value}"
        );
    }
    assert_eq!(
        read_fault("Response { status: 429, headers: {} }"),
        ReadFault::Busy(None)
    );
}

#[tokio::test]
async fn an_http_date_retry_after_stops_at_once_with_its_reason() {
    let calls = std::cell::Cell::new(0);
    let outcome = with_rate_limit_backoff("a read", &Budget::new(Waits::for_reads()), || {
        calls.set(calls.get() + 1);
        async { Err::<u8, _>(Raw(answer(429, "Sat, 14 Nov 2026 10:05:00 GMT"))) }
    })
    .await;
    let text = format!("{:#}", outcome.unwrap_err());
    assert_eq!(calls.get(), 1, "no second read");
    assert!(
        text.contains("a Retry-After the runner does not read"),
        "{text}"
    );
    assert!(text.contains("Sat, 14 Nov 2026 10:05:00 GMT"), "{text}");
    assert!(text.contains("stopped"), "{text}");
}
