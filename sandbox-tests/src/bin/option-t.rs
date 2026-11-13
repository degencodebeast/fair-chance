//! Option T: one creator launches a coin, 20 buyer accounts buy in blocks 0 to N, and then all of
//! them try to sell. The sells share one early-supply allowance and fill the real NEAR cap. The
//! run ends with one `PASS <check>: <detail>` line per check and exits 0; the first failed check
//! prints `FAIL <check>: <reason>` and exits 1.
//!
//!   option-t                         a fresh sandbox (the default)
//!   option-t --network testnet       testnet; refused unless the builder sets
//!                                    FC_BUILDER_SAID_YES_TO_TESTNET_SIGNING=yes for that run
//!   --buy-millinear <n>              NEAR per buy, in milliNEAR (default 1000)
//!   --ticker <ticker>                coin ticker (default optiont)
//!
//! Testnet also reads FC_TESTNET_RPC_URL, FC_TESTNET_FACTORY and FC_TESTNET_FUNDER.

use anyhow::{bail, Context, Result};
use fair_chance_sandbox_tests::option_t::{
    choose_network, run, DemoNetwork, OptionTSetup, TESTNET_YES_FLAG,
};
use fair_chance_sandbox_tests::pass_list::{print_and_exit, PassList};
use fair_chance_sandbox_tests::World;
use near_api::types::NearToken;

const BUYERS: usize = 20;

#[tokio::main]
async fn main() {
    let mut list = PassList::default();
    let outcome = start(&mut list).await;
    print_and_exit(&list, outcome)
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

async fn start(list: &mut PassList) -> Result<()> {
    let network = choose_network(
        argument("--network").as_deref(),
        std::env::var(TESTNET_YES_FLAG).ok().as_deref(),
    )?;
    let buy_near = NearToken::from_millinear(
        argument("--buy-millinear")
            .map(|n| n.parse::<u128>())
            .transpose()
            .context("--buy-millinear takes a whole number")?
            .unwrap_or(1000),
    );
    let ticker = argument("--ticker").unwrap_or_else(|| "optiont".to_string());
    match network {
        DemoNetwork::Sandbox => {
            println!("option-t: start a fresh sandbox with the factory");
            let world = World::launch().await?;
            let setup = OptionTSetup {
                network: world.network.clone(),
                funder: world.bob.clone(),
                factory: world.factory.id.clone(),
                ticker,
                buyers: BUYERS,
                buy_near,
            };
            run(&setup, list).await?;
            Ok(())
        }
        DemoNetwork::Testnet => {
            for name in [
                "FC_TESTNET_RPC_URL",
                "FC_TESTNET_FACTORY",
                "FC_TESTNET_FUNDER",
            ] {
                std::env::var(name).with_context(|| format!("set {name}"))?;
            }
            // How the funder's signer is loaded on testnet is an open question for the builder;
            // until it is answered this path signs nothing.
            bail!("the testnet signer for the funder is not set up yet; nothing was signed")
        }
    }
}
