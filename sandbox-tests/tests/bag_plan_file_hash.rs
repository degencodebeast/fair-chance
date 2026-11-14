//! The return step and the continue step read the plan file the sells ran with. They take it only
//! when its sha256 equals --plan-sha256 and when it is exactly what the step itself writes (the
//! rule, 1 yoctoNEAR and min_near_out 0 for each sell). An edited file stops them before any read.

use fair_chance_sandbox_tests::option_t::parse_arguments;
use fair_chance_sandbox_tests::sells_only::{
    fixed_accounts, plan_from_balances, plan_json, plan_returns, read_plan_file, return_request,
    SellerState,
};
use near_api::types::AccountId;

fn id(s: &str) -> AccountId {
    s.parse().unwrap()
}

/// The plan file text for sellers that plan 1,000 + i BAG, as the sells step writes it.
fn plan_text() -> String {
    let funder = id("fairchance.testnet");
    let (sellers, _) = fixed_accounts("BAG", &funder).unwrap();
    let b: Vec<_> = sellers
        .into_iter()
        .enumerate()
        .map(|(i, a)| (a, 1_000 + i as u128))
        .collect();
    let plan =
        plan_from_balances(&id("bag.factory.fairchance.testnet"), "BAG", &funder, &b).unwrap();
    format!(
        "{}\n",
        serde_json::to_string_pretty(&plan_json(&plan)).unwrap()
    )
}

/// sha256 of plan_text(), from `shasum -a 256` of the same bytes.
const PLAN_SHA256: &str = "ae04e7eeb1c003d8550b3a15eb2243ebf5cd3201467600456581161431b03aa7";

#[test]
fn the_plan_file_is_taken_only_with_its_sha256() {
    let plan = read_plan_file(plan_text().as_bytes(), PLAN_SHA256).unwrap();
    assert_eq!(plan.steps.len(), 18);
    assert_eq!(plan.steps[4].amount, 1_004);
    for bad in [
        "AE04E7EEB1C003D8550B3A15EB2243EBF5CD3201467600456581161431B03AA7",
        "ae04e7eeb1c003d8550b3a15eb2243ebf5cd3201467600456581161431b03aa8",
        "ae04e7",
        "",
    ] {
        let error = read_plan_file(plan_text().as_bytes(), bad).unwrap_err();
        assert!(
            error.to_string().contains("nothing was signed"),
            "{bad}: {error}"
        );
    }
}

#[test]
fn an_edited_amount_cannot_hide_an_unsold_seller() {
    // The review case: bag-buyer-05's amount goes from 1,004 to 999,999; its sell never ran.
    let edited = plan_text().replace("\"amount\": \"1004\"", "\"amount\": \"999999\"");
    assert_ne!(edited, plan_text());
    let error = read_plan_file(edited.as_bytes(), PLAN_SHA256).unwrap_err();
    assert!(error.to_string().contains("sha256"), "{error}");

    // With the true file, the unsold seller stops the return step.
    let plan = read_plan_file(plan_text().as_bytes(), PLAN_SHA256).unwrap();
    let states: Vec<_> = plan
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| SellerState {
            account: s.account.clone(),
            bag: if i == 4 { 1_004 } else { 0 },
            near: 2_000_000_000_000_000_000_000_000,
            storage_bytes: 182,
        })
        .collect();
    assert!(plan_returns(&plan, &id("fairchance.testnet"), &states).is_err());
}

#[test]
fn a_file_that_is_not_what_the_sells_step_writes_is_refused_even_with_its_own_hash() {
    // Another rule text or min_near_out: the hash given matches this file, but the file is not
    // the plan the sells step writes.
    for edited in [
        plan_text().replacen("\"min_near_out\": \"0\"", "\"min_near_out\": \"12345\"", 1),
        plan_text().replacen("\"deposit_yocto\": \"1\"", "\"deposit_yocto\": \"2\"", 1),
    ] {
        let sha = near_api::types::CryptoHash::hash(edited.as_bytes())
            .0
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let error = read_plan_file(edited.as_bytes(), &sha).unwrap_err();
        assert!(error.to_string().contains("nothing was signed"), "{error}");
    }
}

#[test]
fn the_return_step_needs_the_plan_sha256() {
    let args = |l: &[&str]| -> Vec<String> { l.iter().map(|s| s.to_string()).collect() };
    let base = [
        "--network",
        "testnet",
        "--ticker",
        "BAG",
        "--sells-only",
        "--sell-plan",
        "/tmp/p.json",
        "--return-to-funder",
    ];
    let error = parse_arguments(&args(&base))
        .and_then(|p| return_request(&p))
        .unwrap_err();
    assert!(error.to_string().contains("--plan-sha256"), "{error}");
    let mut with = base.to_vec();
    with.extend(["--plan-sha256", PLAN_SHA256]);
    assert!(return_request(&parse_arguments(&args(&with)).unwrap()).unwrap());
}
