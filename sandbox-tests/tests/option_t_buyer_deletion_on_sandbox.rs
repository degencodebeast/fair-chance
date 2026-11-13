//! The Option T delete step on sandbox: the buyer sub-accounts are deleted with their NEAR going
//! back to the funder, and the creator account stays.

use anyhow::Result;
use fair_chance_sandbox_tests::option_t::{delete_buyers, make_accounts, OptionTSetup};
use fair_chance_sandbox_tests::World;
use near_api::types::NearToken;

#[tokio::test]
async fn the_delete_step_removes_only_the_buyers_and_returns_their_near_to_the_funder() -> Result<()> {
    let world = World::launch().await?;
    let setup = OptionTSetup::sandbox_defaults(&world, "gone", 2);
    let (creator, buyers) = make_accounts(&setup).await?;
    let before = world.balance(&world.bob.id).await?;

    let deleted = delete_buyers(&setup).await?;

    let ids: Vec<_> = buyers.iter().map(|b| b.id.clone()).collect();
    assert_eq!(deleted, ids);
    for buyer in &ids {
        assert_eq!(world.contract_state(buyer).await?, None, "{buyer} still exists");
    }
    assert!(world.contract_state(&creator.id).await?.is_some(), "the creator was deleted");
    let back = world.balance(&world.bob.id).await?.saturating_sub(before);
    assert!(
        back > NearToken::from_near(2),
        "the funder got back {back}, less than the buyers held"
    );
    Ok(())
}
