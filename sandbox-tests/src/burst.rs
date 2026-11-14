//! The pre-signed burst: every sniper buy, and one early sell per sniper, is signed before the
//! launch. When the launch result comes back, the run sends all buys at once, waits a fixed gap
//! and sends all early sells, with no read in between. [`check_burst_timing`] refuses the run
//! before anything is signed when the last of them would land after the early window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use near_api::advanced::ExecuteSignedTransaction;
use near_api::types::transaction::result::TransactionResult;
use near_api::types::AccountId;
use near_api::NetworkConfig;
use tokio::task::JoinSet;

use crate::rate_limit::without_urls;
use crate::{send_once, send_once_and_flag};

/// What the burst guard knows before the launch.
#[derive(Debug, Clone, Copy)]
pub struct BurstTiming {
    /// Seconds per block on the network.
    pub block_s: f64,
    /// Seconds one call to the send RPC takes, measured before the launch. Exactly one such call
    /// sits between the launch and an early sell landing: either the launch answer comes back
    /// late, or the send of the buy reaches the node late; a proxy delay on one side is never on
    /// both.
    pub send_s: f64,
    /// Blocks from the coin's launch block (its `new` receipt) to the launch transaction's last
    /// receipt, when the launch result comes back.
    pub launch_tail_blocks: u64,
    /// Seconds between sending the buys and sending the early sells, so each buy is in a block
    /// before the early sell with the next nonce reaches the node.
    pub early_sell_gap_s: f64,
}

/// The longest send call, gap or block time the guard takes, in seconds.
const MAX_TIMING_S: f64 = 600.0;
/// The shortest block time the guard takes, in seconds.
const MIN_BLOCK_S: f64 = 0.001;

/// Blocks after the send of an early sell until its receipt runs: one to get into a block, one to
/// run the receipt on the coin.
pub(crate) const INCLUDE_AND_EXECUTE_BLOCKS: u64 = 2;

/// The block, counted from the launch block, where the last early sell of the burst lands by this
/// estimate: tail + ceil((send + gap) / block) + 2. It refuses the run when that is after
/// `early_blocks`.
pub fn check_burst_timing(timing: &BurstTiming, early_blocks: u64) -> Result<u64> {
    let refuse =
        || anyhow::anyhow!("the burst timing {timing:?} is not usable; nothing was signed");
    let in_range = |s: f64| s.is_finite() && (0.0..=MAX_TIMING_S).contains(&s);
    if !(in_range(timing.block_s) && timing.block_s >= MIN_BLOCK_S)
        || !in_range(timing.send_s)
        || !in_range(timing.early_sell_gap_s)
    {
        return Err(refuse());
    }
    // At most MAX_TIMING_S x 2 / MIN_BLOCK_S blocks: a small whole number.
    let wait_blocks = ((timing.send_s + timing.early_sell_gap_s) / timing.block_s).ceil() as u64;
    let last = timing
        .launch_tail_blocks
        .checked_add(wait_blocks)
        .and_then(|b| b.checked_add(INCLUDE_AND_EXECUTE_BLOCKS))
        .ok_or_else(refuse)?;
    if last > early_blocks {
        bail!(
            "the last early sell would land in block {last} after the launch, after the early window (blocks 0 to {early_blocks}): {timing:?}; nothing was signed"
        );
    }
    Ok(last)
}

/// One sniper's two pre-signed transactions: its buy and its early sell with the next nonce.
pub struct SniperTxs {
    pub buyer: AccountId,
    pub buy: ExecuteSignedTransaction,
    pub early_sell: ExecuteSignedTransaction,
}

/// The fixed waits of the burst.
#[derive(Debug, Clone, Copy)]
pub struct BurstSettings {
    /// Time from sending the buys to sending the early sells.
    pub early_sell_gap: Duration,
    /// A wait before each send, on sandbox only: it stands in for the slow public RPC of testnet
    /// in a rehearsal. Zero on testnet.
    pub send_delay: Duration,
}

/// The chain's answer to every transaction of the burst, in sniper order.
#[derive(Debug)]
pub struct BurstResult {
    pub buys: Vec<TransactionResult>,
    pub early_sells: Vec<TransactionResult>,
}

/// Sends every pre-signed buy at once, then after `early_sell_gap` every early sell at once, with
/// no read in between. Each goes out once through [`send_once`]. When any buy send fails (a 403,
/// no answer, an unknown result) before the gap ends, no early sell is sent. Any failed send stops
/// the run, with each failed transaction named and no URL in the text.
pub async fn send_burst(
    read: &NetworkConfig,
    send: &NetworkConfig,
    snipers: Vec<SniperTxs>,
    settings: BurstSettings,
) -> Result<BurstResult> {
    let stopped = Arc::new(AtomicBool::new(false));
    let planned = snipers.len();
    let mut buys = JoinSet::new();
    let mut early_sells = Vec::new();
    for (index, sniper) in snipers.into_iter().enumerate() {
        let (read, send, stopped) = (read.clone(), send.clone(), stopped.clone());
        let buyer = sniper.buyer.clone();
        buys.spawn(async move {
            tokio::time::sleep(settings.send_delay).await;
            // Any answer but success (403, 429, 5xx, timeout, unknown, a failed buy) sets
            // `stopped` before the lookup by hash and its waits.
            let result = send_once_and_flag(sniper.buy, &buyer, &read, &send, Some(&stopped)).await;
            if result.is_err() {
                stopped.store(true, Ordering::SeqCst);
            }
            (index, buyer, result)
        });
        early_sells.push((sniper.buyer, sniper.early_sell));
    }
    let sells = {
        let (read, send, stopped) = (read.clone(), send.clone(), stopped.clone());
        tokio::spawn(async move {
            tokio::time::sleep(settings.early_sell_gap).await;
            if stopped.load(Ordering::SeqCst) {
                return None;
            }
            let mut tasks = JoinSet::new();
            for (index, (buyer, sell)) in early_sells.into_iter().enumerate() {
                let (read, send, stopped) = (read.clone(), send.clone(), stopped.clone());
                tasks.spawn(async move {
                    tokio::time::sleep(settings.send_delay).await;
                    // Checked again right before the send.
                    if stopped.load(Ordering::SeqCst) {
                        return None;
                    }
                    Some((
                        index,
                        buyer.clone(),
                        send_once(sell, &buyer, &read, &send).await,
                    ))
                });
            }
            let sent: Vec<_> = tasks.join_all().await.into_iter().flatten().collect();
            (!sent.is_empty()).then_some(sent)
        })
    };
    let bought = buys.join_all().await;
    let sold = sells.await?;
    let mut failed = Vec::new();
    let mut buy_results = Vec::new();
    for (index, buyer, result) in sorted(bought) {
        match result {
            Ok(r) => buy_results.push(r),
            Err(e) => failed.push(format!("buy {index} of {buyer}: {e}")),
        }
    }
    let mut sell_results = Vec::new();
    match sold {
        None => failed.push("no early sell was sent".to_string()),
        Some(sold) => {
            if sold.len() < planned {
                failed.push(format!(
                    "{} of {planned} early sells were not sent",
                    planned - sold.len()
                ));
            }
            for (index, buyer, result) in sorted(sold) {
                match result {
                    Ok(r) => sell_results.push(r),
                    Err(e) => failed.push(format!("early sell {index} of {buyer}: {e}")),
                }
            }
        }
    }
    if !failed.is_empty() {
        bail!(
            "the burst stopped: {}; nothing more is sent",
            without_urls(&failed.join("; "))
        );
    }
    Ok(BurstResult {
        buys: buy_results,
        early_sells: sell_results,
    })
}

fn sorted<T>(mut items: Vec<(usize, AccountId, T)>) -> Vec<(usize, AccountId, T)> {
    items.sort_by_key(|(index, _, _)| *index);
    items
}

/// Blocks from sending the launch to the coin's launch block: one to get the launch into a block,
/// one for the factory's receipt that makes the coin.
const LAUNCH_TO_COIN_BLOCKS: u64 = 2;

/// Checks the launch answer against the plan of [`check_burst_timing`]: it may take one send call
/// plus the blocks to the coin, the tail and the spare blocks of the estimate. A later answer
/// means the burst would land later than estimated, so the run stops before any buy is sent.
pub fn check_launch_answer(
    timing: &BurstTiming,
    early_blocks: u64,
    launch_answer_s: f64,
) -> Result<()> {
    let spare = early_blocks.saturating_sub(check_burst_timing(timing, early_blocks)?);
    // The tail is at most early_blocks here, since check_burst_timing passed.
    let blocks = LAUNCH_TO_COIN_BLOCKS + timing.launch_tail_blocks + spare;
    let allowed = timing.send_s + blocks as f64 * timing.block_s;
    if !launch_answer_s.is_finite() || launch_answer_s > allowed {
        bail!(
            "the launch answer took {launch_answer_s:.1} s, more than the {allowed:.1} s the burst plan allows; the coin exists, but no buy was sent"
        );
    }
    Ok(())
}
