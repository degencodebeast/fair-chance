//! On sandbox: the sells-only step sells the full balance of buyers 01 to 18, one after another in
//! that order, writes the plan file before the first signature, and leaves buyers 19 and 20 alone.
//! The return step then sends the NEAR of buyers 01 to 18, minus the keep, back to the funder, once.

use anyhow::Result;
use fair_chance_sandbox_tests::option_t::{make_accounts, OptionTSetup};
use fair_chance_sandbox_tests::pass_list::PassList;
use fair_chance_sandbox_tests::sells_only::{run_returns, run_sells, RETURN_KEEP_YOCTO};
use fair_chance_sandbox_tests::{batch_once, launch_args, view_on, FunctionCall, World};
use near_api::types::{NearGas, NearToken};
use serde_json::json;

fn plan_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("fc-sells-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plan.json");
    let _ = std::fs::remove_file(&path);
    path
}

async fn balance_of(
    setup: &OptionTSetup,
    coin: &near_api::types::AccountId,
    who: &near_api::types::AccountId,
) -> Result<u128> {
    let v = view_on(
        &setup.network,
        coin,
        "ft_balance_of",
        json!({ "account_id": who }),
    )
    .await?;
    Ok(v.as_str().unwrap().parse()?)
}

/// Launch BAG and let all 20 buyers buy 1 NEAR, then wait for the end of the early window.
async fn launched_bag(world: &World) -> Result<(OptionTSetup, near_api::types::AccountId)> {
    let setup = OptionTSetup::sandbox_defaults(world, "BAG", 20);
    let (creator, buyers) = make_accounts(&setup).await?;
    let launch = world
        .try_launch(
            &creator,
            launch_args("Bag Holder", "BAG", None, Some("sandbox"), None),
            setup.launch_deposit,
        )
        .await?;
    assert!(launch.is_success(), "{launch:?}");
    let coin = world.coin_for("bag")?;
    let bounds = view_on(&setup.network, &coin, "storage_balance_bounds", json!({})).await?;
    let storage: u128 = bounds["min"].as_str().unwrap().parse()?;
    for buyer in &buyers {
        let calls = vec![
            FunctionCall::new(
                "storage_deposit",
                json!({}),
                NearToken::from_yoctonear(storage),
            ),
            FunctionCall::new(
                "buy",
                json!({ "min_tokens_out": "0" }),
                NearToken::from_near(1),
            ),
        ];
        let bought = batch_once(
            &setup.network,
            &setup.send_network,
            buyer,
            &coin,
            calls,
            NearGas::from_tgas(300),
        )
        .await?;
        assert!(bought.is_success(), "{bought:?}");
    }
    world.wait_until_after_the_early_window(&coin).await?;
    Ok((setup, coin))
}

#[tokio::test]
async fn buyers_01_to_18_sell_their_full_balance_in_order_and_19_and_20_keep() -> Result<()> {
    let world = World::launch().await?;
    let (setup, coin) = launched_bag(&world).await?;
    let (sellers, keepers) =
        fair_chance_sandbox_tests::sells_only::fixed_accounts("BAG", &setup.funder.id)?;
    let mut before = Vec::new();
    for a in sellers.iter().chain(&keepers) {
        before.push(balance_of(&setup, &coin, a).await?);
    }
    let path = plan_path("run");

    let sells = run_sells(&setup, &path, &mut PassList::default()).await?;

    let plan: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    assert_eq!(plan["sells"].as_array().unwrap().len(), 18);
    assert_eq!(sells.len(), 18);
    for (i, sell) in sells.iter().enumerate() {
        assert_eq!(sell.account, sellers[i], "sell {} out of order", i + 1);
        assert_eq!(
            sell.tokens_asked,
            before[i],
            "sell {} was not the full balance",
            i + 1
        );
        assert_eq!(plan["sells"][i]["amount"], before[i].to_string());
        let after = balance_of(&setup, &coin, &sellers[i]).await?;
        assert_eq!(after, before[i] - sell.tokens_sold, "{}", sellers[i]);
    }
    for (k, a) in keepers.iter().enumerate() {
        assert_eq!(
            balance_of(&setup, &coin, a).await?,
            before[18 + k],
            "{a} sold"
        );
        assert!(before[18 + k] > 0);
    }
    Ok(())
}

#[tokio::test]
async fn an_existing_plan_file_stops_the_step_before_any_signature() -> Result<()> {
    let world = World::launch().await?;
    let (setup, coin) = launched_bag(&world).await?;
    let path = plan_path("exists");
    std::fs::write(&path, "{}")?;
    let first =
        &fair_chance_sandbox_tests::sells_only::fixed_accounts("BAG", &setup.funder.id)?.0[0];
    let before = balance_of(&setup, &coin, first).await?;

    let error = run_sells(&setup, &path, &mut PassList::default())
        .await
        .unwrap_err();

    assert!(error.to_string().contains("nothing was signed"), "{error}");
    assert_eq!(balance_of(&setup, &coin, first).await?, before);
    assert_eq!(std::fs::read_to_string(&path)?, "{}");
    Ok(())
}

/// The sha256 of the plan file, as the operator passes it with --plan-sha256.
fn sha256_of(path: &std::path::Path) -> String {
    near_api::types::CryptoHash::hash(&std::fs::read(path).unwrap())
        .0
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

async fn near_of(setup: &OptionTSetup, who: &near_api::types::AccountId) -> Result<u128> {
    Ok(fair_chance_sandbox_tests::account_view(&setup.network, who)
        .await?
        .expect("account exists")
        .amount
        .as_yoctonear())
}

#[tokio::test]
async fn after_the_sells_buyers_01_to_18_return_their_near_once_and_19_and_20_are_untouched(
) -> Result<()> {
    let world = World::launch().await?;
    let (setup, coin) = launched_bag(&world).await?;
    let (sellers, keepers) =
        fair_chance_sandbox_tests::sells_only::fixed_accounts("BAG", &setup.funder.id)?;
    let path = plan_path("return");
    let missing = plan_path("return-missing");
    let error = run_returns(&setup, &missing, &"0".repeat(64), &mut PassList::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("nothing was signed"), "{error}");

    run_sells(&setup, &path, &mut PassList::default()).await?;
    let sha = sha256_of(&path);
    let mut kept_before = Vec::new();
    for k in &keepers {
        kept_before.push((
            near_of(&setup, k).await?,
            balance_of(&setup, &coin, k).await?,
        ));
    }
    let funder_before = near_of(&setup, &setup.funder.id).await?;

    let returns = run_returns(&setup, &path, &sha, &mut PassList::default()).await?;

    assert_eq!(returns.len(), 18);
    let mut sent = 0;
    for (i, r) in returns.iter().enumerate() {
        assert_eq!(r.account, sellers[i], "return {} out of order", i + 1);
        assert!(r.amount > 0);
        sent += r.amount;
        let left = near_of(&setup, &sellers[i]).await?;
        // The keep minus the transfer fee, which the seller pays.
        assert!(
            left <= RETURN_KEEP_YOCTO && left > RETURN_KEEP_YOCTO - RETURN_KEEP_YOCTO / 10,
            "{} holds {left}",
            sellers[i]
        );
    }
    assert_eq!(
        near_of(&setup, &setup.funder.id).await?,
        funder_before + sent
    );
    for (k, before) in keepers.iter().zip(&kept_before) {
        assert_eq!(
            (
                near_of(&setup, k).await?,
                balance_of(&setup, &coin, k).await?
            ),
            *before,
            "{k} was touched"
        );
    }

    // A second run finds every seller at the keep: it sends nothing.
    let again = run_returns(&setup, &path, &sha, &mut PassList::default()).await?;
    assert!(again.is_empty(), "{again:?}");
    assert_eq!(
        near_of(&setup, &setup.funder.id).await?,
        funder_before + sent
    );
    Ok(())
}

#[tokio::test]
async fn a_stopped_sells_run_continues_with_buyers_04_to_18_from_the_same_plan_file() -> Result<()>
{
    use fair_chance_sandbox_tests::sells_only::{
        fixed_accounts, plan_from_balances, plan_json, run_continue,
    };
    let world = World::launch_for_fast_forward().await?;
    let (setup, coin) = launched_bag(&world).await?;
    let (sellers, keepers) = fixed_accounts("BAG", &setup.funder.id)?;
    // On testnet the buys were hours before the sells. Here they are in the same 1-hour cap
    // window, and tokens bought in the window count toward its cap when sold, so the hand sells
    // would fill only in part. About 2.5 h of block time puts them in a later window.
    world.sandbox.fast_forward(100).await?;
    // The plan file a sells run wrote before it stopped after sell 3.
    let mut balances = Vec::new();
    for a in &sellers {
        balances.push((a.clone(), balance_of(&setup, &coin, a).await?));
    }
    let plan = plan_from_balances(&coin, "BAG", &setup.funder.id, &balances)?;
    let path = plan_path("continue");
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&plan_json(&plan))?),
    )?;
    let sha = sha256_of(&path);
    for step in &plan.steps[..3] {
        let seller = fair_chance_sandbox_tests::Account {
            id: step.account.clone(),
            signer: setup.funder.signer.clone(),
        };
        let sold = fair_chance_sandbox_tests::transact_once(
            &setup.network,
            &setup.send_network,
            &seller,
            &coin,
            "sell",
            json!({ "amount": step.amount.to_string(), "min_near_out": "0" }),
            NearToken::from_yoctonear(1),
            NearGas::from_tgas(300),
        )
        .await?;
        assert!(sold.is_success(), "{sold:?}");
        assert_eq!(
            balance_of(&setup, &coin, &step.account).await?,
            0,
            "the hand sell of {} did not fill in full",
            step.account
        );
    }
    let mut kept = Vec::new();
    for k in &keepers {
        kept.push(balance_of(&setup, &coin, k).await?);
    }
    let file_before = std::fs::read(&path)?;

    // A wrong hash stops it before any signature.
    let error = run_continue(&setup, &path, &"0".repeat(64), &mut PassList::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("nothing was signed"), "{error}");
    assert_eq!(
        balance_of(&setup, &coin, &sellers[3]).await?,
        plan.steps[3].amount
    );

    let sells = run_continue(&setup, &path, &sha, &mut PassList::default()).await?;

    assert_eq!(std::fs::read(&path)?, file_before, "the plan file changed");
    assert_eq!(sells.len(), 15);
    for (k, sell) in sells.iter().enumerate() {
        let i = k + 3;
        assert_eq!(sell.account, sellers[i], "sell {} out of order", i + 1);
        assert_eq!(sell.tokens_asked, plan.steps[i].amount);
    }
    for (k, a) in keepers.iter().enumerate() {
        assert_eq!(balance_of(&setup, &coin, a).await?, kept[k], "{a} sold");
    }
    // Done: a second continue finds every seller sold or not at its planned amount.
    let again = run_continue(&setup, &path, &sha, &mut PassList::default()).await;
    match again {
        Ok(sells) => assert!(sells.is_empty(), "{sells:?}"),
        Err(e) => assert!(e.to_string().contains("nothing was signed"), "{e}"),
    }
    Ok(())
}
