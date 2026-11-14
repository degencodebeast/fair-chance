//! The relaunch rehearsal on a sandbox close to testnet: about 0.6 s per block and a 3.3 s wait
//! before each burst send (the public testnet RPC answered each call in about 3.4 s on 14 Nov).
//! Five snipers sign their buys and early sells before the launch. The buys and the early sells
//! land in the early window, every early sell is refused with FC_EARLY_WINDOW, the snipers'
//! dump after the window is mostly held back by one shared allowance and the cap, and one ordinary
//! later buyer sells all of its tokens.

use std::time::Duration;

use anyhow::Result;
use fair_chance_sandbox_tests::option_t::{run, OptionTSetup};
use fair_chance_sandbox_tests::pass_list::PassList;
use fair_chance_sandbox_tests::World;
use near_api::types::{NearGas, NearToken};

#[tokio::test]
async fn presigned_snipers_land_in_the_early_window_and_their_dump_is_held_back() -> Result<()> {
    let world = World::launch_with_block_ms(600).await?;
    let mut setup = OptionTSetup::sandbox_defaults(&world, "relaunch", 5);
    setup.launch_deposit = NearToken::from_near(1);
    setup.buyer_extra = NearToken::from_millinear(200);
    setup.buyer_gas = NearGas::from_tgas(100);
    setup.later_buy = NearToken::from_millinear(300);
    setup.send_delay = Duration::from_millis(3300);
    let mut list = PassList::default();
    let report = run(&setup, &mut list).await;
    for line in list.lines() {
        println!("{line}");
    }
    let report = report?;
    assert!(
        report.timing.last_block <= 17,
        "last buy in block {}",
        report.timing.last_block
    );
    let lines = list.lines().join("\n");
    for check in [
        "5 buyers bought in blocks 0 to 17",
        "every early sell is refused with FC_EARLY_WINDOW",
        "one shared allowance",
        "most of the dump is held back",
        "the later buyer sells all of its tokens",
    ] {
        assert!(
            lines.contains(&format!("PASS {check}")),
            "no PASS {check}:\n{lines}"
        );
    }
    Ok(())
}
