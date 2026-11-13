#!/usr/bin/env bash
# The project verify run. Prints one last line: "verify: PASS (<mode>) ..." or "verify: FAIL (<mode>) ...".
#   .claude/skills/verify/verify.sh            sandbox mode (the default and the merge gate)
#   .claude/skills/verify/verify.sh doctor     read-only checks of the tools; starts nothing
#   .claude/skills/verify/verify.sh testnet --builder-starts-testnet
#                                              read-only page check on a testnet coin; the builder only
#   .claude/skills/verify/verify.sh screens    the browser screen checks alone (no sandbox, no chain)
#   .claude/skills/verify/verify.sh hosted <url>
#                                              read-only page checks on a hosted coin page; no keys, no wallet.
#                                              The URL may also come from FC_HOSTED_URL. Never write it to a file.
# Evidence for each run goes to target/verify-evidence/<run id>/ and stays after the run.
# This script holds no keys, signs nothing and never reads a key file.
set -uo pipefail
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$repo"

mode="${1:-sandbox}"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
evidence="$repo/target/verify-evidence/$run_id"

finish() { # finish <PASS|FAIL> <text>
  echo "verify: $1 ($mode) $2"
  [ "$1" = PASS ] && exit 0
  exit 1
}

doctor() {
  local problems=0
  check() { # check <name> <want> <have>
    if [ "$2" = "$3" ]; then echo "doctor: ok   $1 $3"; else echo "doctor: FAIL $1: want $2, have '${3:-none}'"; problems=$((problems + 1)); fi
  }
  check node "v$(cat web/.nvmrc)" "$(node --version 2>/dev/null)"
  check cargo-near 0.21.1 "$(cargo near --version 2>/dev/null | awk '{print $2}')"
  check rust-toolchain "$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)" \
    "$(rustc --version 2>/dev/null | awk '{print $2}')"
  check go-available yes "$(command -v go >/dev/null && echo yes)"
  if [ "$mode" = sandbox ] || [ "$mode" = doctor ]; then
    check near-sandbox-path set "$([ -n "${NEAR_SANDBOX_BIN_PATH:-}" ] && [ -x "$NEAR_SANDBOX_BIN_PATH" ] && echo set)"
    check near-sandbox-release 2.13.4 \
      "$("${NEAR_SANDBOX_BIN_PATH:-/nonexistent}" --version 2>/dev/null | sed -n 's/.*(release \([0-9.]*\)).*/\1/p' | head -1)"
  fi
  local chrome="${FC_BROWSER_PATH:-}"
  if [ -z "$chrome" ]; then
    for c in "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" /usr/bin/google-chrome /usr/bin/google-chrome-stable; do
      [ -x "$c" ] && chrome="$c" && break
    done
  fi
  check chrome found "$([ -n "$chrome" ] && [ -x "$chrome" ] && echo found)"
  return "$problems"
}

install_web_packages() {
  # npm ci only when node_modules does not match the lock file; one run at a time.
  if ! cmp -s web/package-lock.json web/node_modules/.verify-installed-lock.json 2>/dev/null; then
    scripts/run-with-lock.sh target/npm-install.lock bash -c '
      cmp -s web/package-lock.json web/node_modules/.verify-installed-lock.json 2>/dev/null && exit 0
      (cd web && npm ci --no-audit --no-fund --loglevel=error) &&
        cp web/package-lock.json web/node_modules/.verify-installed-lock.json'
  fi
}

private_string_scan() {
  scripts/test-scan-private-strings.sh | tail -1 && scripts/scan-private-strings.sh
}

# The browser screen checks. Every outside host is stubbed or blocked, so no request leaves the machine.
screen_checks_in_browser() {
  # A ?network=testnet link in Chrome, every outside host stubbed.
  node web/scripts/check-testnet-link-in-browser.ts || { echo "the testnet link does not load in the browser"; return 1; }
  # The loading and error states keep the page frame at 1440 and 390.
  node web/scripts/check-page-frames-in-browser.ts || { echo "the loading or error frame is missing or too wide"; return 1; }
  # Landing, coin list, alerts, price chart and holders table at 1440 and 390.
  node web/scripts/check-screens-in-browser.ts || { echo "a screen is missing a sample label, shows a live-looking dead control or is too wide"; return 1; }
  # Every screen loads at 1440 and 390 with no console error and the main element shown.
  node web/scripts/check-screens-load-in-browser.ts || { echo "a screen does not load, shows a console error or has no main element"; return 1; }
  # Coin page grid, phone order, trade box, claim panel and launch form with their turned-off buttons.
  node web/scripts/check-turned-off-screens-in-browser.ts || { echo "a turned-off screen has an enabled control without a handler, no reason, the wrong layout or too much width"; return 1; }
}

# Runs `verify.sh hosted` on a local build with a fake chain: it must pass on a good chain and fail on a bad one.
hosted_mode_self_test() {
  node web/scripts/check-hosted-page-on-local-build.ts
}

case "$mode" in
  doctor)
    doctor && finish PASS "the tools match the pins" || finish FAIL "fix the doctor lines above"
    ;;

  sandbox)
    mkdir -p "$evidence"
    exec > >(tee "$evidence/verify.log") 2>&1
    echo "verify: run $run_id, evidence in target/verify-evidence/$run_id/"
    doctor || finish FAIL "doctor found problems"
    install_web_packages || finish FAIL "npm ci in web/ failed"
    private_string_scan || finish FAIL "the private-string scan found private names or paths"
    # The replay tests, with the sandbox equivalence test on (it starts one sandbox).
    FC_REPLAY_ON_SANDBOX=1 scripts/test-trade-replay.sh || finish FAIL "the trade replay tests failed"
    # Option T refuses a mistyped argument before it builds or starts a sandbox.
    scripts/test-option-t-arguments.sh | tail -1 || finish FAIL "option-t.sh accepted an unknown or incomplete argument"
    screen_checks_in_browser || finish FAIL "a browser screen check failed (see target/verify-evidence/$run_id/verify.log)"
    hosted_mode_self_test || finish FAIL "the hosted command does not pass on a good local chain or does not fail on a bad one (see target/verify-evidence/$run_id/verify.log)"
    FC_EVIDENCE_DIR="$evidence" scripts/walking-path.sh --read-in-browser \
      || finish FAIL "the walking path failed (see target/verify-evidence/$run_id/verify.log)"
    [ -s "$evidence/receipts-page.png" ] || finish FAIL "no page screenshot in the evidence folder"
    finish PASS "walking path, headless page receipt and private-string scan (evidence target/verify-evidence/$run_id/)"
    ;;

  screens)
    mkdir -p "$evidence"
    exec > >(tee "$evidence/verify.log") 2>&1
    echo "verify: run $run_id, evidence in target/verify-evidence/$run_id/"
    doctor || finish FAIL "doctor found problems"
    install_web_packages || finish FAIL "npm ci in web/ failed"
    screen_checks_in_browser || finish FAIL "a browser screen check failed (see target/verify-evidence/$run_id/verify.log)"
    finish PASS "all screens load and keep their layout (evidence target/verify-evidence/$run_id/)"
    ;;

  hosted)
    # Read only: opens one hosted page, signs nothing, holds no keys. Only the host is logged.
    hosted_url="${2:-${FC_HOSTED_URL:-}}"
    [ -n "$hosted_url" ] || finish FAIL "give the page URL as the second argument or set FC_HOSTED_URL"
    mkdir -p "$evidence"
    exec > >(tee "$evidence/verify.log") 2>&1
    echo "verify: run $run_id, evidence in target/verify-evidence/$run_id/"
    doctor || finish FAIL "doctor found problems"
    install_web_packages || finish FAIL "npm ci in web/ failed"
    node web/scripts/check-hosted-page-in-browser.ts "$hosted_url" \
      || finish FAIL "the hosted page failed a check (see target/verify-evidence/$run_id/verify.log)"
    finish PASS "the hosted page loads, shows the banner and receipt list, and the account checks pass (evidence target/verify-evidence/$run_id/)"
    ;;

  testnet)
    # Only the builder starts this mode. It reads one public coin page; it signs nothing.
    [ "${2:-}" = "--builder-starts-testnet" ] \
      || finish FAIL "testnet mode is for the builder only; it needs the flag --builder-starts-testnet"
    [ -n "${FC_TESTNET_RPC_URL:-}" ] || finish FAIL "set FC_TESTNET_RPC_URL to a public testnet RPC URL with no key in it"
    [ -n "${FC_TESTNET_COIN:-}" ] || finish FAIL "set FC_TESTNET_COIN to the coin account, for example <ticker>.<factory>.testnet"
    case "$FC_TESTNET_COIN" in *.testnet) ;; *) finish FAIL "FC_TESTNET_COIN must end in .testnet" ;; esac
    case "$FC_TESTNET_RPC_URL" in *mainnet*) finish FAIL "FC_TESTNET_RPC_URL names mainnet" ;; esac
    mkdir -p "$evidence"
    exec > >(tee "$evidence/verify.log") 2>&1
    echo "verify: run $run_id, evidence in target/verify-evidence/$run_id/"
    doctor || finish FAIL "doctor found problems"
    install_web_packages || finish FAIL "npm ci in web/ failed"
    private_string_scan || finish FAIL "the private-string scan found private names or paths"
    # The page reads testnet through its own fixed RPC list; a link may not give an RPC URL.
    node web/scripts/read-receipt-in-browser.ts "?coin=$FC_TESTNET_COIN&network=testnet" any "$evidence/receipts-page.png" \
      || finish FAIL "the testnet page shows no receipt"
    finish PASS "testnet page shows a receipt (evidence target/verify-evidence/$run_id/)"
    ;;

  *)
    echo "usage: .claude/skills/verify/verify.sh [sandbox | doctor | screens | hosted <url> | testnet --builder-starts-testnet]" >&2
    finish FAIL "unknown mode"
    ;;
esac
