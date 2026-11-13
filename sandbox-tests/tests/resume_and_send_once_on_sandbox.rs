//! On sandbox: a rerun reuses the accounts the first run made and creates only the missing ones;
//! a bad existing account stops the survey before anything is signed; and a transaction whose
//! send gets no answer is looked up by its hash and never sent again.

use anyhow::Result;
use fair_chance_sandbox_tests::option_t::{make_accounts, OptionTSetup};
use fair_chance_sandbox_tests::{send_once, World};
use near_api::types::NearToken;
use near_api::NetworkConfig;

#[tokio::test]
async fn a_rerun_reuses_the_made_accounts_and_creates_only_the_missing_ones() -> Result<()> {
    let world = World::launch().await?;
    let first = OptionTSetup::sandbox_defaults(&world, "resume", 2);
    let (_, made) = make_accounts(&first).await?;
    let held_before = world.balance(&made[0].id).await?;
    let funder_before = world.balance(&world.bob.id).await?;

    let rerun = OptionTSetup::sandbox_defaults(&world, "resume", 4);
    let (_, buyers) = make_accounts(&rerun).await?;

    assert_eq!(buyers.len(), 4);
    assert_eq!(
        world.balance(&made[0].id).await?,
        held_before,
        "a reused buyer was funded again"
    );
    let paid = funder_before.saturating_sub(world.balance(&world.bob.id).await?);
    let two_buyers = NearToken::from_millinear(3000);
    assert!(
        paid >= two_buyers && paid < two_buyers.saturating_add(NearToken::from_millinear(100)),
        "the rerun paid {paid}, not two new buyers"
    );
    Ok(())
}

#[tokio::test]
async fn an_existing_account_with_less_than_planned_stops_before_any_signing() -> Result<()> {
    let world = World::launch().await?;
    let mut small = OptionTSetup::sandbox_defaults(&world, "short", 1);
    small.buy_near = NearToken::from_millinear(100);
    make_accounts(&small).await?;
    let funder_before = world.balance(&world.bob.id).await?;

    // The same names with the default 1.5 NEAR plan: buyer 01 holds 0.6 NEAR.
    let error = make_accounts(&OptionTSetup::sandbox_defaults(&world, "short", 3))
        .await
        .err()
        .expect("the survey did not stop")
        .to_string();
    assert!(error.contains("short-buyer-01"), "{error}");
    assert_eq!(
        world.balance(&world.bob.id).await?,
        funder_before,
        "something was signed after the survey stopped"
    );
    Ok(())
}

#[tokio::test]
async fn a_send_with_no_answer_is_looked_up_by_hash_and_never_sent_again() -> Result<()> {
    let world = World::launch().await?;
    let nowhere = NetworkConfig::from_rpc_url("nowhere", "http://127.0.0.1:1".parse()?);
    let before = world.balance(&world.bob.id).await?;
    let transfer = near_api::Tokens::account(world.bob.id.clone())
        .send_to(world.alice.id.clone())
        .near(NearToken::from_near(1))
        .with_signer(world.bob.signer.clone());

    let error = send_once(transfer, &world.bob.id, &world.network, &nowhere)
        .await
        .unwrap_err()
        .to_string();

    assert!(
        error.contains("not found") && error.contains("not sent again"),
        "{error}"
    );
    assert_eq!(
        world.balance(&world.bob.id).await?,
        before,
        "the transfer went out"
    );
    Ok(())
}
