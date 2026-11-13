//! The coin's callable methods, read from the export section of the built wasm (ADR 0006):
//! no owner, pause, setter, upgrade, burn or metadata-changing method exists.

use anyhow::{bail, Context, Result};
use fair_chance_sandbox_tests::wasm;

/// Every method the coin may export: the methods in INTERFACE.md and the standard NEP methods
/// it names (NEP-141, NEP-145, NEP-148, NEP-330). A new method must be added here on purpose.
const ALLOWED_COIN_METHODS: [&str; 24] = [
    "new",
    "buy",
    "sell",
    "claim_creator_fees",
    "fc_rules",
    "fc_pool",
    "fc_early_window",
    "fc_cap",
    "fc_account",
    "fc_quote_sell",
    "fc_quote_buy",
    "fc_creator_fees",
    "ft_transfer",
    "ft_transfer_call",
    "ft_total_supply",
    "ft_balance_of",
    "ft_metadata",
    "storage_deposit",
    "storage_withdraw",
    "storage_unregister",
    "storage_balance_bounds",
    "storage_balance_of",
    "contract_source_metadata",
    // The claim payout callback (INTERFACE.md: the callback restores the balance).
    "on_creator_claim",
];

/// Words that would name a method that changes who controls the coin, its code, its supply
/// or its metadata.
const FORBIDDEN_WORDS: [&str; 12] = [
    "owner",
    "pause",
    "set_",
    "upgrade",
    "migrate",
    "deploy",
    "burn",
    "mint",
    "admin",
    "update",
    "metadata_",
    "delete",
];

#[test]
fn the_coin_exports_only_interface_and_standard_methods() -> Result<()> {
    let methods = exported_functions(&wasm("fair_chance_coin")?)?;
    assert!(methods.contains(&"buy".to_string()), "{methods:?}");
    for method in &methods {
        assert!(
            ALLOWED_COIN_METHODS.contains(&method.as_str()),
            "the coin exports {method}, which is not an interface or standard method"
        );
        for word in FORBIDDEN_WORDS {
            assert!(!method.contains(word), "the coin exports {method}");
        }
    }
    Ok(())
}

#[test]
fn the_export_reader_finds_every_function_export() -> Result<()> {
    // A minimal module: two function exports ("a", "bc") and one memory export ("memory").
    let module = [
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic and version
        0x07, 0x13, 0x03, // export section, 19 bytes, 3 exports
        0x01, b'a', 0x00, 0x00, // "a", function 0
        0x02, b'b', b'c', 0x00, 0x01, // "bc", function 1
        0x06, b'm', b'e', b'm', b'o', b'r', b'y', 0x02, 0x00, // "memory", memory 0
    ];
    assert_eq!(exported_functions(&module)?, vec!["a", "bc"]);
    Ok(())
}

/// The names of all function exports of a wasm module.
fn exported_functions(module: &[u8]) -> Result<Vec<String>> {
    if module.get(..8) != Some(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]) {
        bail!("not a wasm 1 module");
    }
    let mut at = 8;
    while at < module.len() {
        let id = module[at];
        at += 1;
        let size = leb128(module, &mut at)? as usize;
        let end = at + size;
        if id == 7 {
            let mut names = Vec::new();
            let count = leb128(module, &mut at)?;
            for _ in 0..count {
                let len = leb128(module, &mut at)? as usize;
                let name = std::str::from_utf8(module.get(at..at + len).context("short name")?)?;
                at += len;
                let kind = *module.get(at).context("short export")?;
                at += 1;
                let _index = leb128(module, &mut at)?;
                if kind == 0x00 {
                    names.push(name.to_string());
                }
            }
            return Ok(names);
        }
        at = end;
    }
    bail!("the module has no export section")
}

/// One unsigned LEB128 number at `at`; moves `at` past it.
fn leb128(bytes: &[u8], at: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    let mut shift = 0;
    loop {
        let byte = *bytes.get(*at).context("short LEB128 number")?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift > 63 {
            bail!("LEB128 number too long");
        }
    }
}
