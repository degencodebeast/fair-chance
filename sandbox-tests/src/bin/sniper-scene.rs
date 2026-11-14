//! The sniper cabal scene on a fresh near-sandbox, in one command. Writes every action to a JSON
//! file; it signs only with throwaway sandbox keys and never touches testnet or mainnet.
//!
//! sniper-scene <out.json>
//!
//! 1. The creator launches a coin through the factory with an opening buy (the judge path's 8 NEAR).
//! 2. One funder wallet (`cabal.sandbox`) makes 10 sub-accounts. Each sends storage_deposit + buy
//!    in one transaction at once after the launch, so the buys land in blocks 0 to N: marked.
//! 3. Each sniper tries to sell its whole bag in blocks 0 to N: refused with FC_EARLY_WINDOW.
//! 4. After block N, 3 ordinary buyers buy (unmarked).
//! 5. The 10 snipers sell their whole bags, one after another. Before each sell the harness reads
//!    fc_quote_sell for the same amount, and after each sell fc_early_window and fc_pool.
//! 6. The 3 buyers sell their whole bags.
//! 7. When CREATOR_SELLS is on, the creator sells its whole bag, so all early supply together
//!    uses up the shared allowance.
//! Then (second coin, same sandbox): one early wallet and one late wallet make the same buy and
//! the same sell after the window.
//!
//! Needs NEAR_SANDBOX_BIN_PATH (near-sandbox 2.13.4) and the built wasm (scripts/build-contracts.sh).
//! Exits 1 when any check fails, for example an early sell that lands after block N; the run
//! file is still written for diagnosis, and the analysis refuses it.

use anyhow::{bail, Context, Result};
use fair_chance_sandbox_tests::{
    batch_on, check_sandbox_binary, launch_args, transact, wait_for_final_height, wasm, Account,
    FunctionCall, COIN_ICON, LAUNCH_DEPOSIT,
};
use near_api::types::transaction::result::TransactionResult;
use near_api::types::{AccountId, CryptoHash, NearToken};
use near_api::{Contract, NetworkConfig, Reference, Signer};
use near_sandbox::{GenesisAccount, Sandbox, SandboxConfig};
use serde_json::{json, Value};

const SNIPERS: usize = 10;
const SNIPER_BUY: NearToken = NearToken::from_near(3);
const BUYERS: usize = 3;
const BUYER_BUY: NearToken = NearToken::from_near(1);
/// The judge path's opening buy (sandbox-tests/src/bin/judge-path.rs OPENING_BUY).
const OPENING_BUY: NearToken = NearToken::from_near(8);
/// The snipers send their buys when the head reaches launch block + SNIPE_AT.
const SNIPE_AT: u64 = 7;
/// Sandbox block time floor in milliseconds (FC_SCENE_MIN_BLOCK_MS overrides it).
const MIN_BLOCK_MS: u64 = 600;
/// NEAR each new sub-account starts with: the buy plus storage and gas.
const ACCOUNT_NEAR: NearToken = NearToken::from_near(5);
/// The creator sells its whole bag at the end of the scene (step 7).
const CREATOR_SELLS: bool = true;

struct Chain {
    _sandbox: Sandbox,
    network: NetworkConfig,
    factory: Account,
    creator: Account,
    cabal: Account,
    funder2: Account,
    coin_code_hash: CryptoHash,
}

fn genesis(name: &str) -> Result<(GenesisAccount, Account)> {
    let account = GenesisAccount::generate_with_name(name.parse()?);
    let signer = Signer::from_secret_key(account.private_key.parse()?)?;
    Ok((
        account.clone(),
        Account {
            id: account.account_id.clone(),
            signer,
        },
    ))
}

async fn start() -> Result<Chain> {
    check_sandbox_binary()?;
    let coin_code = wasm("fair_chance_coin")?;
    let factory_code = wasm("fair_chance_factory")?;
    let (fg, factory) = genesis("factory.sandbox")?;
    let (cg, creator) = genesis("creator.sandbox")?;
    let (kg, cabal) = genesis("cabal.sandbox")?;
    let (bg, funder2) = genesis("buyers.sandbox")?;
    let mut config = json!({ "gc_num_epochs_to_keep": 10_000 });
    // Blocks every MIN_BLOCK_MS, so a transaction sent at one head lands at the same offset from
    // the launch on every run (the early buy fee depends on that offset).
    let ms: u64 = std::env::var("FC_SCENE_MIN_BLOCK_MS")
        .map(|v| v.parse())
        .unwrap_or(Ok(MIN_BLOCK_MS))?;
    config["consensus"] = json!({
        "min_block_production_delay": { "secs": ms / 1000, "nanos": (ms % 1000) * 1_000_000 },
        "max_block_production_delay": { "secs": 2 * ms / 1000, "nanos": (2 * ms % 1000) * 1_000_000 },
    });
    let sandbox = Sandbox::start_sandbox_with_config(SandboxConfig {
        additional_accounts: vec![fg, cg, kg, bg],
        additional_config: Some(config),
        ..Default::default()
    })
    .await?;
    let network = NetworkConfig::from_rpc_url("sandbox", sandbox.rpc_addr.parse()?);
    let coin_code_hash = CryptoHash::hash(&coin_code);
    Contract::deploy_global_contract_code(coin_code)
        .as_hash()
        .with_signer(factory.id.clone(), factory.signer.clone())
        .send_to(&network)
        .await?
        .assert_success();
    Contract::deploy(factory.id.clone())
        .use_code(factory_code)
        .with_init_call(
            "new",
            json!({ "coin_code_hash": coin_code_hash.to_string() }),
        )?
        .with_signer(factory.signer.clone())
        .send_to(&network)
        .await?
        .assert_success();
    Ok(Chain {
        _sandbox: sandbox,
        network,
        factory,
        creator,
        cabal,
        funder2,
        coin_code_hash,
    })
}

/// A new sub-account of `funder` with its own fresh key, funded by `funder`.
async fn sub_account(chain: &Chain, funder: &Account, name: &str) -> Result<Account> {
    let id: AccountId = format!("{name}.{}", funder.id).parse()?;
    let secret = near_api::signer::generate_secret_key()?;
    let made = near_api::Account::create_account(id.clone())
        .fund_myself(funder.id.clone(), ACCOUNT_NEAR)
        .with_public_key(secret.public_key())
        .with_signer(funder.signer.clone())
        .send_to(&chain.network)
        .await?;
    if !made.is_success() {
        bail!("creating {id} failed: {made:?}");
    }
    Ok(Account {
        id,
        signer: Signer::from_secret_key(secret)?,
    })
}

async fn view_at(
    network: &NetworkConfig,
    coin: &AccountId,
    method: &str,
    args: Value,
    at: Reference,
) -> Result<Value> {
    Ok(Contract(coin.clone())
        .call_function(method, args)
        .read_only::<Value>()
        .at(at)
        .fetch_from(network)
        .await?
        .data)
}

async fn view(chain: &Chain, coin: &AccountId, method: &str, args: Value) -> Result<Value> {
    view_at(&chain.network, coin, method, args, Reference::Optimistic).await
}

async fn head(chain: &Chain) -> Result<u64> {
    Ok(near_api::Chain::block_number()
        .at(Reference::Optimistic)
        .fetch_from(&chain.network)
        .await?)
}

/// Waits until the optimistic head is at `height` or above.
async fn wait_head(chain: &Chain, height: u64) -> Result<()> {
    let now = head(chain).await?;
    if now > height {
        bail!("the head is at {now}, past {height}; the setup took too long");
    }
    while head(chain).await? < height {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    Ok(())
}

async fn block_height(chain: &Chain, hash: CryptoHash) -> Result<u64> {
    Ok(near_api::Chain::block()
        .at(Reference::AtBlockHash(hash))
        .fetch_from(&chain.network)
        .await?
        .header
        .height)
}

fn amount(v: &Value) -> Result<u128> {
    Ok(v.as_str()
        .with_context(|| format!("{v} is not a decimal string"))?
        .parse()?)
}

/// The facts of one transaction: hash, success, each receipt outcome (id, executor, block,
/// success), every NEP-297 event, and the refusal code when it failed.
async fn record(chain: &Chain, result: &TransactionResult) -> Result<Value> {
    let TransactionResult::Full(full) = result else {
        bail!("the transaction has no execution result")
    };
    let tx_hash = full
        .outcomes()
        .first()
        .context("no outcome")?
        .transaction_hash
        .to_string();
    let mut receipts = Vec::new();
    let mut events = Vec::new();
    let mut coin_block = None;
    for o in full.receipt_outcomes() {
        let height = block_height(chain, o.block_hash).await?;
        receipts.push(json!({
            "receipt_id": o.transaction_hash.to_string(),
            "executor": o.executor_id.to_string(),
            "block": height,
            "success": o.is_success(),
        }));
        if coin_block.is_none() && o.executor_id.as_str() != result_signer(result) {
            coin_block = Some(height);
        }
        for line in &o.logs {
            if let Some(text) = line.strip_prefix("EVENT_JSON:") {
                if let Ok(event) = serde_json::from_str::<Value>(text) {
                    events.push(json!({
                        "receipt_id": o.transaction_hash.to_string(),
                        "executor": o.executor_id.to_string(),
                        "block": height,
                        "standard": event["standard"],
                        "event": event["event"],
                        "data": event["data"][0],
                    }));
                }
            }
        }
    }
    let success = result.is_success();
    let refusal = if success {
        Value::Null
    } else {
        let text = format!("{result:?}");
        let code = text
            .find("FC_")
            .map(|i| {
                text[i..]
                    .chars()
                    .take_while(|c| c.is_ascii_uppercase() || *c == '_' || c.is_ascii_digit())
                    .collect::<String>()
            })
            .unwrap_or_default();
        let failed_block = match full.receipt_outcomes().iter().find(|o| !o.is_success()) {
            Some(o) => Some(block_height(chain, o.block_hash).await?),
            None => None,
        };
        json!({ "code": code, "failed_receipt_block": failed_block })
    };
    Ok(json!({
        "tx_hash": tx_hash,
        "success": success,
        "coin_receipt_block": coin_block,
        "receipts": receipts,
        "events": events,
        "refusal": refusal,
    }))
}

fn result_signer(result: &TransactionResult) -> &str {
    result.transaction().signer_id().as_str()
}

fn fair_event<'a>(rec: &'a Value, name: &str) -> Option<&'a Value> {
    rec["events"]
        .as_array()?
        .iter()
        .find(|e| e["standard"] == "fair_chance" && e["event"] == name)
        .map(|e| &e["data"])
}

async fn balance(chain: &Chain, coin: &AccountId, who: &AccountId) -> Result<u128> {
    amount(&view(chain, coin, "ft_balance_of", json!({ "account_id": who })).await?)
}

async fn snapshot(chain: &Chain, coin: &AccountId, label: &str) -> Result<Value> {
    let height = head(chain).await?;
    let at = Reference::AtBlock(height);
    Ok(json!({
        "label": label,
        "block": height,
        "fc_pool": view_at(&chain.network, coin, "fc_pool", json!({}), at.clone()).await?,
        "fc_early_window": view_at(&chain.network, coin, "fc_early_window", json!({}), at).await?,
    }))
}

async fn sell(
    chain: &Chain,
    who: &Account,
    coin: &AccountId,
    tokens: u128,
) -> Result<TransactionResult> {
    transact(
        &chain.network,
        who,
        coin,
        "sell",
        json!({ "amount": tokens.to_string(), "min_near_out": "0" }),
        NearToken::from_yoctonear(1),
    )
    .await
}

async fn register_and_buy(
    chain: &Chain,
    who: &Account,
    coin: &AccountId,
    storage_min: u128,
    near_in: NearToken,
) -> Result<TransactionResult> {
    batch_on(
        &chain.network,
        who,
        coin,
        vec![
            FunctionCall::new(
                "storage_deposit",
                json!({}),
                NearToken::from_yoctonear(storage_min),
            ),
            FunctionCall::new("buy", json!({ "min_tokens_out": "0" }), near_in),
        ],
    )
    .await
}

/// Launches a coin with the opening buy. Returns the coin, the raw result (recorded later, so the
/// early window is not spent on RPC reads), `fc_rules`, the launch block, N and the storage minimum.
async fn launch_coin(
    chain: &Chain,
    ticker: &str,
    name: &str,
    desc: &str,
) -> Result<(AccountId, TransactionResult, Value, u64, u64, u128)> {
    let launched = transact(
        &chain.network,
        &chain.creator,
        &chain.factory.id,
        "launch",
        launch_args(name, ticker, Some(COIN_ICON), Some(desc), Some(OPENING_BUY)),
        LAUNCH_DEPOSIT.saturating_add(OPENING_BUY),
    )
    .await?;
    if !launched.is_success() {
        bail!("launch failed: {launched:?}");
    }
    let coin: AccountId = format!("{ticker}.{}", chain.factory.id).parse()?;
    let rules = view(chain, &coin, "fc_rules", json!({})).await?;
    let launch_block = rules["launch_block"].as_u64().context("launch_block")?;
    let early_blocks = rules["early_blocks"].as_u64().context("early_blocks")?;
    let storage_min =
        amount(&view(chain, &coin, "storage_balance_bounds", json!({})).await?["min"])?;
    Ok((
        coin,
        launched,
        rules,
        launch_block,
        early_blocks,
        storage_min,
    ))
}

/// tokens_out of the one fc_buy in a raw result, with no RPC read.
fn bought(result: &TransactionResult) -> Result<u128> {
    let found = fair_chance_sandbox_tests::events(result, "fair_chance", "fc_buy");
    amount(&found.first().context("no fc_buy")?.event["data"][0]["tokens_out"])
}

async fn scene(chain: &Chain) -> Result<Value> {
    let mut actions: Vec<Value> = Vec::new();
    let mut checks: Vec<Value> = Vec::new();

    // Accounts first, before the launch, so the early window holds only trades.
    let mut snipers = Vec::new();
    for i in 1..=SNIPERS {
        snipers.push(sub_account(chain, &chain.cabal, &format!("s{i:02}")).await?);
    }
    let mut buyers = Vec::new();
    for i in 1..=BUYERS {
        let a = sub_account(chain, &chain.funder2, &format!("b{i}")).await?;
        actions.push(json!({ "step": "fund", "role": format!("buyer #{i}"), "account": a.id, "funded_by": chain.funder2.id, "near": ACCOUNT_NEAR.as_yoctonear().to_string() }));
        buyers.push(a);
    }

    // 1. Launch with the opening buy.
    let (coin, launched, rules, launch_block, early_blocks, storage_min) = launch_coin(
        chain,
        "cabal",
        "Cabal Test",
        "You buy. The cabal dumps on you. You hold the bag.",
    )
    .await?;
    let head_after_launch = head(chain).await? - launch_block;

    // 2. The snipers buy at once: one transaction each, sent together at a fixed head.
    wait_head(chain, launch_block + SNIPE_AT).await?;
    // 3. Each sniper tries to sell its whole bag inside the early window. The sells go out as
    // soon as the optimistic head shows every bag, without waiting for the buys to be final.
    let buying = join_all(
        snipers
            .iter()
            .map(|s| register_and_buy(chain, s, &coin, storage_min, SNIPER_BUY))
            .collect(),
    );
    let selling = async {
        let bags = loop {
            let mut bags = Vec::new();
            for s in &snipers {
                bags.push(balance(chain, &coin, &s.id).await.unwrap_or(0));
            }
            if bags.iter().all(|b| *b > 0) {
                break bags;
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        };
        let pool = view(chain, &coin, "fc_pool", json!({})).await;
        let sent = join_all(
            snipers
                .iter()
                .zip(&bags)
                .map(|(s, bag)| sell(chain, s, &coin, *bag))
                .collect(),
        )
        .await;
        (bags, pool, sent)
    };
    let (buys, (seen_bags, pool_before, sells)) = tokio::join!(buying, selling);
    let pool_before = pool_before?;
    let buys: Vec<TransactionResult> = buys.into_iter().collect::<Result<_>>()?;
    let bags: Vec<u128> = buys.iter().map(bought).collect::<Result<_>>()?;
    if seen_bags != bags {
        bail!("a bag seen before the early sells differs from its fc_buy tokens_out");
    }
    let sells: Vec<TransactionResult> = sells.into_iter().collect::<Result<_>>()?;

    // Number the snipers by buy order: the buys of one block run one after another, and each
    // later buy gets fewer tokens. The block's order can change between runs; the amounts do not.
    let mut order: Vec<usize> = (0..SNIPERS).collect();
    order.sort_by(|&x, &y| bags[y].cmp(&bags[x]));
    let snipers: Vec<Account> = order.iter().map(|&k| snipers[k].clone()).collect();
    let buys: Vec<TransactionResult> = order.iter().map(|&k| buys[k].clone()).collect();
    let sells: Vec<TransactionResult> = order.iter().map(|&k| sells[k].clone()).collect();
    let bags: Vec<u128> = order.iter().map(|&k| bags[k]).collect();

    actions.push(json!({ "step": "launch", "role": "creator", "account": chain.creator.id,
        "opening_buy_near": OPENING_BUY.as_yoctonear().to_string(), "head_after_launch_returned": head_after_launch,
        "tx": record(chain, &launched).await? }));
    for (i, s) in snipers.iter().enumerate() {
        actions.push(json!({ "step": "fund", "role": format!("sniper #{}", i + 1), "account": s.id, "funded_by": chain.cabal.id, "near": ACCOUNT_NEAR.as_yoctonear().to_string() }));
    }
    for (i, (s, r)) in snipers.iter().zip(&buys).enumerate() {
        let rec = record(chain, r).await?;
        let buy = fair_event(&rec, "fc_buy")
            .context("sniper buy has no fc_buy")?
            .clone();
        let b = buy["block"].as_u64().unwrap_or(0) - launch_block;
        checks.push(
            json!({ "check": format!("sniper #{} buy in blocks 0 to N and marked", i + 1),
            "pass": b <= early_blocks && buy["marked_tokens"] == buy["tokens_out"] }),
        );
        actions.push(json!({ "step": "sniper buy", "role": format!("sniper #{}", i + 1), "account": s.id,
            "sent_at_block_after_launch": SNIPE_AT, "block_after_launch": b, "near_in": SNIPER_BUY.as_yoctonear().to_string(), "tx": rec }));
    }
    for (i, (s, r)) in snipers.iter().zip(&sells).enumerate() {
        let rec = record(chain, r).await?;
        let fb = rec["refusal"]["failed_receipt_block"].as_u64();
        let after = balance(chain, &coin, &s.id).await?;
        let ok = !rec["success"].as_bool().unwrap_or(true)
            && rec["refusal"]["code"] == "FC_EARLY_WINDOW"
            && after == bags[i]
            && fb
                .map(|h| h - launch_block <= early_blocks)
                .unwrap_or(false);
        checks.push(json!({ "check": format!("sniper #{} early sell refused with FC_EARLY_WINDOW in blocks 0 to N, no tokens moved", i + 1), "pass": ok }));
        actions.push(json!({ "step": "sniper sell in early window", "role": format!("sniper #{}", i + 1), "account": s.id,
            "tokens_asked": bags[i].to_string(), "block_after_launch": fb.map(|h| h - launch_block),
            "balance_after": after.to_string(), "tx": rec }));
    }
    let pool_after = view(chain, &coin, "fc_pool", json!({})).await?;
    checks.push(json!({ "check": "pool unchanged by the refused sells",
        "pass": pool_before["token_reserve"] == pool_after["token_reserve"] && pool_before["real_near"] == pool_after["real_near"] }));
    let early_view = view(chain, &coin, "fc_early_window", json!({})).await?;

    // Wait until block N + 1 is final.
    wait_for_final_height(&chain.network, launch_block + early_blocks + 1).await?;
    let mut snapshots = vec![
        json!({ "label": "in the early window, after the refused sells", "fc_early_window": early_view, "fc_pool": pool_after }),
    ];

    // 4. Ordinary buyers buy after the window.
    for (i, b) in buyers.iter().enumerate() {
        let r = register_and_buy(chain, b, &coin, storage_min, BUYER_BUY).await?;
        let rec = record(chain, &r).await?;
        let buy = fair_event(&rec, "fc_buy").context("buyer fc_buy")?.clone();
        actions.push(
            json!({ "step": "buyer buy", "role": format!("buyer #{}", i + 1), "account": b.id,
            "block_after_launch": buy["block"].as_u64().unwrap_or(0) - launch_block,
            "near_in": BUYER_BUY.as_yoctonear().to_string(), "tx": rec }),
        );
    }
    let mut accounts_before = Vec::new();
    for (i, s) in snipers.iter().enumerate() {
        accounts_before.push(json!({ "role": format!("sniper #{}", i + 1),
            "fc_account": view(chain, &coin, "fc_account", json!({ "account_id": s.id })).await? }));
    }
    snapshots.push(snapshot(chain, &coin, "before the snipers' dump").await?);

    // 5. The snipers dump, one after another, in order.
    for (i, s) in snipers.iter().enumerate() {
        let bag = balance(chain, &coin, &s.id).await?;
        let quote = view(
            chain,
            &coin,
            "fc_quote_sell",
            json!({ "account_id": s.id, "amount": bag.to_string() }),
        )
        .await?;
        let quote_block = head(chain).await?;
        let r = sell(chain, s, &coin, bag).await?;
        let rec = record(chain, &r).await?;
        let ev = fair_event(&rec, "fc_sell")
            .context("sniper fc_sell")?
            .clone();
        let matches = ev["tokens_sold"] == quote["tokens_filled"]
            && ev["near_out"] == quote["near_out"]
            && ev["fill"] == quote["fill"]
            && ev["limit_reason"] == quote["limit_reason"];
        checks.push(json!({ "check": format!("sniper #{} sell matches fc_quote_sell before it", i + 1), "pass": matches }));
        let after = snapshot(chain, &coin, &format!("after sniper #{} sell", i + 1)).await?;
        actions.push(json!({ "step": "sniper dump", "role": format!("sniper #{}", i + 1), "account": s.id,
            "tokens_asked": bag.to_string(), "fc_quote_sell_before": quote, "quote_block": quote_block,
            "tx": rec, "fc_sell": ev, "after": after }));
    }
    snapshots.push(snapshot(chain, &coin, "after the snipers' dump").await?);

    // 6. The ordinary buyers sell their whole bags.
    for (i, b) in buyers.iter().enumerate() {
        let bag = balance(chain, &coin, &b.id).await?;
        let quote = view(
            chain,
            &coin,
            "fc_quote_sell",
            json!({ "account_id": b.id, "amount": bag.to_string() }),
        )
        .await?;
        let r = sell(chain, b, &coin, bag).await?;
        let rec = record(chain, &r).await?;
        let ev = fair_event(&rec, "fc_sell")
            .context("buyer fc_sell")?
            .clone();
        let matches =
            ev["tokens_sold"] == quote["tokens_filled"] && ev["near_out"] == quote["near_out"];
        checks.push(json!({ "check": format!("buyer #{} sell matches fc_quote_sell before it", i + 1), "pass": matches }));
        let after = snapshot(chain, &coin, &format!("after buyer #{} sell", i + 1)).await?;
        actions.push(json!({ "step": "buyer sell", "role": format!("buyer #{}", i + 1), "account": b.id,
            "tokens_asked": bag.to_string(), "fc_quote_sell_before": quote, "tx": rec, "fc_sell": ev, "after": after }));
    }
    // 7. The creator sells its whole bag: with the snipers, all early supply uses the shared allowance.
    if CREATOR_SELLS {
        let who = &chain.creator;
        let bag = balance(chain, &coin, &who.id).await?;
        let quote = view(
            chain,
            &coin,
            "fc_quote_sell",
            json!({ "account_id": who.id, "amount": bag.to_string() }),
        )
        .await?;
        let r = sell(chain, who, &coin, bag).await?;
        let rec = record(chain, &r).await?;
        let ev = fair_event(&rec, "fc_sell")
            .context("creator fc_sell")?
            .clone();
        let matches =
            ev["tokens_sold"] == quote["tokens_filled"] && ev["near_out"] == quote["near_out"];
        checks.push(
            json!({ "check": "creator sell matches fc_quote_sell before it", "pass": matches }),
        );
        actions.push(json!({ "step": "creator sell", "role": "creator", "account": who.id,
            "tokens_asked": bag.to_string(), "fc_quote_sell_before": quote, "tx": rec, "fc_sell": ev }));
    }
    let end = snapshot(chain, &coin, "end").await?;
    if CREATOR_SELLS {
        // Each holder's part rounds down, so at most one base unit per marked holder stays.
        let left = amount(&end["fc_early_window"]["shared_allowance_left"])?;
        let holders = (SNIPERS + 1) as u128;
        checks.push(json!({ "check": "shared allowance left ends at 0 (at most one base unit of rounding per marked holder)",
            "pass": left <= holders }));
    }
    snapshots.push(end);

    Ok(json!({
        "coin": coin, "launch_block": launch_block, "early_blocks": early_blocks,
        "fc_rules": rules, "coin_code_hash": chain.coin_code_hash.to_string(),
        "sizes": { "opening_buy": OPENING_BUY.as_yoctonear().to_string(), "snipers": SNIPERS,
            "sniper_buy": SNIPER_BUY.as_yoctonear().to_string(), "buyers": BUYERS, "buyer_buy": BUYER_BUY.as_yoctonear().to_string(),
            "creator_sells": CREATOR_SELLS },
        "actions": actions, "snapshots": snapshots, "sniper_accounts_before_dump": accounts_before, "checks": checks,
    }))
}

/// One early wallet and one late wallet make the same buy and sell their whole bag after the
/// window, on a second coin with the same opening buy.
async fn early_vs_late(chain: &Chain) -> Result<Value> {
    let early = sub_account(chain, &chain.cabal, "early").await?;
    let late = sub_account(chain, &chain.funder2, "late").await?;
    let (coin, launched, _rules, launch_block, early_blocks, storage_min) = launch_coin(
        chain,
        "twin",
        "Twin Test",
        "One early wallet and one late wallet.",
    )
    .await?;
    wait_head(chain, launch_block + SNIPE_AT).await?;
    let r = register_and_buy(chain, &early, &coin, storage_min, SNIPER_BUY).await?;
    let early_buy = record(chain, &r).await?;
    let launch = json!({ "launch": record(chain, &launched).await? });
    wait_for_final_height(&chain.network, launch_block + early_blocks + 1).await?;
    let r = register_and_buy(chain, &late, &coin, storage_min, SNIPER_BUY).await?;
    let late_buy = record(chain, &r).await?;
    let mut sells = Vec::new();
    for (role, who) in [("early wallet", &early), ("late wallet", &late)] {
        let bag = balance(chain, &coin, &who.id).await?;
        let quote = view(
            chain,
            &coin,
            "fc_quote_sell",
            json!({ "account_id": who.id, "amount": bag.to_string() }),
        )
        .await?;
        let r = sell(chain, who, &coin, bag).await?;
        let rec = record(chain, &r).await?;
        let ev = fair_event(&rec, "fc_sell").context("fc_sell")?.clone();
        sells.push(json!({ "role": role, "account": who.id, "tokens_asked": bag.to_string(), "fc_quote_sell_before": quote, "fc_sell": ev, "tx": rec }));
    }
    Ok(
        json!({ "coin": coin, "launch_block": launch_block, "launch": launch["launch"],
        "early_buy": early_buy, "late_buy": late_buy, "sells": sells,
        "end": snapshot(chain, &coin, "end").await? }),
    )
}

async fn join_all<F: std::future::Future>(futs: Vec<F>) -> Vec<F::Output> {
    let mut pinned: Vec<std::pin::Pin<Box<F>>> = futs.into_iter().map(Box::pin).collect();
    let mut outs: Vec<Option<F::Output>> = (0..pinned.len()).map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut done = true;
        for (i, f) in pinned.iter_mut().enumerate() {
            if outs[i].is_none() {
                match f.as_mut().poll(cx) {
                    std::task::Poll::Ready(v) => outs[i] = Some(v),
                    std::task::Poll::Pending => done = false,
                }
            }
        }
        if done {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
    outs.into_iter().map(|o| o.unwrap()).collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: sniper-scene <out.json>");
        std::process::exit(2);
    }
    let started = std::time::Instant::now();
    let chain = start().await?;
    let scene = scene(&chain).await?;
    let twin = early_vs_late(&chain).await?;
    let out = json!({
        "network": "local near-sandbox (fresh, throwaway keys)",
        "sandbox_release": fair_chance_sandbox_tests::SANDBOX_RELEASE,
        "scene": scene,
        "early_vs_late": twin,
        "seconds": started.elapsed().as_secs(),
    });
    std::fs::write(&args[1], serde_json::to_string_pretty(&out)?)?;
    let checks = out["scene"]["checks"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let failed: Vec<_> = checks.iter().filter(|c| c["pass"] != true).collect();
    for c in &checks {
        println!(
            "{} {}",
            if c["pass"] == true { "PASS" } else { "FAIL" },
            c["check"].as_str().unwrap_or("")
        );
    }
    println!(
        "sniper-scene: wrote {} in {} s",
        args[1],
        started.elapsed().as_secs()
    );
    if !failed.is_empty() {
        eprintln!(
            "sniper-scene: FAIL, {} check(s) failed; no numbers come from this run",
            failed.len()
        );
        std::process::exit(1);
    }
    Ok(())
}
