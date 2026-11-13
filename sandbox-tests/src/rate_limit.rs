//! Reads on a public RPC back off on a 429. near-api retries a few times on its own and then gives
//! up with the response in the error text (`status: 429` and its `retry-after` header), so this
//! module reads that text: a 429 waits (the Retry-After when it is longer) and reads again, up to
//! a total wait limit. A 403 or any other error is not waited on: the caller stops and reports.
//! Only reads go through here; a transaction is never sent twice.

use std::future::Future;
use std::time::Duration;

use anyhow::{bail, Result};

/// The total time one read may spend waiting on 429s before it stops.
pub const READ_WAIT_LIMIT: Duration = Duration::from_secs(15 * 60);

/// The waits of one read: `start`, then double up to `max`, a longer Retry-After wins, and no
/// wait that would pass `total` in all.
#[derive(Debug, Clone, Copy)]
pub struct Waits {
    next: Duration,
    max: Duration,
    total: Duration,
    waited: Duration,
}

impl Waits {
    pub fn new(start: Duration, max: Duration, total: Duration) -> Self {
        Self {
            next: start,
            max,
            total,
            waited: Duration::ZERO,
        }
    }

    /// The default for testnet reads: 2 s, doubling to 60 s, [`READ_WAIT_LIMIT`] in all.
    pub fn for_reads() -> Self {
        Self::new(
            Duration::from_secs(2),
            Duration::from_secs(60),
            READ_WAIT_LIMIT,
        )
    }

    /// The next wait, or `None` when it would pass the total limit.
    pub fn next(&mut self, retry_after: Option<Duration>) -> Option<Duration> {
        let wait = retry_after.map_or(self.next, |r| r.max(self.next));
        if self.waited + wait > self.total {
            return None;
        }
        self.waited += wait;
        self.next = (self.next * 2).min(self.max);
        Some(wait)
    }
}

/// `Some(retry_after)` when the error text is an HTTP 429, with its Retry-After in seconds when
/// the text has one; `None` for anything else.
pub fn rate_limit_wait(error: &str) -> Option<Option<Duration>> {
    let after = error.split("status: 429").nth(1)?;
    if after.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let retry_after = after
        .split(r#""retry-after": ""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .and_then(|secs| secs.parse::<u64>().ok())
        .map(Duration::from_secs);
    Some(retry_after)
}

/// Runs `read` until it answers, waiting on each 429 as [`Waits`] says. A 403, any other error,
/// or a 429 past the total limit stops it with `what` in the error.
pub async fn with_rate_limit_backoff<T, E, F, Fut>(
    what: &str,
    mut waits: Waits,
    mut read: F,
) -> Result<T>
where
    E: std::fmt::Debug,
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<T, E>>,
{
    loop {
        let error = match read().await {
            Ok(value) => return Ok(value),
            Err(e) => format!("{e:?}"),
        };
        let Some(retry_after) = rate_limit_wait(&error) else {
            bail!("unknown result of {what}: {error}; stopped, no retry");
        };
        let Some(wait) = waits.next(retry_after) else {
            bail!("{what} stayed rate limited (HTTP 429) past the wait limit; stopped: {error}");
        };
        println!(
            "rate limit: {what} got HTTP 429; waiting {} s",
            wait.as_secs_f64()
        );
        tokio::time::sleep(wait).await;
    }
}
