//! The recorded second Option C testnet run (`evidence/testnet-cabal2-run.json`) holds the run as
//! the run log printed it: 10 buys, 10 early sells, one later buy, 10 sells and one later sell, with
//! every transaction and receipt hash the log printed and no other hash. The hashes below are copied
//! from that log, so this test does not read the log itself.

use serde_json::Value;
use std::collections::BTreeSet;

const LOGGED_HASHES: [&str; 43] = [
    "BgzcrdkorKb9iLzoS4a4WqVrTkL8ihXssb15wAmpw5RP",
    "5SHs53cnj6jyNUVJ8k77EuGRyBAnm1xrp6nWXQDXKwp8",
    "8LeJ8BsRwGwacUfCvwyM2TT1urNbPhXzVnG9sRrGJvaS",
    "9pKS3FcqZdgNVpVBDfGwh4Q4CpEYghPQJW9p8kZknf7M",
    "4z6sSoJTeiDukHrsS8NxymE68vKUsXCruqrb15BoKxKf",
    "7ViWPbWQVZ5HfyWGK6aQySeJ5hVDd1Q4qgzJQp75kx5D",
    "8FjbHwys56xHosWg15WbtEHQ49VPWzXEHdjhoQ1XHP7",
    "FWJGFvHx79NgJvw8qz8WahAbQETDNQmD7Tf44o5gEKW5",
    "EATigiacRxtVd98hFS3jYLLcbev3TfjXroEtMdfjEgtY",
    "EE3xoHkdWHQ3a6qJ4ZtJ45wyAw16pm944bM9gexJ34Vg",
    "4Wbv2Dy1HkkCsvRLMYt9DRAM1iESRfQZTVFoNAQ8eBtw",
    "HHY8oh2HmB9TNqf5uurRLEmG9Lu5JFpAnqT8AWVL8j3t",
    "7gfWzxvJD4uu2DuvFu5jc63Uq1rEKNbpkEAoNCVtv4Sq",
    "2bay4tNEMnTRh5wRt17pr3BurSXsQKEuPzX2AMKwNFBo",
    "8Cfbe11kcjVJeAJP7bD1Gsmp7Gw2ydACB2tUmMLsepjf",
    "4JfgAC49RPrqwgXY4V9yXC69rJj1KXg7P4UJ6kcRWY6f",
    "CyGC619Kmgvz6g61kE8DXyPdLtA1V7jfjETjMnq6mWVM",
    "EfPnedGoxCEwpsZx73v9g3P1pEhexPt4TKviGNn69Bk3",
    "9ENYTn2PuM7a3KbPfyacsjtvpVr6kEe1NttoAjtBYMLN",
    "CRgbcpRp9b8QYUvgxZTLTKepqYiZQLGAw2iEHxk7udSi",
    "3Tt7Kq2GRUHT9JV93tWXfqRvMeGzNxpqSKYxh2LAL3ps",
    "9k4WFfawxaGxzHEa91JGouyByesSymi3u9ViDANVXzZa",
    "4RPq2EpBoBynMssXJwP7Pf5QgReEuQdvPYf8Kdqws7Mu",
    "78k3L7HEoWpU3Y43PCWSDDEDbUXqwV26KGhqGqnGopPF",
    "DvZgxNL4taSxyAHedhMUmwnpfw7X9YJxd95Q2MW9BUC3",
    "3LuiWdwwibbjhg5yZpCFwXcKwycvVM2aK7MrYCJfvyz4",
    "6qFYiKndcVyBnPU8X3qLhVNMkdRE5oBt9haQc8aw3EYX",
    "9NHp9h7QkVJRPbcTiogq6X6jhtAfDUnMz1A2yRkZ7ayB",
    "4wqsptkQ7opLP8P5Z1hzhwQh1GWbnwHa7Watt2J7bFCA",
    "7AqrJJJPwBXkoWQigdyRTheJxAqwkYa2BCyNsdBEpmM2",
    "DmL1cBNknBn9LjQnJJCUEUbzFGsJLyAuJe5vf8s2VQai",
    "4s5RTkyR1DTK5ZvN9hhyHdCrA3qCnakd3L7Ne1UD7mEY",
    "52jmu8U5i7XS166HwxfGzxzWd1WW1H6PqEyCzAc8U4ZW",
    "H1h8SBvYuwZJDDxWbEpFwpzYiExMa6Sbo7VcmqRgk9Y",
    "HLvgm2uMWdY4E1bkhmyGCtT1JkKUbxtAi56dDj4WP5dJ",
    "AYmZbvhRqodNX2BtG6x9QXYUwEs99cV7m9MgYS7K9Yfh",
    "4AVYMu44D7c57BgGs1jgahwMU1K4TxZ2Fgcg7sKQg9zs",
    "6YJFVF12JKkeadZfLJ9ecyvhQu93QvVeZwPu5fGKRhW2",
    "F5aF9zSNLkvgSBLM6XysBdZhYzP8TGaLpndY6x1ftviE",
    "G6x6AEtTCAfEcx6pw9Z4BocJhkSFhFESrz1cSEh1MXaX",
    "EwuYfe5cXp2Hev6riwa7a6a2H6dbkgUBhQnR466socQ7",
    "DZecpkQCs6Ty6kvzf9iRq7LREponzxSLL8892kXCMWrn",
    "A6j6eFJ664trF575bTzaecPuzv4kUTtkmy5ZiGrcMtmh",
];

fn evidence() -> (String, Value) {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../evidence/testnet-cabal2-run.json"
    );
    let text = std::fs::read_to_string(path).expect("read evidence/testnet-cabal2-run.json");
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

/// Every token amount the log printed in M or K is rounded, so its key must say so.
fn rounded_amounts_without_the_mark(value: &Value, key: &str, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => map
            .iter()
            .for_each(|(k, item)| rounded_amounts_without_the_mark(item, k, found)),
        Value::Array(items) => items
            .iter()
            .for_each(|item| rounded_amounts_without_the_mark(item, key, found)),
        Value::String(text) => {
            let short = text.strip_suffix(['M', 'K']).is_some_and(|n| {
                !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '.')
            });
            if short && !key.ends_with("_rounded") {
                found.push(format!("{key}: {text}"));
            }
        }
        _ => {}
    }
}

fn list<'a>(run: &'a Value, key: &str) -> &'a Vec<Value> {
    run[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is a list"))
}

#[test]
fn the_cabal2_run_names_its_coin_network_and_runner_commit() {
    let (_, run) = evidence();
    assert_eq!(run["network"], "testnet");
    assert_eq!(run["coin"], "cabal2.factory.fairchance.testnet");
    assert_eq!(run["runner_commit"], "67f5fb2");
    assert_eq!(
        run["launch"]["tx"],
        "BgzcrdkorKb9iLzoS4a4WqVrTkL8ihXssb15wAmpw5RP"
    );
    assert_eq!(run["launch"]["launch_block"], 277642845);
    assert_eq!(run["exit_code"], 0);
}

#[test]
fn the_cabal2_run_has_10_buys_10_early_sells_10_sells_and_one_later_buyer() {
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
    for (reason, count) in [("allowance", 2), ("cap_full", 1), ("floor", 7)] {
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
fn every_rounded_token_amount_is_marked_rounded() {
    let (_, run) = evidence();
    let mut found = Vec::new();
    rounded_amounts_without_the_mark(&run, "", &mut found);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_cabal2_run_file_holds_no_rpc_url_and_no_key() {
    let (text, _) = evidence();
    for needle in ["http", "ed25519:", "secp256k1:", "secret", "private"] {
        assert!(
            !text.contains(needle),
            "the evidence file contains {needle}"
        );
    }
}
