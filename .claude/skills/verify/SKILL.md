---
name: verify
description: Drive Fair Chance (NEAR coin factory, coin contract, Go sell watcher, coin page) end to end on a fresh near-sandbox and prove it works. Runs the walking path (launch, buy, limited sell, fc_sell receipt, watcher alert), opens the coin page in headless Chrome to read one receipt, the stats, the code hash and the account checks, and scans the repo for private strings. Use it before every merge, after any change to contracts, watcher or page, and whenever a done line needs proof.
---

# Verify Fair Chance

One command is the merge gate. It prints one last line, `verify: PASS (sandbox) ...` or `verify: FAIL (sandbox) <reason>`.

```bash
.claude/skills/verify/verify.sh
```

Run it from the repo root (any folder works; the script finds the root). A full run takes about 1 to 3 minutes after the first build.

## What the run does

1. **Doctor** (read only): Node equals `web/.nvmrc`, cargo-near 0.21.1, rustc equals `rust-toolchain.toml`, Go present, `NEAR_SANDBOX_BIN_PATH` set and reporting release 2.13.4, Google Chrome found.
2. **Web packages**: `npm ci` in `web/` only when `node_modules` does not match `package-lock.json` (one install at a time, lock folder `target/npm-install.lock`).
3. **Private-string scan**: `scripts/test-scan-private-strings.sh`, then `scripts/scan-private-strings.sh`. It fails on private build-plan names (ticket or part numbers), private workspace folder names and home folder paths in any file git tracks or would add, and in every commit message from `HEAD`. Commits listed in `scripts/private-string-scan-known-commits.txt` are skipped and counted.
4. **Walking path with the browser step**: `scripts/walking-path.sh --read-in-browser` (see Drive).
5. **Evidence check**: the page screenshot must exist.

## Launch

There is no long-lived server. Each run launches its own instance:

- `scripts/walking-path.sh` builds the coin and factory wasm, the Go watcher (`target/watcher`) and the runner, under the lock folder `target/build.lock`. Runs at the same time take turns here.
- The runner (`sandbox-tests/src/bin/walking-path.rs`) starts near-sandbox from `NEAR_SANDBOX_BIN_PATH` on a free RPC port with a new temporary data folder. It never downloads a sandbox binary.
- Ready means: the line `walking-path:   coin walk.factory.sandbox launch_block <n> version "<interface version>"`. At that point the factory, the coin code (deployed by hash) and one coin exist.
- Teardown is automatic: the sandbox process stops and its data folder is deleted when the runner exits, on success or failure.

To keep a sandbox up for manual driving: `scripts/walking-path.sh --hold`. It stops after the sell and prints `FC_RPC_URL=` and `FC_COIN=`. Stop it with Ctrl-C.

## Doctor

```bash
.claude/skills/verify/verify.sh doctor
```

Read only. It starts nothing. Run it first when anything looks wrong. Each line is `doctor: ok   <tool> <version>` or `doctor: FAIL <tool>: want <x>, have '<y>'`.

## Drive

The runner drives the real user path through the contracts' public methods, then reads the chain the way the page and the watcher do:

- **Launch**: the factory `launch` call; checks one `fc_launch` event and `contract_source_metadata.version`.
- **Buy**: `alice.sandbox` calls `buy` with 1 NEAR.
- **Limited sell**: `alice.sandbox` sells half with `sell` and 1 yoctoNEAR; checks one `fc_sell` event with the interface field names.
- **Receipt by block scan**: `node web/scripts/read-receipts.ts` reads the page model from the URL alone and must find the sell receipt.
- **Receipt in the browser**: `node web/scripts/read-receipt-in-browser.ts "?coin=<coin>&rpc=<url-encoded RPC URL>&factory=<factory>&code_hash=<hash>" <receipt id> [screenshot.png]`. It builds the page with Vite into a new temporary folder, serves it on a free `127.0.0.1` port, opens it in headless Google Chrome (`playwright-core` 1.64.1, branded Chrome channel, fresh profile) and waits for `#receipts li[data-receipt-id="<receipt id>"]`. The row text (`Block <n>: ` plus `.receipt-sentence`) must equal `Block <n>: <page row text>`. The step also fails when the stats row is incomplete, the rules show no code hash, or a coin account check does not pass. If the page shows `Values cannot load: ...` in `#status`, the step fails with that text.
- **Alert**: `target/watcher -once -want 1` with `FC_SENDER=fake` reads the event through its adapter; the alert must name the seller.

Stable handles in the page: `#coin` (coin name), `#status` (load state), `#stats`, `#code-hash`, `#chain-checks li[data-check]`, `#receipts li[data-receipt-id]` (one row per sell receipt).

Set `FC_BROWSER_PATH` to a Chrome binary when Chrome is not at its standard install place.

## Evidence

Each run writes to `target/verify-evidence/<UTC time>-<pid>/`:

- `verify.log`: the full run output, including the doctor lines, the scan result, every walking-path step, the receipt id, the browser version and row, and the alert text.
- `receipts-page.png`: the coin page as headless Chrome showed it, with the stats, who can sell, the sell receipt row and the rules.

Proof standard: a done line quotes the command and the last lines of `verify.log`, ending in `verify: PASS (sandbox)`. "Tests pass" alone is not proof.

## Two runs at the same time

Safe. Each run has its own sandbox port and data folder, its own page build folder and server port, its own browser profile and its own evidence folder. Builds and `npm ci` take turns through lock folders in `target/`. A lock left by a dead run is taken over.

## Testnet mode (the builder only)

```bash
FC_TESTNET_RPC_URL=<public testnet RPC URL> FC_TESTNET_COIN=<coin>.testnet \
  .claude/skills/verify/verify.sh testnet --builder-starts-testnet
```

- It refuses to start without the flag `--builder-starts-testnet`. An agent never passes this flag.
- It is read only: doctor, private-string scan, then the browser page read of the first receipt (`any`) of a coin that the builder already deployed and sold on. It holds no keys, signs nothing and reads no key file.
- It refuses a coin that does not end in `.testnet` and an RPC URL that names mainnet.
- The URL comes from the environment only. Never write it into a file.

## Cleanup

- The runner stops its own sandbox and deletes its data folder. The browser script closes Chrome, stops its server and deletes its build folder. Nothing is killed by process name.
- If a run is killed with SIGKILL, a stray `near-sandbox` may stay. Find it with `ps -o pid,command -ax | grep near-sandbox` and stop only a pid that the killed run started (check its start time and command line).
- Evidence in `target/verify-evidence/` stays. Delete old run folders by hand when they are no longer needed.

## CI

`.github/workflows/build-and-test.yml` runs the private-string scan, the Rust format check, the wasm build, the rule engine and contract tests (`cargo test --workspace --exclude fair-chance-sandbox-tests`), the watcher tests and the web typecheck, tests and build. It does not run the sandbox walking path or the browser. CI runs only after a push.

## Feature map

`features/README.md` lists the user-facing features and how this run proves each one.
