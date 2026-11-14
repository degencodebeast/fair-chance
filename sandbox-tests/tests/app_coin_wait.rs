//! The wait for a coin the app launches: it reads again every interval while the coin is missing,
//! waits on a 429 or a 5xx with the run's one wait budget (at most two waits), stops at once on a
//! 403 or any answer it does not know, and stops when the coin does not appear in time. No error
//! text carries an RPC URL.

use std::cell::Cell;
use std::time::Duration;

use fair_chance_sandbox_tests::app_coin::{wait_for_app_coin, Probe};
use fair_chance_sandbox_tests::rate_limit::{Budget, Waits};

fn budget() -> Budget {
    Budget::new(Waits::new(
        Duration::from_millis(1),
        Duration::from_millis(4),
        Duration::from_secs(1),
    ))
}

const TICK: Duration = Duration::from_millis(1);
const LONG: Duration = Duration::from_secs(5);

/// Answers from a script, in order; the last one repeats.
async fn run(
    script: &[Result<Option<u32>, &str>],
    deadline: Duration,
) -> (anyhow::Result<u32>, usize) {
    let calls = Cell::new(0usize);
    let budget = budget();
    let result = wait_for_app_coin(
        || {
            let i = calls.get();
            calls.set(i + 1);
            let answer = script[i.min(script.len() - 1)];
            async move {
                match answer {
                    Ok(None) => Ok(Probe::Missing),
                    Ok(Some(v)) => Ok(Probe::Found(v)),
                    Err(text) => Err(text.to_string()),
                }
            }
        },
        TICK,
        &budget,
        deadline,
    )
    .await;
    (result, calls.get())
}

#[tokio::test]
async fn it_reads_again_while_the_coin_is_missing_and_returns_what_it_found() {
    let (found, calls) = run(&[Ok(None), Ok(None), Ok(Some(7))], LONG).await;
    assert_eq!(found.unwrap(), 7);
    assert_eq!(calls, 3);
}

#[tokio::test]
async fn a_429_or_a_5xx_waits_and_reads_again() {
    for busy in [
        "HTTP error: status: 429 Too Many Requests",
        "HTTP error: status: 503 Service Unavailable",
        "HTTP error: status: 500 Internal Server Error",
    ] {
        let (found, calls) = run(&[Err(busy), Ok(Some(1))], LONG).await;
        assert_eq!(found.unwrap(), 1, "{busy}");
        assert_eq!(calls, 2);
    }
}

#[tokio::test]
async fn the_third_busy_answer_stops_the_wait() {
    let (stopped, calls) = run(
        &[
            Err("status: 429 Too Many Requests"),
            Err("status: 502 Bad Gateway"),
            Err("status: 429 Too Many Requests"),
        ],
        LONG,
    )
    .await;
    let text = stopped.unwrap_err().to_string();
    assert!(text.contains("wait limit"), "{text}");
    assert_eq!(calls, 3);
}

#[tokio::test]
async fn a_403_or_an_unknown_answer_stops_at_once() {
    for answer in [
        "HTTP error: status: 403 Forbidden",
        "something no rule knows",
        "status: 4290 is not a 429",
    ] {
        let (stopped, calls) = run(&[Err(answer), Ok(Some(1))], LONG).await;
        let text = stopped.unwrap_err().to_string();
        assert!(text.contains("no retry"), "{answer}: {text}");
        assert_eq!(calls, 1, "{answer}");
    }
}

#[tokio::test]
async fn a_coin_that_does_not_appear_in_time_stops_the_wait() {
    let (stopped, calls) = run(&[Ok(None)], Duration::from_millis(30)).await;
    let text = stopped.unwrap_err().to_string();
    assert!(text.contains("did not appear"), "{text}");
    assert!(calls >= 2);
}

#[tokio::test]
async fn no_rpc_url_is_in_the_stop_text() {
    let (stopped, _) = run(
        &[Err(
            "status: 403 Forbidden for url https://rpc.fake.example/KEY123",
        )],
        LONG,
    )
    .await;
    let text = stopped.unwrap_err().to_string();
    assert!(
        !text.contains("KEY123") && !text.contains("https://"),
        "{text}"
    );
}
