//! ADR 0010 on a real chain: the creator claims its fee share after the pool held 10 real NEAR
//! for 24 h. The sandbox fast-forwards the block time. The claim needs exactly 1 yoctoNEAR and
//! the creator account; `fc_creator_claim` is logged in the callback, after the transfer. When
//! the transfer fails (the creator account was deleted before it arrived), the callback puts the
//! amount back in the creator balance and logs nothing.
//!
//! The steps print as they pass (`--nocapture`), so the run can be recorded.

use anyhow::{bail, Context, Result};
use fair_chance_sandbox_tests::{events, Account, World};
use near_api::types::transaction::result::TransactionResult;
use near_api::types::{AccessKeyPermission, NearToken, TxExecutionStatus};
use near_api::{Contract, Signer};
use serde_json::{json, Value};

fn amount(value: &Value) -> Result<u128> {
    Ok(value.as_str().context("amount string")?.parse()?)
}

async fn fees(world: &World) -> Result<Value> {
    world.view(&world.coin, "fc_creator_fees", json!({})).await
}

async fn real_near(world: &World) -> Result<u128> {
    amount(&world.view(&world.coin, "fc_pool", json!({})).await?["real_near"])
}

fn step(text: &str) {
    println!("PASS {text}");
}

/// Fast-forwards the sandbox until `fc_creator_fees` says the creator can claim.
async fn fast_forward_until_claimable(world: &World) -> Result<()> {
    for _ in 0..10 {
        if fees(world).await?["claimable_now"] == true {
            return Ok(());
        }
        world.sandbox.fast_forward(FAST_FORWARD_BLOCKS).await?;
    }
    bail!("the claim never became claimable: {}", fees(world).await?)
}

/// Blocks per fast-forward call: about 12 h of block time on a [`World::launch_for_fast_forward`]
/// sandbox (`fast_forward_block_time_on_sandbox.rs`).
const FAST_FORWARD_BLOCKS: u64 = 500;

async fn claim(world: &World, who: &Account, deposit: NearToken) -> Result<TransactionResult> {
    Ok(Contract(world.coin.clone())
        .call_function("claim_creator_fees", json!({}))
        .transaction()
        .deposit(deposit)
        .gas(near_api::types::NearGas::from_tgas(100))
        .with_signer(who.id.clone(), who.signer.clone())
        .send_to(&world.network)
        .await?)
}

#[tokio::test]
async fn the_creator_claims_after_24_h_of_depth_and_a_failed_transfer_restores_the_balance(
) -> Result<()> {
    let world = World::launch_for_fast_forward().await?;
    let coin = &world.coin;
    let _ = world.register(&world.alice, coin).await?;
    let _ = world
        .call(
            &world.alice,
            coin,
            "buy",
            json!({ "min_tokens_out": "0" }),
            NearToken::from_near(11),
        )
        .await?;
    let balance = amount(&fees(&world).await?["balance"])?;
    // 30% of the 1% fee on 11 NEAR.
    assert_eq!(balance, 33_000_000_000_000_000_000_000);
    assert!(real_near(&world).await? >= 10 * 10u128.pow(24));
    step("alice buys 11 NEAR: 10.967 real NEAR, creator balance 0.033 NEAR");

    let no_yocto = world
        .call_refused(
            &world.creator,
            coin,
            "claim_creator_fees",
            json!({}),
            NearToken::from_yoctonear(0),
        )
        .await?;
    assert!(no_yocto.contains("FC_ONE_YOCTO"), "{no_yocto}");
    let not_creator = world
        .call_refused(
            &world.alice,
            coin,
            "claim_creator_fees",
            json!({}),
            NearToken::from_yoctonear(1),
        )
        .await?;
    assert!(not_creator.contains("FC_NOT_CREATOR"), "{not_creator}");
    let too_early = world
        .call_refused(
            &world.creator,
            coin,
            "claim_creator_fees",
            json!({}),
            NearToken::from_yoctonear(1),
        )
        .await?;
    assert!(too_early.contains("FC_CLAIM_NOT_READY"), "{too_early}");
    let callback = world
        .call_refused(
            &world.creator,
            coin,
            "on_creator_claim",
            json!({ "amount": balance.to_string(), "depth_ok_since_ns": "0" }),
            NearToken::from_yoctonear(0),
        )
        .await?;
    assert!(callback.contains("is private"), "{callback}");
    step("refused: no 1 yocto (FC_ONE_YOCTO), not the creator (FC_NOT_CREATOR), before 24 h (FC_CLAIM_NOT_READY), callback from outside");

    // Window 0 starts its lowest real NEAR at 0, so the 24 h count starts at window 1.
    assert_eq!(fees(&world).await?["depth_ok_since_ns"], Value::Null);
    fast_forward_until_claimable(&world).await?;
    let counting = fees(&world).await?;
    let since: u64 = counting["depth_ok_since_ns"]
        .as_str()
        .context("depth_ok_since_ns")?
        .parse()?;
    let rules = world.view(coin, "fc_rules", json!({})).await?;
    let launch_ns: u64 = rules["launch_ns"].as_str().context("launch_ns")?.parse()?;
    assert_eq!(since, launch_ns + 3_600_000_000_000);
    step(&format!(
        "fast-forward: depth_ok_since_ns = launch + 1 h ({since}); 24 h later the claim is ready"
    ));

    let real_before = real_near(&world).await?;
    let creator_before = world.balance(&world.creator.id).await?.as_yoctonear();
    let claimed = claim(&world, &world.creator, NearToken::from_yoctonear(1)).await?;
    assert!(claimed.is_success(), "{claimed:?}");
    let logged = events(&claimed, "fair_chance", "fc_creator_claim");
    assert_eq!(logged.len(), 1);
    let data = &logged[0].event["data"][0];
    assert_eq!(data["creator"], world.creator.id.to_string());
    assert_eq!(amount(&data["amount"])?, balance);
    assert_eq!(data["depth_ok_since_ns"], counting["depth_ok_since_ns"]);
    let after = fees(&world).await?;
    assert_eq!(after["balance"], "0");
    assert_eq!(amount(&after["claimed_total"])?, balance);
    assert_eq!(real_near(&world).await?, real_before);
    let creator_after = world.balance(&world.creator.id).await?.as_yoctonear();
    let gas = World::tokens_burnt(&claimed)?;
    assert_eq!(creator_after + gas + 1, creator_before + balance);
    step("the creator claims 0.033 NEAR; fc_creator_claim logged after the transfer; real NEAR unchanged");

    // A second balance, then a claim whose transfer fails: the creator deletes its account with
    // a second key right after the claim transaction is in a block, before the payout arrives.
    let _ = world
        .call(
            &world.alice,
            coin,
            "buy",
            json!({ "min_tokens_out": "0" }),
            NearToken::from_near(1),
        )
        .await?;
    let second = amount(&fees(&world).await?["balance"])?;
    assert_eq!(second, 3_000_000_000_000_000_000_000);
    let delete_key = near_api::signer::generate_secret_key()?;
    let delete_signer = Signer::from_secret_key(delete_key.clone())?;
    near_api::Account(world.creator.id.clone())
        .add_key(AccessKeyPermission::FullAccess, delete_key.public_key())
        .with_signer(world.creator.signer.clone())
        .send_to(&world.network)
        .await?
        .assert_success();
    world
        .wait_for_height(
            near_api::Chain::block_number()
                .fetch_from(&world.network)
                .await?
                + 3,
        )
        .await?;

    let claim_tx = Contract(world.coin.clone())
        .call_function("claim_creator_fees", json!({}))
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .gas(near_api::types::NearGas::from_tgas(100))
        .with_signer(world.creator.id.clone(), world.creator.signer.clone())
        .presign_with(&world.network)
        .await?;
    let claim_hash = match &claim_tx.transaction {
        near_api::advanced::TransactionableOrSigned::Signed((signed, _)) => signed.get_hash(),
        _ => bail!("the claim is not signed"),
    };
    let delete_tx = near_api::Account(world.creator.id.clone())
        .delete_account_with_beneficiary(world.bob.id.clone())
        .with_signer(delete_signer)
        .presign_with(&world.network)
        .await?;
    let _ = claim_tx
        .wait_until(TxExecutionStatus::Included)
        .send_to(&world.network)
        .await?;
    delete_tx.send_to(&world.network).await?.assert_success();
    let failed = near_api::Transaction::status_with_options(
        world.creator.id.clone(),
        claim_hash,
        TxExecutionStatus::Final,
    )
    .fetch_from(&world.network)
    .await?;
    assert!(world.contract_state(&world.creator.id).await?.is_none());
    assert!(
        events(
            &TransactionResult::Full(Box::new(failed)),
            "fair_chance",
            "fc_creator_claim"
        )
        .is_empty(),
        "fc_creator_claim logged for a failed transfer"
    );
    let restored = fees(&world).await?;
    assert_eq!(amount(&restored["balance"])?, second);
    assert_eq!(amount(&restored["claimed_total"])?, balance);
    step("the creator account is gone before the payout: the transfer fails, the callback restores 0.003 NEAR, no fc_creator_claim");
    Ok(())
}
