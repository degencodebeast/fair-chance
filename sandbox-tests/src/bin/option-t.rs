//! Option T: one creator launches a coin, 20 buyer accounts buy in blocks 0 to N, and then all of
//! them try to sell. The sells share one early-supply allowance and fill the real NEAR cap. The
//! run ends with one `PASS <check>: <detail>` line per check and exits 0; the first failed check
//! prints `FAIL <check>: <reason>` and exits 1.
//!
//!   option-t                          a fresh sandbox (the default)
//!   option-t --network testnet        testnet; refused unless the builder sets
//!                                     FC_BUILDER_SAID_YES_TO_TESTNET_SIGNING=yes for that run
//!   option-t --delete-buyers          the separate delete step: deletes the 20 buyer accounts
//!                                     (never the creator, coin or factory); on testnet it also
//!                                     needs FC_BUILDER_SAID_YES_TO_DELETING_OPTION_T_BUYERS=yes
//!   option-t --network testnet --ticker BAG --sells-only --sell-plan <new file>
//!                                     the BAG sells-only step: buyers 01 to 18 sell their full
//!                                     balance in order; 19 and 20 never sell (sells_only.rs)
//!   option-t --network testnet --ticker BAG --sells-only --sell-plan <that file> --continue
//!                                     --plan-sha256 <hex>: after a stopped sells run, sells the
//!                                     rest from the same plan file, in order
//!   option-t --network testnet --ticker BAG --sells-only --sell-plan <that file> --return-to-funder
//!                                     the return step after the sells: buyers 01 to 18 send their
//!                                     NEAR minus 0.05 NEAR to the funder, once; 19 and 20 untouched
//!   option-t --coin <label>.<factory> the run does not launch: it makes the buyers, signs every
//!                                     buy and early sell, then waits for the app to launch that
//!                                     coin and sends the burst when it appears (app_coin.rs). The
//!                                     creator must not be the funder or one of its sub-accounts,
//!                                     and the coin must run the known code. On sandbox a stand-in
//!                                     launches it from alice at a random delay, for a rehearsal
//!   --buy-millinear <n>               NEAR per buy, in milliNEAR (default 1000)
//!   --buyer-extra-millinear <n>       NEAR per buyer on top of its buy (default 500)
//!   --buyer-tgas <n>                  prepaid gas per buyer transaction, in TGas (default 300)
//!   --launch-deposit-millinear <n>    NEAR the creator attaches to launch (default 5000)
//!   --buyers <n>                      buyer accounts that buy in the early window (default 20)
//!   --later-buy-millinear <n>         NEAR one later buyer pays after the early window, then it
//!                                     sells every token after the dump (default 0: no later buyer)
//!   --sandbox-block-ms <n>            sandbox only: about one block every n ms (testnet: 581)
//!   --sandbox-send-delay-ms <n>       sandbox only: a wait before the launch and each burst send,
//!                                     like the public testnet RPC (about 3,400 ms on 14 Nov)
//!   --ticker <ticker>                 coin ticker, kept as given for the symbol; accounts use it
//!                                     in lowercase (default optiont)
//!   --name <name>                     coin name (default "Option T")
//!   --description <text>              coin description (default "Option T demo coin.")
//!   --start-balance <NEAR>            funder balance before the session's first signing
//!   --spend-cap <NEAR>                the most the session may spend from the start balance
//!   --keep <NEAR>                     the funder balance never goes below this
//!                                     (the last three are required on testnet; before each
//!                                     step that signs, the run reads the funder balance and
//!                                     refuses the step that would break them)
//!
//! Testnet reads FC_TESTNET_READ_RPC_URL (reads first; FastNEAR by default), FC_TESTNET_RPC_URL
//! (the public RPC: read fallback and the one RPC that takes each signed transaction, once),
//! FC_TESTNET_FACTORY and FC_TESTNET_FUNDER, and loads the
//! funder's key from the system keychain (build with `--features testnet-keychain`;
//! scripts/option-t.sh does this for `--network testnet`).

use anyhow::Result;
use fair_chance_sandbox_tests::option_t::{
    choose_buyer_deletion, choose_network, delete_buyers, parse_arguments, run, DemoNetwork,
    OptionTSetup, DELETE_BUYERS_YES_FLAG, TESTNET_YES_FLAG,
};
use fair_chance_sandbox_tests::pass_list::{print_and_exit, PassList};
use fair_chance_sandbox_tests::World;
use near_api::types::{NearGas, NearToken};

const BUYERS: usize = 20;

#[tokio::main]
async fn main() {
    let mut list = PassList::default();
    let outcome = start(&mut list).await;
    print_and_exit(&list, outcome)
}

async fn start(list: &mut PassList) -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_arguments(&args)?;
    let network = choose_network(
        args.network.as_deref(),
        std::env::var(TESTNET_YES_FLAG).ok().as_deref(),
    )?;
    let sell_plan = fair_chance_sandbox_tests::sells_only::sells_only_request(&args)?;
    let return_to_funder = fair_chance_sandbox_tests::sells_only::return_request(&args)?;
    let continue_sells = fair_chance_sandbox_tests::sells_only::continue_request(&args)?;
    // --return-to-funder and --continue need --sells-only, so they pass the BAG gate with it.
    fair_chance_sandbox_tests::option_t::check_ticker(
        args.ticker.as_deref().unwrap_or("optiont"),
        network,
        sell_plan.is_some(),
    )?;
    if sell_plan.is_some() && network != DemoNetwork::Testnet {
        anyhow::bail!(
            "--sells-only runs on testnet only (the sandbox check is the bag_sells_on_sandbox test); nothing was run"
        );
    }
    // The sells-only step runs only on bag.factory.fairchance.testnet: check the factory before any
    // read or signature, and again on the setup the run uses.
    let sells_only_coin = match sell_plan {
        Some(_) => Some(
            fair_chance_sandbox_tests::sells_only::check_sells_only_factory(
                std::env::var("FC_TESTNET_FACTORY").ok().as_deref(),
            )?,
        ),
        None => None,
    };
    let delete = args.delete_buyers;
    if delete {
        choose_buyer_deletion(
            network,
            std::env::var(DELETE_BUYERS_YES_FLAG).ok().as_deref(),
        )?;
    }
    fair_chance_sandbox_tests::option_t::check_sandbox_only_flags(&args, network)?;
    let limits = fair_chance_sandbox_tests::option_t::choose_spend_limits(&args, network)?;
    let buyers = args.buyers.map_or(BUYERS, |n| n as usize);
    if buyers == 0 {
        anyhow::bail!("--buyers takes 1 or more; nothing was run");
    }
    // With --coin the accounts take the coin's label (cabal3-buyer-01 for cabal3.<factory>); the
    // full check of the coin and of the flags with it runs on the setup's factory below.
    let ticker = match &args.coin {
        Some(coin) => coin.split('.').next().unwrap_or_default().to_string(),
        None => args.ticker.clone().unwrap_or_else(|| "optiont".to_string()),
    };
    let tune = |setup: &mut OptionTSetup| {
        setup.buy_near = NearToken::from_millinear(args.buy_millinear.unwrap_or(1000).into());
        setup.buyer_extra =
            NearToken::from_millinear(args.buyer_extra_millinear.unwrap_or(500).into());
        setup.buyer_gas = fair_chance_sandbox_tests::app_coin::buyer_gas(&args);
        setup.launch_deposit =
            NearToken::from_millinear(args.launch_deposit_millinear.unwrap_or(5000).into());
        setup.later_buy = NearToken::from_millinear(args.later_buy_millinear.unwrap_or(0).into());
        setup.send_delay =
            std::time::Duration::from_millis(args.sandbox_send_delay_ms.unwrap_or(0));
        if let Some(name) = &args.name {
            setup.name = name.clone();
        }
        if let Some(description) = &args.description {
            setup.description = description.clone();
        }
    };
    let world;
    let mut world_for_stand_in = None;
    let mut setup = match network {
        DemoNetwork::Sandbox => {
            println!("option-t: start a fresh sandbox with the factory");
            world = match args.sandbox_block_ms {
                Some(ms) => World::launch_with_block_ms(ms).await?,
                None => World::launch().await?,
            };
            world_for_stand_in = Some(&world);
            OptionTSetup::sandbox_defaults(&world, &ticker, buyers)
        }
        DemoNetwork::Testnet => testnet_setup(&ticker, buyers).await?,
    };
    tune(&mut setup);
    setup.limits = limits;
    if let Some(request) =
        fair_chance_sandbox_tests::app_coin::app_coin_request(&args, &setup.factory)?
    {
        return run_on_app_coin(&setup, &request, network, &world_for_stand_in, list).await;
    }
    if let Some(plan) = sell_plan {
        let coin =
            fair_chance_sandbox_tests::option_t::coin_account_id(&setup.ticker, &setup.factory)?;
        if Some(&coin) != sells_only_coin.as_ref() {
            anyhow::bail!(
                "the run would sell on {coin}, not on the checked coin; nothing was signed"
            );
        }
        let sha = args.plan_sha256.as_deref().unwrap_or_default();
        if return_to_funder {
            fair_chance_sandbox_tests::sells_only::run_returns(&setup, &plan, sha, list).await?;
        } else if continue_sells {
            fair_chance_sandbox_tests::sells_only::run_continue(&setup, &plan, sha, list).await?;
        } else {
            fair_chance_sandbox_tests::sells_only::run_sells(&setup, &plan, list).await?;
        }
        return Ok(());
    }
    if delete {
        if network == DemoNetwork::Sandbox {
            // A fresh sandbox has no buyers yet; make them so the step has something to delete.
            fair_chance_sandbox_tests::option_t::make_accounts(&setup).await?;
        }
        let funder = setup.funder.id.clone();
        return list.check(
            "buyer accounts deleted",
            delete_buyers(&setup).await.map(|deleted| {
                format!(
                    "{} buyer accounts deleted, their NEAR went to {funder}",
                    deleted.len()
                )
            }),
        );
    }
    run(&setup, list).await?;
    Ok(())
}

/// Longest wait of the sandbox stand-in after the runner starts to wait, before it launches.
const STAND_IN_MAX_DELAY: std::time::Duration = std::time::Duration::from_secs(20);

/// The run on a coin the app launches. On testnet the known code hash is the deployed coin's; on
/// sandbox it is the World's coin code, and a stand-in task launches the coin from alice (never the
/// funder) at a random delay after the runner starts to wait.
async fn run_on_app_coin(
    setup: &OptionTSetup,
    request: &fair_chance_sandbox_tests::app_coin::AppCoinRequest,
    network: DemoNetwork,
    world: &Option<&World>,
    list: &mut PassList,
) -> Result<()> {
    use fair_chance_sandbox_tests::app_coin::{
        run_app_coin, stand_in_launch, TESTNET_COIN_CODE_HASH,
    };
    match (network, world) {
        (DemoNetwork::Testnet, _) => {
            run_app_coin(setup, request, TESTNET_COIN_CODE_HASH, list, || {}).await?;
        }
        (DemoNetwork::Sandbox, Some(world)) => {
            let (ready, go) = tokio::sync::oneshot::channel();
            let stand_in = tokio::spawn(stand_in_launch(
                world.network.clone(),
                world.factory.id.clone(),
                world.alice.clone(),
                request.label.clone(),
                STAND_IN_MAX_DELAY,
                go,
            ));
            let known = world.coin_code_hash.to_string();
            let outcome = run_app_coin(setup, request, &known, list, move || {
                let _ = ready.send(());
            })
            .await;
            let (report, launched) =
                fair_chance_sandbox_tests::app_coin::join_app_coin_run(outcome, stand_in).await?;
            if report.launch_tx != launched {
                anyhow::bail!(
                    "the run read launch tx {} from the chain, not the stand-in's {launched}",
                    report.launch_tx
                );
            }
        }
        (DemoNetwork::Sandbox, None) => anyhow::bail!("no sandbox for the stand-in"),
    }
    Ok(())
}

#[cfg(feature = "testnet-keychain")]
async fn testnet_setup(ticker: &str, buyers: usize) -> Result<OptionTSetup> {
    use anyhow::Context;
    use fair_chance_sandbox_tests::option_t::funder_signer_from_keychain;
    let read = |name: &str| std::env::var(name).with_context(|| format!("set {name}"));
    let public_rpc = read("FC_TESTNET_RPC_URL")?;
    let read_rpc = std::env::var("FC_TESTNET_READ_RPC_URL")
        .unwrap_or_else(|_| "https://rpc.testnet.fastnear.com".to_string());
    let (network, send_network) =
        fair_chance_sandbox_tests::rpcs::testnet_rpcs(&read_rpc, &public_rpc)?;
    let funder: near_api::types::AccountId = read("FC_TESTNET_FUNDER")?.parse()?;
    let factory: near_api::types::AccountId = read("FC_TESTNET_FACTORY")?.parse()?;
    for account in [&funder, &factory] {
        if !account.as_str().ends_with(".testnet") {
            anyhow::bail!("{account} is not a .testnet account");
        }
    }
    let signer = funder_signer_from_keychain(&funder, &network).await?;
    Ok(OptionTSetup {
        network,
        send_network,
        funder: fair_chance_sandbox_tests::Account { id: funder, signer },
        factory,
        ticker: ticker.to_string(),
        name: fair_chance_sandbox_tests::option_t::DEFAULT_NAME.to_string(),
        description: fair_chance_sandbox_tests::option_t::DEFAULT_DESCRIPTION.to_string(),
        buyers,
        buy_near: NearToken::from_near(1),
        buyer_extra: NearToken::from_millinear(500),
        buyer_gas: NearGas::from_tgas(300),
        launch_deposit: fair_chance_sandbox_tests::LAUNCH_DEPOSIT,
        limits: None,
        later_buy: NearToken::from_yoctonear(0),
        send_delay: std::time::Duration::ZERO,
    })
}

#[cfg(not(feature = "testnet-keychain"))]
async fn testnet_setup(_ticker: &str, _buyers: usize) -> Result<OptionTSetup> {
    anyhow::bail!(
        "the testnet path loads the funder key from the keychain; build with --features testnet-keychain (scripts/option-t.sh does this for --network testnet). Nothing was signed"
    )
}
