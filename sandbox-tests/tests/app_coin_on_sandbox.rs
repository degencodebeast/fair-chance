//! The run on a coin the app launches, on a fresh sandbox. A stand-in for the app launches the
//! coin through the factory from another account (alice), at a random delay after the runner
//! starts to wait. The runner never launches: it finds the coin, checks it, sends the burst, finds
//! the launch transaction on chain and passes the same seven checks as the cabal run. A coin the
//! funder itself launched gets no buy.
//!
//! Heavy: each test starts its own sandbox. Needs NEAR_SANDBOX_BIN_PATH.

use std::time::Duration;

use fair_chance_sandbox_tests::app_coin::{
    join_app_coin_run, run_app_coin, stand_in_launch, AppCoinRequest,
};
use fair_chance_sandbox_tests::option_t::OptionTSetup;
use fair_chance_sandbox_tests::pass_list::PassList;
use fair_chance_sandbox_tests::World;
use near_api::types::NearToken;

const STAND_IN_MAX_DELAY: Duration = Duration::from_secs(8);
/// About one block every 577 ms, as testnet on 14 Nov, so the burst guard sees testnet timing.
const TESTNET_BLOCK_MS: u64 = 577;

fn setup(world: &World, label: &str) -> (OptionTSetup, AppCoinRequest) {
    let mut setup = OptionTSetup::sandbox_defaults(world, label, 10);
    setup.buy_near = NearToken::from_millinear(500);
    setup.buyer_extra = NearToken::from_millinear(200);
    setup.later_buy = NearToken::from_millinear(300);
    let request = AppCoinRequest {
        coin: format!("{label}.{}", world.factory.id).parse().unwrap(),
        label: label.to_string(),
    };
    (setup, request)
}

#[tokio::test]
async fn a_coin_launched_from_another_account_gets_the_burst_and_passes_the_seven_checks() {
    let world = World::launch_with_block_ms(TESTNET_BLOCK_MS).await.unwrap();
    let (setup, request) = setup(&world, "cabal3");
    let (ready, go) = tokio::sync::oneshot::channel();
    let stand_in = tokio::spawn(stand_in_launch(
        world.network.clone(),
        world.factory.id.clone(),
        world.alice.clone(),
        request.label.clone(),
        STAND_IN_MAX_DELAY,
        go,
    ));
    let mut list = PassList::default();
    let known = world.coin_code_hash.to_string();
    let report = run_app_coin(&setup, &request, &known, &mut list, move || {
        let _ = ready.send(());
    })
    .await;
    let (report, launched) = join_app_coin_run(report, stand_in)
        .await
        .unwrap_or_else(|e| panic!("{e:#}\n{list:?}"));
    assert_eq!(list.lines().len(), 7, "{list:?}");
    assert!(
        list.lines().iter().all(|l| l.starts_with("PASS ")),
        "{list:?}"
    );
    assert!(report.run.timing.last_block <= 17, "{report:?}");
    // The runner found the launch transaction and the creator on chain: the stand-in's own.
    assert_eq!(report.launch_tx, launched, "{report:?}");
    assert_eq!(report.creator, world.alice.id, "{report:?}");
}

#[tokio::test]
async fn a_coin_the_funder_launched_gets_no_buy() {
    let world = World::launch_with_block_ms(TESTNET_BLOCK_MS).await.unwrap();
    let (setup, request) = setup(&world, "cabal3f");
    let (ready, go) = tokio::sync::oneshot::channel();
    // The funder of the sandbox run is bob: a coin bob launches is refused.
    let stand_in = tokio::spawn(stand_in_launch(
        world.network.clone(),
        world.factory.id.clone(),
        world.bob.clone(),
        request.label.clone(),
        Duration::ZERO,
        go,
    ));
    let mut list = PassList::default();
    let known = world.coin_code_hash.to_string();
    let refused = run_app_coin(&setup, &request, &known, &mut list, move || {
        let _ = ready.send(());
    })
    .await
    .expect_err("a coin the funder launched must be refused");
    stand_in.await.unwrap().unwrap();
    assert!(
        format!("{refused:#}").contains("no buy was sent"),
        "{refused:#}"
    );
    assert!(list.lines().is_empty(), "{list:?}");
}
