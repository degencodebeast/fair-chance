//! A run on a coin the app launches: the runner does not launch. It makes the buyer accounts,
//! signs every sniper buy and early sell, then waits for the coin to appear on chain and sends the
//! burst at once. The launch transaction and the creator come from the chain, never from the
//! runner, and the run refuses a coin its own funder launched or a coin with unknown code.

use std::future::Future;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use near_api::types::transaction::result::TransactionResult;
use near_api::types::{AccountId, CryptoHash, NearGas, NearToken, Reference};
use near_api::NetworkConfig;
use near_openapi_client::types;
use serde_json::{json, Value};

use crate::burst::{send_burst, BurstSettings};
use crate::buyer_funding::{buyer_need, check_buyer_funding, gas_prices};
use crate::option_t::{
    balance, burst_guard, check_after_burst, make_buyer_accounts, measured_block_s,
    presign_snipers, send_call_s, AfterBurst, OptionTArguments, OptionTReport, OptionTSetup,
    EARLY_SELL_GAP,
};
use crate::pass_list::PassList;
use crate::rate_limit::{
    read_fault, run_budget, with_rate_limit_backoff, without_urls, Budget, ReadFault,
};
use crate::spend_guard::check_funder;
use crate::{
    account_view, events, launch_args, transaction_hash, view_on, Account, LAUNCH_DEPOSIT,
};

/// The gas each buy (and each sell after the window) reserves in the run on a coin the app
/// launches: a CABAL2 buy burnt 5.04 Tgas of 100. The node buys the reserved gas at 1e9
/// yoctoNEAR per gas, so 300 Tgas costs 0.3 NEAR a buy.
pub const APP_COIN_BUYER_GAS: NearGas = NearGas::from_tgas(100);
/// The gas the run that launches reserves per buy unless `--buyer-tgas` says otherwise.
const LAUNCHING_RUN_BUYER_GAS: NearGas = NearGas::from_tgas(300);

/// `--buyer-tgas` when given; else [`APP_COIN_BUYER_GAS`] with `--coin`, and the launching run's
/// 300 Tgas without it.
pub fn buyer_gas(args: &OptionTArguments) -> NearGas {
    match (args.buyer_tgas, &args.coin) {
        (Some(tgas), _) => NearGas::from_tgas(tgas),
        (None, Some(_)) => APP_COIN_BUYER_GAS,
        (None, None) => LAUNCHING_RUN_BUYER_GAS,
    }
}

/// What `--coin` asks for: the coin account and its label, the first part of its name, which
/// names the runner's accounts (`<label>-buyer-01`, `<label>-later-buyer`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCoinRequest {
    pub coin: AccountId,
    pub label: String,
}

/// `None` without `--coin`. With it: the coin must be `<label>.<factory>`, and no flag of a run
/// that launches, or of a BAG step, may come with it.
pub fn app_coin_request(
    args: &OptionTArguments,
    factory: &AccountId,
) -> Result<Option<AppCoinRequest>> {
    let Some(coin) = args.coin.as_deref() else {
        return Ok(None);
    };
    for (flag, given) in [
        ("--ticker", args.ticker.is_some()),
        ("--name", args.name.is_some()),
        ("--description", args.description.is_some()),
        (
            "--launch-deposit-millinear",
            args.launch_deposit_millinear.is_some(),
        ),
        ("--sells-only", args.sells_only),
        ("--delete-buyers", args.delete_buyers),
        ("--continue", args.continue_sells),
        ("--return-to-funder", args.return_to_funder),
        ("--sell-plan", args.sell_plan.is_some()),
    ] {
        if given {
            bail!("{flag} does not go with --coin: the app launches that coin; nothing was run");
        }
    }
    let refuse = || {
        anyhow::anyhow!(
            "--coin takes a coin of the factory {factory} (<label>.{factory}), not {coin:?}; nothing was run"
        )
    };
    let parsed: AccountId = coin.parse().map_err(|_| refuse())?;
    let label = parsed
        .as_str()
        .strip_suffix(&format!(".{factory}"))
        .filter(|label| !label.is_empty() && !label.contains('.'))
        .ok_or_else(refuse)?
        .to_string();
    Ok(Some(AppCoinRequest {
        coin: parsed,
        label,
    }))
}

/// What the chain says about the coin when it appears: the code hash of the account, and from
/// `fc_rules` the code hash the coin recorded, its creator and its early window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCoinFacts {
    pub account_code_hash: String,
    pub rules_code_hash: String,
    pub creator: String,
    pub early_blocks: u64,
}

/// The checks before any buy is sent: the known code on the account and in `fc_rules`, the fixed
/// early window, and a creator that is neither the funder nor one of its sub-accounts (the runner
/// signs for those, so such a coin could be the runner's own launch).
pub fn check_app_coin(
    facts: &AppCoinFacts,
    funder: &AccountId,
    known_code_hash: &str,
    early_blocks: u64,
) -> Result<()> {
    for (what, hash) in [
        ("the coin account's code hash", &facts.account_code_hash),
        ("fc_rules.code_hash", &facts.rules_code_hash),
    ] {
        if hash != known_code_hash {
            bail!("{what} is {hash}, not the known {known_code_hash}; no buy was sent");
        }
    }
    if facts.early_blocks != early_blocks {
        bail!(
            "fc_rules.early_blocks is {}, not {early_blocks}; no buy was sent",
            facts.early_blocks
        );
    }
    let creator = facts.creator.as_str();
    if creator == funder.as_str() || creator.ends_with(&format!(".{funder}")) {
        bail!(
            "the coin's creator {creator} is the funder {funder} or one of its sub-accounts, so the runner could have launched it; the app must launch it from another account; no buy was sent"
        );
    }
    Ok(())
}

/// Checks the blocks from the coin's launch block to the read that found it against the plan of
/// [`crate::burst::check_burst_timing`]: the planned detect blocks plus the spare blocks of the
/// estimate. A later find means the burst would land later than estimated, so no buy is sent.
pub fn check_detection(
    timing: &crate::burst::BurstTiming,
    early_blocks: u64,
    lag_blocks: u64,
) -> Result<()> {
    let last = crate::burst::check_burst_timing(timing, early_blocks)?;
    let allowed = timing.launch_tail_blocks + early_blocks.saturating_sub(last);
    if lag_blocks > allowed {
        bail!(
            "the coin was found {lag_blocks} blocks after its launch block, more than the {allowed} the burst plan allows; no buy was sent"
        );
    }
    Ok(())
}

/// The block, counted from the launch block, where the buys should land when the coin was found
/// `lag_blocks` after its launch block: one send call, then one block to get in and one to run on
/// the coin (the estimate of [`crate::burst::check_burst_timing`] without the early-sell gap).
pub fn expected_buy_block(timing: &crate::burst::BurstTiming, lag_blocks: u64) -> u64 {
    let send_blocks = (timing.send_s / timing.block_s).ceil() as u64;
    lag_blocks + send_blocks + crate::burst::INCLUDE_AND_EXECUTE_BLOCKS
}

/// One read of the coin: not there yet, or there with what the read found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe<T> {
    Missing,
    Found(T),
}

/// Reads the coin with `probe` every `interval` until it is found, for `deadline` at most. A 429 or
/// a 5xx waits as `budget` allows (the shared rule: 2 s doubling, at most two waits in the run); a
/// 403, any other answer, or a busy answer past the budget stops it with no retry. The error
/// texts carry no URL.
pub async fn wait_for_app_coin<T, F, Fut>(
    mut probe: F,
    interval: Duration,
    budget: &Budget,
    deadline: Duration,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<Probe<T>, String>>,
{
    let started = Instant::now();
    loop {
        match probe().await {
            Ok(Probe::Found(found)) => return Ok(found),
            Ok(Probe::Missing) => {}
            Err(error) => {
                let error = without_urls(&error);
                let ReadFault::Busy(retry_after) = read_fault(&error) else {
                    bail!("unknown result of the read of the coin: {error}; stopped, no retry");
                };
                let Some(wait) = budget.next(retry_after) else {
                    bail!(
                        "the read of the coin got a 429 or 5xx past the run's wait limit; stopped: {error}"
                    );
                };
                println!(
                    "app-coin: the read of the coin got a 429 or 5xx; waiting {} s",
                    wait.as_secs_f64()
                );
                tokio::time::sleep(wait).await;
                continue;
            }
        }
        if started.elapsed() >= deadline {
            bail!(
                "the coin did not appear within {} s; stopped, no buy was sent",
                deadline.as_secs()
            );
        }
        tokio::time::sleep(interval).await;
    }
}

/// The code hash of the testnet coin (the reproducible build the factory deployed; the page's
/// known-deployments file names the same hash).
pub const TESTNET_COIN_CODE_HASH: &str = "6Ld5DFTJ1BYc6uzb6YYvdK3yAMHdfBsvrwNEhsKE9er3";
/// Blocks from the coin's launch block to the read that finds it, in the burst plan: the coin is
/// in the optimistic head from its launch block, one poll interval and the account read find it,
/// and one `fc_rules` read follows.
pub const DETECT_BLOCKS: u64 = 2;
/// Time between two reads of the coin while it is missing.
pub const POLL_INTERVAL: Duration = Duration::from_millis(300);
/// The longest wait for the app's launch; then the run stops with no buy sent.
pub const COIN_WAIT_LIMIT: Duration = Duration::from_secs(30 * 60);
/// Blocks before the coin's launch block in which the run looks for the launch transaction.
pub const LAUNCH_SEARCH_BLOCKS: u64 = 10;

/// The run on a coin the app launched: the Option T report, and what the chain says about the
/// launch.
#[derive(Debug)]
pub struct AppCoinReport {
    pub run: OptionTReport,
    pub launch_tx: CryptoHash,
    pub creator: AccountId,
    pub launch_block: u64,
}

/// What one read found when the coin appeared.
#[derive(Debug, Clone)]
struct FoundCoin {
    facts: AppCoinFacts,
    launch_block: u64,
    /// The block of the `fc_rules` read: how far the head was when the run found the coin.
    read_block: u64,
}

/// One read of the coin at the optimistic head: its account (missing until the launch), then its
/// `fc_rules`. A node that knows the account but not yet its state counts as missing.
async fn read_coin(
    network: &NetworkConfig,
    coin: &AccountId,
) -> std::result::Result<Probe<FoundCoin>, String> {
    let missing = |text: &str| text.contains("UnknownAccount") || text.contains("UNKNOWN_ACCOUNT");
    let account = match near_api::Account(coin.clone())
        .view()
        .at(Reference::Optimistic)
        .fetch_from(network)
        .await
    {
        Ok(view) => view.data,
        Err(e) => {
            let text = format!("{e:?}");
            return if missing(&text) {
                Ok(Probe::Missing)
            } else {
                Err(text)
            };
        }
    };
    let rules = match near_api::Contract(coin.clone())
        .call_function("fc_rules", json!({}))
        .read_only::<Value>()
        .at(Reference::Optimistic)
        .fetch_from(network)
        .await
    {
        Ok(rules) => rules,
        Err(e) => {
            let text = format!("{e:?}");
            return if missing(&text) {
                Ok(Probe::Missing)
            } else {
                Err(text)
            };
        }
    };
    let launch_block = rules.data["launch_block"]
        .as_u64()
        .ok_or_else(|| "fc_rules has no launch_block".to_string())?;
    let text = |name: &str| rules.data[name].as_str().unwrap_or_default().to_string();
    Ok(Probe::Found(FoundCoin {
        facts: AppCoinFacts {
            account_code_hash: code_hash(&account.contract_state),
            rules_code_hash: text("code_hash"),
            creator: text("creator"),
            early_blocks: rules.data["early_blocks"].as_u64().unwrap_or_default(),
        },
        launch_block,
        read_block: rules.block_height,
    }))
}

/// The code hash an account runs: its global or local code hash; otherwise a text that matches no
/// hash.
fn code_hash(state: &near_api::types::account::ContractState) -> String {
    use near_api::types::account::ContractState;
    match state {
        ContractState::GlobalHash(hash) | ContractState::LocalHash(hash) => hash.to_string(),
        ContractState::GlobalAccountId(id) => format!("the code of {id}"),
        ContractState::None => "no code".to_string(),
    }
}

/// The run on a coin the app launches, start to end. Before any signature: the factory makes coins
/// with the known code, the burst guard passes, and the coin does not exist yet. Then it makes the
/// buyers, signs every buy and early sell, calls `ready` and waits for the coin. When the coin is
/// there and passes [`check_app_coin`] and [`check_detection`], it sends the burst at once, finds
/// the launch transaction on chain and runs the same checks as a run that launches.
pub async fn run_app_coin(
    setup: &OptionTSetup,
    request: &AppCoinRequest,
    known_code_hash: &str,
    list: &mut PassList,
    ready: impl FnOnce(),
) -> Result<AppCoinReport> {
    let say = |text: &str| println!("option-t: {text}");
    let coin = setup.coin()?;
    if coin != request.coin {
        bail!(
            "the run would buy {coin}, not {}; nothing was signed",
            request.coin
        );
    }
    let funder_before = balance(&setup.network, &setup.funder.id).await?;
    say(&format!(
        "make {} buyer accounts{} under {}; the app launches {coin}",
        setup.buyers,
        if setup.later_buy.is_zero() {
            ""
        } else {
            " and one later buyer"
        },
        setup.funder.id
    ));
    let factory_hash = view_on(&setup.network, &setup.factory, "coin_code_hash", json!({})).await?;
    if factory_hash.as_str() != Some(known_code_hash) {
        bail!(
            "the factory {} makes coins with code {factory_hash}, not the known {known_code_hash}; nothing was signed",
            setup.factory
        );
    }
    let block_s = measured_block_s(&setup.network).await?;
    let (_, first_estimate) =
        burst_guard(setup, block_s, send_call_s(setup).await?, DETECT_BLOCKS)?;
    say(&format!(
        "burst guard before any signature: {block_s:.4} s per block; the last early sell lands by block {first_estimate} (estimate)"
    ));
    if account_view(&setup.network, &coin).await?.is_some() {
        bail!("{coin} exists already; the run must wait before the app launches it. Nothing was signed");
    }
    let prices = gas_prices(&setup.network).await?;
    check_buyer_funding(
        setup.buy_near,
        setup.later_buy,
        setup.buyer_extra,
        setup.buyer_gas,
        prices.used,
    )?;
    say(&format!(
        "funding check before any signature: gas price {}, min_gas_purchase_price {}, used {} yoctoNEAR per gas, {} Tgas per buy; each early buyer needs {}, holds {}",
        prices.gas_price,
        prices.min_purchase,
        prices.used,
        setup.buyer_gas.as_tgas(),
        buyer_need(setup.buy_near, setup.buyer_gas, prices.used).exact_amount_display(),
        setup.buy_near.saturating_add(setup.buyer_extra).exact_amount_display()
    ));
    let (buyers, later) = make_buyer_accounts(setup).await?;
    check_funder(
        setup.limits.as_ref(),
        &setup.network,
        &setup.funder.id,
        NearToken::from_yoctonear(0),
        "the wait for the coin",
    )
    .await?;

    let early_blocks = rule_engine::FixedValues::fixed().early_blocks;
    let (snipers, send_s) = presign_snipers(setup, &coin, &buyers).await?;
    let (burst_timing, estimate) = burst_guard(setup, block_s, send_s, DETECT_BLOCKS)?;
    say(&format!(
        "signed {} buys and {} early sells before the coin exists; {block_s:.4} s per block, {:.2} s per send call; the last early sell lands by block {estimate} of 0 to {early_blocks} (estimate)",
        snipers.len(),
        snipers.len(),
        burst_timing.send_s
    ));
    say(&format!(
        "waiting for {coin}: a read every {} ms, {} min at most; launch it in the app now",
        POLL_INTERVAL.as_millis(),
        COIN_WAIT_LIMIT.as_secs() / 60
    ));
    ready();
    let found = wait_for_app_coin(
        || read_coin(&setup.network, &coin),
        POLL_INTERVAL,
        run_budget(),
        COIN_WAIT_LIMIT,
    )
    .await?;
    let lag = found.read_block.saturating_sub(found.launch_block);
    check_app_coin(
        &found.facts,
        &setup.funder.id,
        known_code_hash,
        early_blocks,
    )?;
    check_detection(&burst_timing, early_blocks, lag)?;
    say(&format!(
        "detected {coin}: launch block {}, detect block {} ({lag} after the launch block), buys expected by block {} after the launch block; creator {}, code {}",
        found.launch_block,
        found.read_block,
        expected_buy_block(&burst_timing, lag),
        found.facts.creator,
        found.facts.account_code_hash
    ));
    let started = Instant::now();
    let burst = send_burst(
        &setup.network,
        &setup.send_network,
        snipers,
        BurstSettings {
            early_sell_gap: EARLY_SELL_GAP,
            send_delay: setup.send_delay,
        },
    )
    .await?;
    let seconds = started.elapsed().as_secs_f64();
    let creator: AccountId = found.facts.creator.parse()?;
    let (launch_tx, launched) = find_launch_tx(
        &setup.network,
        &setup.factory,
        &creator,
        &coin,
        found.launch_block,
    )
    .await?;
    say(&format!(
        "launch tx {launch_tx} by {creator}, read from the chain"
    ));
    let run = check_after_burst(
        setup,
        list,
        AfterBurst {
            coin: &coin,
            creator: None,
            buyers: &buyers,
            later: later.as_ref(),
            launched: &launched,
            burst,
            seconds,
            block_s,
            funder_before,
        },
    )
    .await?;
    Ok(AppCoinReport {
        run,
        launch_tx,
        creator,
        launch_block: found.launch_block,
    })
}

/// The block at `height`, or `None` when the chain has no block there (a skipped height).
async fn block_at(
    network: &NetworkConfig,
    height: u64,
) -> Result<Option<near_api::types::RpcBlockResponse>> {
    with_rate_limit_backoff(&format!("block {height}"), run_budget(), || async {
        match near_api::Chain::block()
            .at(Reference::AtBlock(height))
            .fetch_from(network)
            .await
        {
            Ok(block) => Ok(Some(block)),
            Err(e) if format!("{e:?}").contains("UnknownBlock") => Ok(None),
            Err(e) => Err(e),
        }
    })
    .await
}

/// Finds the launch transaction of `coin` on chain: in the chunks of the blocks from
/// [`LAUNCH_SEARCH_BLOCKS`] before the launch block up to it, a transaction of `creator` to the
/// factory whose outcome logs `fc_launch` for `coin`. Reads only; the first read RPC answers.
pub async fn find_launch_tx(
    network: &NetworkConfig,
    factory: &AccountId,
    creator: &AccountId,
    coin: &AccountId,
    launch_block: u64,
) -> Result<(CryptoHash, TransactionResult)> {
    let endpoint = network
        .rpc_endpoints
        .first()
        .context("the read network has no RPC")?;
    let client = near_openapi_client::Client::new(endpoint.url.as_str().trim_end_matches('/'));
    let from = launch_block.saturating_sub(LAUNCH_SEARCH_BLOCKS);
    let mut candidates = Vec::new();
    for height in from..=launch_block {
        let Some(block) = block_at(network, height).await? else {
            continue;
        };
        for chunk in &block.chunks {
            // A chunk header repeated from an earlier block holds no new transaction.
            if chunk.height_included != height {
                continue;
            }
            let request = types::JsonRpcRequestForChunk {
                id: "fc".to_string(),
                jsonrpc: "2.0".to_string(),
                method: types::JsonRpcRequestForChunkMethod::Chunk,
                params: types::RpcChunkRequest::ChunkHash {
                    chunk_id: chunk.chunk_hash.clone(),
                },
            };
            let answer = with_rate_limit_backoff(
                &format!("a chunk of block {height}"),
                run_budget(),
                || client.chunk(&request),
            )
            .await?
            .into_inner();
            let chunk = match answer {
                types::JsonRpcResponseForRpcChunkResponseAndRpcChunkError::Variant0 {
                    result,
                    ..
                } => result,
                types::JsonRpcResponseForRpcChunkResponseAndRpcChunkError::Variant1 {
                    error,
                    ..
                } => bail!(
                    "a chunk of block {height} gave an error: {}; stopped",
                    without_urls(&format!("{error:?}"))
                ),
            };
            for tx in chunk.transactions {
                if &tx.receiver_id == factory && &tx.signer_id == creator {
                    candidates.push(CryptoHash::from(tx.hash));
                }
            }
        }
    }
    for hash in candidates {
        let outcome =
            with_rate_limit_backoff(&format!("the status of tx {hash}"), run_budget(), || {
                near_api::Transaction::status_with_options(
                    creator.clone(),
                    hash,
                    near_api::types::TxExecutionStatus::Final,
                )
                .fetch_from(network)
            })
            .await?;
        let result = TransactionResult::Full(Box::new(outcome));
        let names_coin = events(&result, "fair_chance_factory", "fc_launch")
            .iter()
            .any(|e| e.event["data"][0]["coin"] == coin.as_str());
        if result.is_success() && names_coin {
            return Ok((hash, result));
        }
    }
    bail!(
        "no transaction of {creator} to {factory} that logs fc_launch for {coin} in blocks {from} to {launch_block}"
    )
}

/// A stand-in for the app, for a sandbox rehearsal only: when `go` fires (the runner waits for the
/// coin), it waits a random time up to `max_delay`, then `launcher` launches the coin `<label>`
/// through the factory, as the app's launch form does. Returns the launch transaction's hash.
pub async fn stand_in_launch(
    network: NetworkConfig,
    factory: AccountId,
    launcher: Account,
    label: String,
    max_delay: Duration,
    go: tokio::sync::oneshot::Receiver<()>,
) -> Result<CryptoHash> {
    go.await
        .context("the runner stopped before it waited for the coin")?;
    let delay = random_delay(max_delay);
    println!(
        "stand-in: the app launches {label} as {} in {:.1} s",
        launcher.id,
        delay.as_secs_f64()
    );
    tokio::time::sleep(delay).await;
    let launched = near_api::Contract(factory)
        .call_function(
            "launch",
            launch_args(
                "Cabal Test",
                &label.to_uppercase(),
                None,
                Some("A stand-in launch for a sandbox rehearsal."),
                None,
            ),
        )
        .transaction()
        .deposit(LAUNCH_DEPOSIT)
        .gas(near_api::types::NearGas::from_tgas(300))
        .with_signer(launcher.id.clone(), launcher.signer.clone())
        .send_to(&network)
        .await?;
    if !launched.is_success() {
        bail!("the stand-in launch failed: {launched:?}");
    }
    let hash = transaction_hash(&launched)?;
    println!("stand-in: launch tx {hash}");
    Ok(hash)
}

/// Joins a rehearsal: the runner's outcome first. When the runner stopped, the stand-in is
/// stopped too (it may not have launched yet) and the runner's own reason is the error, so a
/// runner that stops before it waits is never reported as the stand-in's "channel closed". When
/// the runner ended well, the stand-in's answer follows: its launch hash, or its own error.
pub async fn join_app_coin_run<T>(
    runner: Result<T>,
    stand_in: tokio::task::JoinHandle<Result<CryptoHash>>,
) -> Result<(T, CryptoHash)> {
    let run = match runner {
        Ok(run) => run,
        Err(stopped) => {
            stand_in.abort();
            return Err(stopped);
        }
    };
    let launched = stand_in.await.context("the stand-in task ended badly")??;
    Ok((run, launched))
}

/// A delay from 0 up to `max`, from the clock's nanoseconds: enough spread for a rehearsal.
fn random_delay(max: Duration) -> Duration {
    let millis = max.as_millis();
    if millis == 0 {
        return Duration::ZERO;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    Duration::from_millis((u128::from(nanos) % millis) as u64)
}
