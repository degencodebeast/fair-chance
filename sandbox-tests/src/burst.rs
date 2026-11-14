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
use crate::send_once;

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

/// Blocks after the send of an early sell until its receipt runs: one to get into a block, one to
/// run the receipt on the coin.
const INCLUDE_AND_EXECUTE_BLOCKS: u64 = 2;

/// The block, counted from the launch block, where the last early sell of the burst lands by this
/// estimate: tail + ceil((send + gap) / block) + 2. It refuses the run when that is after
/// `early_blocks`.
pub fn check_burst_timing(timing: &BurstTiming, early_blocks: u64) -> Result<u64> {
    let usable = timing.block_s.is_finite()
        && timing.block_s > 0.0
        && timing.send_s.is_finite()
        && timing.send_s >= 0.0
        && timing.early_sell_gap_s.is_finite()
        && timing.early_sell_gap_s >= 0.0;
    if !usable {
        bail!("the burst timing {timing:?} is not usable; nothing was signed");
    }
    let wait_blocks = ((timing.send_s + timing.early_sell_gap_s) / timing.block_s).ceil() as u64;
    let last = timing.launch_tail_blocks + wait_blocks + INCLUDE_AND_EXECUTE_BLOCKS;
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
    let mut buys = JoinSet::new();
    let mut early_sells = Vec::new();
    for (index, sniper) in snipers.into_iter().enumerate() {
        let (read, send, stopped) = (read.clone(), send.clone(), stopped.clone());
        let buyer = sniper.buyer.clone();
        buys.spawn(async move {
            tokio::time::sleep(settings.send_delay).await;
            let result = send_once(sniper.buy, &buyer, &read, &send).await;
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
                let (read, send) = (read.clone(), send.clone());
                tasks.spawn(async move {
                    tokio::time::sleep(settings.send_delay).await;
                    let result = send_once(sell, &buyer, &read, &send).await;
                    (index, buyer, result)
                });
            }
            Some(tasks.join_all().await)
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
