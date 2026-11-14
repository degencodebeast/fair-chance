//! The recorded Option C testnet run (`evidence/testnet-cabal-run.json`) holds the run as the run
//! log printed it: 10 buys, 10 early sells, one later buy, 10 sells and one later sell, with every
//! transaction and receipt hash the log printed and no other hash. The hashes below are copied from
//! that log, so this test does not read the log itself.

use serde_json::Value;
use std::collections::BTreeSet;

const LOGGED_HASHES: [&str; 43] = [
    "3H5bnbX8v29eiN4e8NaY5nT5dhEpMscan89mFwVBeHyA",
    "4kndfrpPtbrKSm86bnRw9bNEQCqpaxkW3BGRCu4NjSX8",
    "3ESs3YontS3bQPLh44b5wvvbGyK2pmLp1uCT4LZYfcXe",
    "BYMTUf2xZTDmau1mQeKHmMh9F1afV5MfQntZ4V5PDK6u",
    "DP1eUB75stGDbPA1Q2S6GG1dYGfr3pxhckBUnWgKKMyj",
    "3QS7YD3686vLonrWmwmy9tqiMJppTWDA5RZYhFgmaaE2",
    "HDxGY4wovRdorwkHJDR9kkJUNuUL2xRnhjJvwS4k9tJ3",
    "99vAfCykKmRKgE6q2fDvgwg26RNXBJP1TXuZWLBME3eD",
    "GZAD992xEe2VJ3HqED2ARQSWw2u9DYetxLXJ8pCtRvcW",
    "Vwd7JrKxPLVTUDSMyqu5rZtf47ueSbbPa48aPsPTiof",
    "Ft5NndMrkFidK9SC77HoDV2cy4kcGpKRfeQVWmZWZcRj",
    "4Xu56idMDiLPvq7REK7Tt3uotogc4MCS2othPy6dvbxm",
    "BmXw5mFGVbAawLzhfrpv8mJBE9tjM2M3mWqcYoQhHcG3",
    "oSoUkfYTCpxrCGh5RSDjwcrNeHCZHH5D3A9NuRiPFWS",
    "FCho5bPhuieb57TacEdZ7G12QgZAr6eCaYa5gtga6Nq7",
    "28e5Jipxaf8W3oaimMoU5uR47cSWr9HfDReEhbtY5QNc",
    "2rQSR4gJNSeaR2RhaK8x6wNNCMsAvrnYpGbhqXwFg922",
    "9KdQcXvBtChNCYXhhRRcxpUvDVQwP1iCX27BXpaGiEaV",
    "5ieGVJwYbdj7GZNZVqfLYoTUZ7m6KAJy2JYqHwPJHkFg",
    "D92fLaV637uvmVyFapeXdQjEYYd55oW1BnYZ9MsiXSxz",
    "B71BXnHenaCoBobvejm8azr7f8DZizhaVaaiLp9i6nVh",
    "4FK4K9xjsqzeZMBEGaHPZ8aG4bGrLwhgr5JTrjs9PLK5",
    "7Lwb8D19FRbx4zK91n3vdA39tZbVhitvWA2NPqZv5pHh",
    "D5qBVfoyigGwpatP13PSokHPSsSNCKit51eBSFv61C38",
    "2CX2rDAQRNtoscsGo4SdYyg9SQBjC5NyHhotgbfCtcss",
    "9u2XQg4zVJdJgR77nDtHZTytCk7Gjt2Z1s4gbjYiDKaw",
    "BG2Zm1S7z2bRxgHJgVdjuP7MVsPyrVgPYqjySqT4RTWR",
    "GW2kCKq93LGSTqmMKzkoZr4RS8YZTZVmVLVo5P3vJpqH",
    "5szf1aiELT1xP8Bv7ZnEj9sjKUqd882EXXqdR8Aq8FTJ",
    "3o7aAtpaP8T8rrYiGxEQivuJgH4ZTEdTovWtgAC4hGjm",
    "91W1Pfv2DLhyz3a6hwGrRPm8mzkWqRLE879KsEHscZPD",
    "3AGBuDto8y66WVmd8sxRLytV6SLNZv93aTkyETPkGprk",
    "4Z11QpZ8w59amdG3mWCHvJNE9Yt6dhvKxQSmYmP2VBjv",
    "6XsGiY1C6bnKmKJ9uCfpyM4ZUYyfdsL8wC1wF7abKNKj",
    "JDhUU6rYeYCk4RCXqKBA9cxZnPT4dShqy5VowvKvDSP8",
    "AbxgYdXAH3uLDrYccgu8acRAnXk1oZxsmFAhdxfpkVWS",
    "Fvs7qfLdSBf2BNgJXPABuJ58Yrcw3F5uNh3jTmTmgYzM",
    "2rEKwypRK7hUScrTNo6m5FDiH76ynZgn4E7Z7amM44ir",
    "5fnCPQ9xf9QTMxK4BuaRfauNRL2DufURnQRi8r74r17q",
    "E6CBs84mMCgpG7smXjcxh8pmeJz7x7QgZee3AGtZqCvY",
    "9qkZdY1qAZhNuy41goapuErZF63T4o176FLNKzPdX817",
    "H157YqVv8GJuNG2Rgq3k2K81Aa9tq38sHufnsHCSjR9C",
    "jy6vFrc3QSunB5NprSyc7aNT4exGxsjdwbTX8duwMY8",
];

fn evidence() -> (String, Value) {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../evidence/testnet-cabal-run.json"
    );
    let text = std::fs::read_to_string(path).expect("read evidence/testnet-cabal-run.json");
    let value = serde_json::from_str(&text).expect("the evidence file is JSON");
    (text, value)
}

fn hashes(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                match (key.as_str(), item) {
                    ("tx" | "receipt", Value::String(hash)) => found.push(hash.clone()),
                    _ => hashes(item, found),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| hashes(item, found)),
        _ => {}
    }
}

fn list<'a>(run: &'a Value, key: &str) -> &'a Vec<Value> {
    run[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is a list"))
}

#[test]
fn the_cabal_run_names_its_coin_network_and_runner_commit() {
    let (_, run) = evidence();
    assert_eq!(run["network"], "testnet");
    assert_eq!(run["coin"], "cabal.factory.fairchance.testnet");
    assert_eq!(run["runner_commit"], "5dafb73");
    assert_eq!(
        run["launch"]["tx"],
        "3H5bnbX8v29eiN4e8NaY5nT5dhEpMscan89mFwVBeHyA"
    );
    assert_eq!(run["launch"]["launch_block"], 277635900);
    assert_eq!(run["exit_code"], 0);
}

#[test]
fn the_cabal_run_has_10_buys_10_early_sells_10_sells_and_one_later_buyer() {
    let (_, run) = evidence();
    assert_eq!(list(&run, "buys").len(), 10);
    assert_eq!(list(&run, "early_sells").len(), 10);
    assert_eq!(list(&run, "sells").len(), 10);
    assert!(run["later_buy"].is_object() && run["later_sell"].is_object());
    for sell in list(&run, "early_sells") {
        assert_eq!(sell["error"], "FC_EARLY_WINDOW", "{sell}");
    }
    let reasons: Vec<&str> = list(&run, "sells")
        .iter()
        .map(|sell| sell["limit_reason"].as_str().unwrap())
        .collect();
    for (reason, count) in [("allowance", 7), ("cap_full", 1), ("floor", 2)] {
        assert_eq!(
            reasons.iter().filter(|r| **r == reason).count(),
            count,
            "{reasons:?}"
        );
    }
    let checks = list(&run, "checks");
    assert_eq!(checks.len(), 7);
    assert!(checks
        .iter()
        .all(|line| line.as_str().unwrap().starts_with("PASS ")));
}

#[test]
fn every_logged_hash_is_in_the_file_and_no_other_hash_is() {
    let (_, run) = evidence();
    let mut found = Vec::new();
    hashes(&run, &mut found);
    let found: BTreeSet<&str> = found.iter().map(String::as_str).collect();
    let logged: BTreeSet<&str> = LOGGED_HASHES.into_iter().collect();
    assert_eq!(found, logged);
}

#[test]
fn the_cabal_run_file_holds_no_rpc_url_and_no_key() {
    let (text, _) = evidence();
    for needle in ["http", "ed25519:", "secp256k1:", "secret", "private"] {
        assert!(
            !text.contains(needle),
            "the evidence file contains {needle}"
        );
    }
}

/// The early-supply total the run used was read from the chain after the run, before the window
/// ended (the value goes back to 0 at the window's end). The exact value, the block height and the
/// block hash of the read are kept next to the rounded log lines.
#[test]
fn the_early_supply_total_is_kept_as_an_exact_chain_read() {
    let (_, run) = evidence();
    let read = &run["chain_read"];
    assert_eq!(read["method"], "fc_early_window");
    assert_eq!(read["finality"], "final");
    assert_eq!(read["account"], run["coin"]);
    assert!(read["block_height"].as_u64().is_some_and(|h| h > 0));
    assert!(read["block_hash"].as_str().is_some_and(|h| h.len() >= 43));
    assert_eq!(read["shared_allowance_used"], "6912014798715522765922994");
    for field in [
        "shared_allowance_used",
        "shared_allowance",
        "early_supply_total",
        "window_end_ns",
    ] {
        let text = read[field]
            .as_str()
            .unwrap_or_else(|| panic!("{field} is a decimal string"));
        assert!(
            !text.is_empty() && text.chars().all(|c| c.is_ascii_digit()),
            "{field}"
        );
    }
}
