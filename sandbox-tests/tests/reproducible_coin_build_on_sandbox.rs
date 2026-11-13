//! The coin wasm from the reproducible Docker build (cargo-near `reproducible-wasm`) runs as the
//! coin's global code, its hash is the factory's recorded hash, and its NEP-330
//! `contract_source_metadata` links to the source at the commit it was built from.
//!
//! Ignored in a plain `cargo test`: it needs the Docker build output. Run it with
//! `scripts/build-coin-reproducible.sh`, which builds and then runs this file with `--ignored`.

use anyhow::Result;
use fair_chance_sandbox_tests::{reproducible_coin_build, ContractState, World};
use near_api::types::CryptoHash;
use serde_json::json;

/// The public source repository (`package.repository` of the coin crate).
const SOURCE_REPOSITORY: &str = "https://github.com/degencodebeast/fair-chance";
/// The pinned build image digest (`image_digest` in `contracts/coin/Cargo.toml`).
const BUILD_IMAGE_DIGEST: &str =
    "sha256:7467038bdddc86484b73b416eeadce926ff59013e128e53dec5a19e1cb4b2234";

#[tokio::test]
#[ignore = "needs the Docker build: run scripts/build-coin-reproducible.sh"]
async fn the_factory_records_the_hash_of_the_reproducible_coin_build() -> Result<()> {
    let build = reproducible_coin_build()?;
    let world = World::launch_with_coin_code(build.wasm).await?;
    let built: CryptoHash = build.cargo_near_sha256_base58.parse()?;

    let recorded = world
        .view(&world.factory.id, "coin_code_hash", json!({}))
        .await?;
    assert_eq!(recorded, build.cargo_near_sha256_base58);
    assert_eq!(
        world.contract_state(&world.coin).await?,
        Some(ContractState::GlobalHash(built))
    );
    let rules = world.view(&world.coin, "fc_rules", json!({})).await?;
    assert_eq!(rules["code_hash"], build.cargo_near_sha256_base58);
    Ok(())
}

#[tokio::test]
#[ignore = "needs the Docker build: run scripts/build-coin-reproducible.sh"]
async fn contract_source_metadata_links_to_the_source_at_the_built_commit() -> Result<()> {
    let build = reproducible_coin_build()?;
    let commit = build.commit.clone();
    let world = World::launch_with_coin_code(build.wasm).await?;

    let source = world
        .view(&world.coin, "contract_source_metadata", json!({}))
        .await?;
    assert_eq!(source["version"], rule_engine::INTERFACE_VERSION);
    assert_eq!(
        source["link"],
        format!("{SOURCE_REPOSITORY}/tree/{commit}"),
        "{source:#}"
    );
    let info = &source["build_info"];
    assert_eq!(
        info["source_code_snapshot"],
        format!("git+{SOURCE_REPOSITORY}?rev={commit}"),
        "{source:#}"
    );
    assert_eq!(info["contract_path"], "contracts/coin", "{source:#}");
    assert!(
        info["build_environment"]
            .as_str()
            .is_some_and(|env| env.ends_with(BUILD_IMAGE_DIGEST)),
        "{source:#}"
    );
    Ok(())
}
