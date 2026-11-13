use std::fs::{self, File};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use fair_chance_sandbox_tests::newer_sources;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fair-chance-wasm-freshness-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn touch(path: &PathBuf, at: SystemTime) {
    File::create(path).unwrap().set_modified(at).unwrap();
}

#[test]
fn a_source_newer_than_the_wasm_is_reported() {
    let dir = scratch("newer");
    let now = SystemTime::now();
    let wasm = dir.join("coin.wasm");
    touch(&wasm, now - Duration::from_secs(60));
    touch(&dir.join("src/lib.rs"), now);
    touch(&dir.join("Cargo.toml"), now - Duration::from_secs(120));

    let newer = newer_sources(&wasm, &[dir.join("src"), dir.join("Cargo.toml")]).unwrap();
    assert_eq!(newer, vec![dir.join("src/lib.rs")]);
}

#[test]
fn a_wasm_newer_than_every_source_is_fresh() {
    let dir = scratch("fresh");
    let now = SystemTime::now();
    let wasm = dir.join("coin.wasm");
    touch(&dir.join("src/lib.rs"), now - Duration::from_secs(60));
    touch(&wasm, now);

    assert!(newer_sources(&wasm, &[dir.join("src")]).unwrap().is_empty());
}

#[test]
fn a_missing_wasm_is_an_error() {
    let dir = scratch("missing");
    assert!(newer_sources(&dir.join("coin.wasm"), &[dir.join("src")]).is_err());
}
