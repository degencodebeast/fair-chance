//! The builder's conditions for the Option T resume: one survey of the creator and all buyers
//! before anything is signed, reads on FastNEAR with the public RPC as fallback, and each signed
//! transaction sent to one RPC only, once.

use fair_chance_sandbox_tests::option_t::{
    plan_accounts, AccountPlan, ExistingAccount, SurveyEntry,
};
use fair_chance_sandbox_tests::rpcs::testnet_rpcs;
use near_api::types::{AccountId, NearToken};

fn id(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn entry(name: &str, existing: Option<ExistingAccount>) -> SurveyEntry {
    SurveyEntry {
        id: id(name),
        planned: NearToken::from_millinear(1500),
        existing,
    }
}

fn good() -> Option<ExistingAccount> {
    Some(ExistingAccount {
        balance: NearToken::from_millinear(1500),
        funder_key_full_access: true,
    })
}

#[test]
fn the_survey_plans_creates_and_reuses_in_order() {
    let plan = plan_accounts(vec![
        entry("bag-creator.f.testnet", good()),
        entry("bag-buyer-01.f.testnet", good()),
        entry("bag-buyer-11.f.testnet", None),
    ])
    .unwrap();
    let kinds: Vec<_> = plan.iter().map(|(e, p)| (e.id.to_string(), *p)).collect();
    assert_eq!(
        kinds,
        vec![
            ("bag-creator.f.testnet".to_string(), AccountPlan::Reuse),
            ("bag-buyer-01.f.testnet".to_string(), AccountPlan::Reuse),
            ("bag-buyer-11.f.testnet".to_string(), AccountPlan::Create),
        ]
    );
}

#[test]
fn any_bad_existing_account_stops_the_whole_survey_and_every_bad_one_is_named() {
    let no_key = Some(ExistingAccount {
        balance: NearToken::from_millinear(1500),
        funder_key_full_access: false,
    });
    let short = Some(ExistingAccount {
        balance: NearToken::from_millinear(1000),
        funder_key_full_access: true,
    });
    let error = plan_accounts(vec![
        entry("bag-buyer-01.f.testnet", good()),
        entry("bag-buyer-02.f.testnet", no_key),
        entry("bag-buyer-03.f.testnet", None),
        entry("bag-buyer-04.f.testnet", short),
    ])
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("bag-buyer-02") && error.contains("bag-buyer-04"),
        "{error}"
    );
    assert!(
        !error.contains("bag-buyer-01") && !error.contains("bag-buyer-03"),
        "{error}"
    );
}

#[test]
fn reads_use_fastnear_then_the_public_rpc_and_sends_one_rpc_with_no_retry() {
    let (read, send) = testnet_rpcs(
        "https://rpc.testnet.fastnear.com",
        "https://rpc.testnet.near.org",
    )
    .unwrap();
    let urls: Vec<_> = read.rpc_endpoints.iter().map(|e| e.url.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://rpc.testnet.fastnear.com/",
            "https://rpc.testnet.near.org/"
        ]
    );
    assert_eq!(send.rpc_endpoints.len(), 1);
    assert_eq!(
        send.rpc_endpoints[0].url.as_str(),
        "https://rpc.testnet.near.org/"
    );
    for endpoint in read.rpc_endpoints.iter().chain(&send.rpc_endpoints) {
        assert_eq!(endpoint.retries, 1, "near-api must not retry on its own");
        assert!(endpoint.bearer_header.is_none());
    }
}

#[test]
fn a_url_that_could_hold_a_key_or_names_mainnet_is_refused_without_echoing_it() {
    for bad in [
        "https://rpc.testnet.fastnear.com/?apiKey=abc",
        "https://user:pw@rpc.testnet.near.org",
        "https://rpc.mainnet.near.org",
        "http://rpc.testnet.near.org",
        "not a url",
    ] {
        let error = testnet_rpcs(bad, "https://rpc.testnet.near.org")
            .unwrap_err()
            .to_string();
        assert!(!error.contains("abc") && !error.contains("pw"), "{error}");
    }
}
