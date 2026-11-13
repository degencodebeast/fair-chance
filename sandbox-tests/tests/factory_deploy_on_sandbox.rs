//! The factory deploy step on sandbox, with the same code path the testnet run uses: the coin goes
//! into global storage by code hash, the factory sub-account is made, deployed and started in one
//! transaction, a second run refuses before it signs, and the factory launches a coin whose
//! metadata keeps the ticker as given.

use anyhow::Result;
use fair_chance_sandbox_tests::factory_deploy::{check_deployed, deploy, DeployPlan};
use fair_chance_sandbox_tests::{
    launch_args, sandbox_with_accounts, transact, view_on, wasm, ContractState, LAUNCH_DEPOSIT,
};
use near_api::types::{AccountId, NearToken};
use serde_json::json;

#[tokio::test]
async fn the_deploy_step_makes_a_working_factory_and_costs_what_the_plan_says() -> Result<()> {
    let (_sandbox, network, accounts) = sandbox_with_accounts(&["deployer.sandbox"]).await?;
    let funder = &accounts[0];
    let plan = DeployPlan::new(
        funder.id.clone(),
        wasm("fair_chance_coin")?,
        wasm("fair_chance_factory")?,
    )?;
    let before = balance(&network, &funder.id).await?;

    let report = deploy(&plan, &network, funder, None).await?;

    assert!(
        report.global_deploy_tx.is_some(),
        "the coin code was not new"
    );
    let paid = before.saturating_sub(balance(&network, &funder.id).await?);
    let gas_margin = NearToken::from_millinear(100);
    assert!(
        paid >= plan.total() && paid <= plan.total().saturating_add(gas_margin),
        "the funder paid {paid}, the plan said {} plus gas",
        plan.total()
    );

    let factory = near_api::Account(plan.factory.clone())
        .view()
        .fetch_from(&network)
        .await?
        .data;
    assert_eq!(
        factory.contract_state,
        ContractState::LocalHash(plan.factory_code_hash)
    );
    let recorded = view_on(&network, &plan.factory, "coin_code_hash", json!({})).await?;
    assert_eq!(recorded, json!(plan.coin_code_hash.to_string()));

    // The post-deploy read the testnet run uses: factory code, recorded coin hash, and the global
    // code found under its hash on a real node.
    let checked = check_deployed(&plan, &network).await?;
    assert_eq!(checked.len(), 3, "{checked:?}");

    // A second run finds the factory account and refuses before it signs anything.
    let again = before_second_run(&network, funder, &plan).await;
    assert!(again.is_err(), "a second deploy was not refused");

    // The factory launches a coin; the symbol keeps the ticker as given.
    let launched = transact(
        &network,
        funder,
        &plan.factory,
        "launch",
        launch_args("Bag Holder", "BAG", None, Some("A test coin."), None),
        LAUNCH_DEPOSIT,
    )
    .await?;
    assert!(launched.is_success(), "{launched:?}");
    let coin: AccountId = format!("bag.{}", plan.factory).parse()?;
    let metadata = view_on(&network, &coin, "ft_metadata", json!({})).await?;
    assert_eq!(metadata["symbol"], "BAG");
    assert_eq!(metadata["name"], "Bag Holder");
    let rules = view_on(&network, &coin, "fc_rules", json!({})).await?;
    assert_eq!(rules["code_hash"], json!(plan.coin_code_hash.to_string()));
    Ok(())
}

async fn before_second_run(
    network: &near_api::NetworkConfig,
    funder: &fair_chance_sandbox_tests::Account,
    plan: &DeployPlan,
) -> Result<()> {
    let before = balance(network, &funder.id).await?;
    let result = deploy(plan, network, funder, None).await.map(|_| ());
    let after = balance(network, &funder.id).await?;
    assert_eq!(before, after, "the refused second run spent NEAR");
    result
}

async fn balance(network: &near_api::NetworkConfig, id: &AccountId) -> Result<NearToken> {
    Ok(near_api::Account(id.clone())
        .view()
        .fetch_from(network)
        .await?
        .data
        .amount)
}
