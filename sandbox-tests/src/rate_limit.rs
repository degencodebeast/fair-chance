//! Reads on a public RPC back off on a 429. near-api retries a few times on its own and then gives
//! up with the response in the error text (`status: 429` and its `retry-after` header), so this
//! module reads that text: a 429 waits (the Retry-After when it is longer) and reads again, up to
//! a total wait limit. A 403 or any other error is not waited on: the caller stops and reports.
//! Only reads go through here; a transaction is never sent twice.
//!
//! The waits are one budget for the whole run ([`run_budget`]): every read shares it, so the run
//! obeys at most two 429 waits and 15 minutes of waiting in all. No error text carries an RPC URL
//! ([`without_urls`]): a URL could hold a key.

use std::future::Future;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{bail, Result};

/// The total time the run may spend waiting on 429s before it stops (the builder's rule).
pub const READ_WAIT_LIMIT: Duration = Duration::from_secs(15 * 60);
/// The most waits the run may make on 429s before it stops (the builder's rule: obey
/// Retry-After at most twice).
pub const READ_MAX_WAITS: u32 = 2;

/// The waits of one read: `start`, then double up to `max`, a longer Retry-After wins, no wait
/// that would pass `total` in all, and no more than `max_waits` waits.
#[derive(Debug, Clone, Copy)]
pub struct Waits {
    next: Duration,
    max: Duration,
    total: Duration,
    waited: Duration,
    max_waits: u32,
    waits: u32,
}

impl Waits {
    pub fn new(start: Duration, max: Duration, total: Duration) -> Self {
        Self {
            next: start,
            max,
            total,
            waited: Duration::ZERO,
            max_waits: READ_MAX_WAITS,
            waits: 0,
        }
    }

    /// The default for testnet reads: 2 s, doubling to 60 s, at most [`READ_MAX_WAITS`] waits and
    /// [`READ_WAIT_LIMIT`] in all.
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
        if self.waits >= self.max_waits || self.waited + wait > self.total {
            return None;
        }
        self.waits += 1;
        self.waited += wait;
        self.next = (self.next * 2).min(self.max);
        Some(wait)
    }
}

/// One wait budget, shared by every read that is given it.
#[derive(Debug)]
pub struct Budget(Mutex<Waits>);

impl Budget {
    pub const fn new(waits: Waits) -> Self {
        Self(Mutex::new(waits))
    }

    pub(crate) fn next(&self, retry_after: Option<Duration>) -> Option<Duration> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .next(retry_after)
    }
}

/// The budget of this run (one process is one run): [`Waits::for_reads`], shared by every read.
pub fn run_budget() -> &'static Budget {
    static BUDGET: OnceLock<Budget> = OnceLock::new();
    BUDGET.get_or_init(|| Budget::new(Waits::for_reads()))
}

/// The text with every http:// or https:// URL replaced by `<rpc url>`. A URL ends at a quote,
/// a space, a closing bracket or a comma.
pub fn without_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = ["http://", "https://"]
        .iter()
        .filter_map(|scheme| rest.find(scheme))
        .min()
    {
        out.push_str(&rest[..at]);
        out.push_str("<rpc url>");
        let tail = &rest[at..];
        let end = tail
            .find(|c: char| c.is_whitespace() || "\"'`)]},;<>".contains(c))
            .unwrap_or(tail.len());
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
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

/// Runs `read` until it answers, waiting on each 429 as `budget` allows. A 403, any other error,
/// or a 429 past the budget stops it with `what` in the error and no URL in it.
pub async fn with_rate_limit_backoff<T, E, F, Fut>(
    what: &str,
    budget: &Budget,
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
            bail!(
                "unknown result of {what}: {}; stopped, no retry",
                without_urls(&error)
            );
        };
        let Some(wait) = budget.next(retry_after) else {
            bail!(
                "{what} got HTTP 429 past the run's wait limit ({READ_MAX_WAITS} waits, {} s in all); stopped: {}",
                READ_WAIT_LIMIT.as_secs(),
                without_urls(&error)
            );
        };
        println!(
            "rate limit: {what} got HTTP 429; waiting {} s",
            wait.as_secs_f64()
        );
        tokio::time::sleep(wait).await;
    }
}
