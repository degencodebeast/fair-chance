//! Reads on a public RPC back off on a 429 or a 5xx. near-api gives up with the response in the
//! error text (`status: 429` and its `retry-after` header), so this module reads that text with
//! one rule ([`read_fault`]): any 4xx but 429 (a 403 first) stops, wherever it is in the text; a
//! 429 or a 5xx waits (the Retry-After when it is longer) and reads again, up to a total wait
//! limit; anything else stops. The caller stops and reports. Only reads go through here; a
//! transaction is never sent twice.
//!
//! The waits are one budget for the whole run ([`run_budget`]): every read shares it, so the run
//! obeys at most two waits and 15 minutes of waiting in all; a Retry-After past that stops the run. No error text carries an RPC URL
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

    /// The next wait, or `None` when it would pass the total limit (a wait too large to add
    /// passes it too).
    pub fn next(&mut self, retry_after: Option<Duration>) -> Option<Duration> {
        let wait = retry_after.map_or(self.next, |r| r.max(self.next));
        let waited = self.waited.checked_add(wait)?;
        if self.waits >= self.max_waits || waited > self.total {
            return None;
        }
        self.waits += 1;
        self.waited = waited;
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

/// What a read error asks for: stop with no retry, or wait (the Retry-After when the answer had
/// one) and read again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFault {
    Stop,
    Busy(Option<Duration>),
}

/// Every HTTP status in the error text: a `status: ` followed by exactly three digits.
fn status_codes(error: &str) -> Vec<u16> {
    error
        .split("status: ")
        .skip(1)
        .filter_map(|after| {
            let digits = after.as_bytes();
            let three = digits.len() >= 3
                && digits[..3].iter().all(u8::is_ascii_digit)
                && !digits.get(3).is_some_and(u8::is_ascii_digit);
            three.then(|| after[..3].parse().ok()).flatten()
        })
        .collect()
}

/// The Retry-After header of an answer, as the text shows it.
enum RetryAfter {
    Missing,
    Seconds(Duration),
    /// Present, but not whole seconds (ASCII digits within u64): an HTTP date, a sign, an
    /// exponent, a space, an empty value or too many digits.
    Unreadable(String),
}

fn retry_after(error: &str) -> RetryAfter {
    let Some(rest) = error.split(r#""retry-after": ""#).nth(1) else {
        return RetryAfter::Missing;
    };
    let value = rest.split('"').next().unwrap_or_default();
    let digits = !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
    match value.parse::<u64>() {
        Ok(secs) if digits => RetryAfter::Seconds(Duration::from_secs(secs)),
        _ => RetryAfter::Unreadable(value.to_string()),
    }
}

/// The one rule for every read error, in this order: any 4xx but 429 (a 403 first) stops, wherever
/// it is in the text; then a 429 waits; then a 5xx waits; anything else stops. A wait takes the
/// Retry-After of the answer when it has one in whole seconds; a Retry-After the runner cannot
/// read stops, as any answer it does not understand.
pub fn read_fault(error: &str) -> ReadFault {
    if !asks_to_wait(error) {
        return ReadFault::Stop;
    }
    match retry_after(error) {
        RetryAfter::Missing => ReadFault::Busy(None),
        RetryAfter::Seconds(wait) => ReadFault::Busy(Some(wait)),
        RetryAfter::Unreadable(_) => ReadFault::Stop,
    }
}

/// No 4xx but 429 anywhere in the text, and a 429 or a 5xx.
fn asks_to_wait(error: &str) -> bool {
    let codes = status_codes(error);
    !codes.iter().any(|c| (400..500).contains(c) && *c != 429)
        && codes.iter().any(|c| *c == 429 || (500..600).contains(c))
}

/// `Some(retry_after)` when the error text is an HTTP 429 that [`read_fault`] waits on, with its
/// Retry-After when the text has one; `None` for anything else.
pub fn rate_limit_wait(error: &str) -> Option<Option<Duration>> {
    match read_fault(error) {
        ReadFault::Busy(wait) if status_codes(error).contains(&429) => Some(wait),
        _ => None,
    }
}

/// Runs `read` until it answers, waiting on each 429 or 5xx as `budget` allows ([`read_fault`]). A
/// 403 or any other 4xx, an unknown error, or a wait past the budget stops it with `what` in the
/// error and no URL in it.
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
        let ReadFault::Busy(retry_after) = read_fault(&error) else {
            if let (true, RetryAfter::Unreadable(value)) =
                (asks_to_wait(&error), self::retry_after(&error))
            {
                bail!(
                    "{what} got a Retry-After the runner does not read ({value:?}, not whole seconds); stopped, no retry: {}",
                    without_urls(&error)
                );
            }
            bail!(
                "unknown result of {what}: {}; stopped, no retry",
                without_urls(&error)
            );
        };
        let Some(wait) = budget.next(retry_after) else {
            bail!(
                "{what} got HTTP 429 or 5xx past the run's wait limit ({READ_MAX_WAITS} waits, {} s in all); stopped: {}",
                READ_WAIT_LIMIT.as_secs(),
                without_urls(&error)
            );
        };
        println!(
            "rate limit: {what} got HTTP 429 or 5xx; waiting {} s",
            wait.as_secs_f64()
        );
        tokio::time::sleep(wait).await;
    }
}
