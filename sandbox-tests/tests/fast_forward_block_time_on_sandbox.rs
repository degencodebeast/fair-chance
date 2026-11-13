//! The creator fee claim needs 24 h of block time. These check that the sandbox's
//! `sandbox_fast_forward` moves the block timestamp, not only the height, and by how much.
//!
//! nearcore adds (min + max block production delay) / 2 of block time per fast-forwarded block.
//! With the defaults that is about 0.29 s, and the sandbox makes about 120 blocks per second, so
//! 24 h takes about 300,000 blocks and 40 min. [`fast_forward_config`] raises the max delay so
//! one fast-forwarded block adds about 90 s, while normal blocks still come at the min delay.

use anyhow::Result;
use fair_chance_sandbox_tests::{check_sandbox_binary, fast_forward_config};
use near_api::{Chain, NetworkConfig, Reference};
use near_sandbox::{Sandbox, SandboxConfig};

async fn head(network: &NetworkConfig) -> Result<(u64, u64)> {
    let block = Chain::block()
        .at(Reference::Optimistic)
        .fetch_from(network)
        .await?;
    Ok((block.header.height, block.header.timestamp))
}

/// Fast-forwards `blocks` and returns the block time it added and the wall-clock time it took,
/// in seconds.
async fn fast_forward(
    sandbox: &Sandbox,
    network: &NetworkConfig,
    blocks: u64,
) -> Result<(f64, f64)> {
    let (height_before, ns_before) = head(network).await?;
    let wall_before = std::time::Instant::now();
    sandbox.fast_forward(blocks).await?;
    let (height_after, ns_after) = head(network).await?;
    let wall_s = wall_before.elapsed().as_secs_f64();
    let block_time_s = (ns_after - ns_before) as f64 / 1e9;
    println!(
        "fast_forward {blocks}: height {height_before} -> {height_after}, block time +{block_time_s:.1} s, wall clock {wall_s:.1} s"
    );
    assert!(height_after >= height_before + blocks);
    Ok((block_time_s, wall_s))
}

#[tokio::test]
async fn fast_forward_moves_the_block_time_as_well_as_the_height() -> Result<()> {
    check_sandbox_binary()?;
    let sandbox = Sandbox::start_sandbox().await?;
    let network = NetworkConfig::from_rpc_url("sandbox", sandbox.rpc_addr.parse()?);
    let (block_time_s, wall_s) = fast_forward(&sandbox, &network, 1_000).await?;
    assert!(block_time_s > wall_s * 10.0);
    Ok(())
}

#[tokio::test]
async fn with_the_fast_forward_config_24_h_of_block_time_takes_about_1_000_blocks() -> Result<()> {
    check_sandbox_binary()?;
    let sandbox = Sandbox::start_sandbox_with_config(SandboxConfig {
        additional_config: Some(fast_forward_config()),
        ..Default::default()
    })
    .await?;
    let network = NetworkConfig::from_rpc_url("sandbox", sandbox.rpc_addr.parse()?);
    // Normal blocks still come fast.
    let (start, _) = head(&network).await?;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let (after_3_s, _) = head(&network).await?;
    println!("normal blocks: {} in 3 s", after_3_s - start);
    assert!(after_3_s - start >= 3);

    let (block_time_s, wall_s) = fast_forward(&sandbox, &network, 1_000).await?;
    assert!(
        block_time_s >= 86_400.0,
        "1,000 blocks moved {block_time_s:.0} s"
    );
    assert!(wall_s < 60.0);
    Ok(())
}
