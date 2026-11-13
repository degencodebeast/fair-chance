//! The factory deploy step plans its actions and costs from the real wasm bytes before it signs
//! anything, and runs on testnet only with the builder's yes flag.

use fair_chance_sandbox_tests::factory_deploy::{
    parse_deploy_arguments, DeployMode, DeployPlan, FACTORY_EXTRA, FACTORY_STATE_MARGIN_BYTES,
    GLOBAL_CONTRACT_YOCTO_PER_BYTE, STORAGE_YOCTO_PER_BYTE,
};
use fair_chance_sandbox_tests::option_t::TESTNET_YES_FLAG;
use near_api::types::{AccountId, CryptoHash, NearToken};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|a| a.to_string()).collect()
}

fn funder() -> AccountId {
    "fairchance.testnet".parse().unwrap()
}

#[test]
fn the_global_deploy_burns_a_ten_thousandth_of_a_near_per_coin_code_byte() {
    // nearcore parameters.yaml: global_contract_storage_amount_per_byte 0.0001 N, ten times
    // storage_amount_per_byte (10^19 yoctoNEAR).
    assert_eq!(GLOBAL_CONTRACT_YOCTO_PER_BYTE, 100_000_000_000_000_000_000);
    assert_eq!(STORAGE_YOCTO_PER_BYTE, 10_000_000_000_000_000_000);
    let plan = DeployPlan::new(funder(), vec![7; 243_458], vec![9; 148_781]).unwrap();
    assert_eq!(
        plan.global_burn,
        NearToken::from_yoctonear(243_458 * 100_000_000_000_000_000_000)
    );
    assert_eq!(plan.global_burn.exact_amount_display(), "24.3458 NEAR");
}

#[test]
fn the_factory_is_funded_for_its_code_and_state_plus_a_fixed_extra() {
    let plan = DeployPlan::new(funder(), vec![7; 100], vec![9; 148_781]).unwrap();
    let storage = (148_781 + FACTORY_STATE_MARGIN_BYTES) * STORAGE_YOCTO_PER_BYTE;
    assert_eq!(
        plan.factory_funding,
        NearToken::from_yoctonear(storage).saturating_add(FACTORY_EXTRA)
    );
    assert_eq!(
        plan.total(),
        plan.global_burn.saturating_add(plan.factory_funding)
    );
}

#[test]
fn the_plan_names_the_factory_sub_account_and_both_code_hashes() {
    let coin = vec![1u8, 2, 3];
    let factory = vec![4u8, 5];
    let plan = DeployPlan::new(funder(), coin.clone(), factory.clone()).unwrap();
    assert_eq!(plan.factory.as_str(), "factory.fairchance.testnet");
    assert_eq!(plan.coin_code_hash, CryptoHash::hash(&coin));
    assert_eq!(plan.factory_code_hash, CryptoHash::hash(&factory));
    let text = plan.describe().join("\n");
    for needle in [
        "fairchance.testnet",
        "factory.fairchance.testnet",
        &plan.coin_code_hash.to_string(),
        &plan.factory_code_hash.to_string(),
        "by code hash",
        "new",
    ] {
        assert!(text.contains(needle), "{needle} missing from:\n{text}");
    }
}

#[test]
fn an_empty_wasm_is_refused() {
    assert!(DeployPlan::new(funder(), vec![], vec![1]).is_err());
    assert!(DeployPlan::new(funder(), vec![1], vec![]).is_err());
}

#[test]
fn dry_run_needs_no_yes_flag() {
    assert_eq!(
        parse_deploy_arguments(&args(&["--dry-run"]), None).unwrap(),
        DeployMode::DryRun
    );
}

#[test]
fn testnet_runs_only_with_the_yes_flag_exactly_yes() {
    assert_eq!(
        parse_deploy_arguments(&args(&["--network", "testnet"]), Some("yes")).unwrap(),
        DeployMode::Testnet
    );
    let error = parse_deploy_arguments(&args(&["--network", "testnet"]), None).unwrap_err();
    assert!(error.to_string().contains(TESTNET_YES_FLAG), "{error}");
    for value in ["", "YES", "yes ", "true"] {
        assert!(
            parse_deploy_arguments(&args(&["--network", "testnet"]), Some(value)).is_err(),
            "{value:?}"
        );
    }
}

#[test]
fn every_other_form_is_refused() {
    for list in [
        vec![],
        vec!["--network"],
        vec!["--network", "mainnet"],
        vec!["--network", "sandbox"],
        vec!["--network=testnet"],
        vec!["--dry-run", "--network", "testnet"],
        vec!["--dry-run", "--dry-run"],
        vec!["--network", "testnet", "--network", "testnet"],
        vec!["deploy"],
    ] {
        assert!(
            parse_deploy_arguments(&args(&list), Some("yes")).is_err(),
            "{list:?} was accepted"
        );
    }
}

#[test]
fn only_the_nodes_no_global_code_answer_for_this_hash_counts_as_absent() {
    use fair_chance_sandbox_tests::factory_deploy::is_no_global_code;
    let hash: CryptoHash = "2qUeEiS27wiu3QZAbsJMP7adv1iZhY7zu8EhCSHUiEj5"
        .parse()
        .unwrap();
    // The Debug text near-api 0.8.7 gave on near-sandbox 2.13.4 (trimmed).
    let seen = r#"Critical(TransportError(Invalid Response Payload (b"{\"jsonrpc\":\"2.0\",\"error\":{\"name\":\"HANDLER_ERROR\",\"cause\":{\"info\":{\"identifier\":{\"hash\":\"2qUeEiS27wiu3QZAbsJMP7adv1iZhY7zu8EhCSHUiEj5\"}},\"name\":\"NO_GLOBAL_CONTRACT_CODE\"}}}")))"#;
    assert!(is_no_global_code(seen, hash));
    let other: CryptoHash = CryptoHash::hash(b"other");
    assert!(!is_no_global_code(seen, other), "another hash counted");
    assert!(!is_no_global_code("TransportError(timeout)", hash));
    assert!(!is_no_global_code(
        "UNKNOWN_BLOCK 2qUeEiS27wiu3QZAbsJMP7adv1iZhY7zu8EhCSHUiEj5",
        hash
    ));
}
