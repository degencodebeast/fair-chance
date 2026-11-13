#!/usr/bin/env bash
# The project verify run. Prints one last line: "verify: PASS (<mode>) ..." or "verify: FAIL (<mode>) ...".
#   .claude/skills/verify/verify.sh            sandbox mode (the default and the merge gate)
#   .claude/skills/verify/verify.sh doctor     read-only checks of the tools; starts nothing
#   .claude/skills/verify/verify.sh testnet --builder-starts-testnet
#                                              read-only page check on a testnet coin; the builder only
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
    FC_EVIDENCE_DIR="$evidence" scripts/walking-path.sh --read-in-browser \
      || finish FAIL "the walking path failed (see target/verify-evidence/$run_id/verify.log)"
    [ -s "$evidence/receipts-page.png" ] || finish FAIL "no page screenshot in the evidence folder"
    finish PASS "walking path, headless page receipt and private-string scan (evidence target/verify-evidence/$run_id/)"
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
    rpc="$(node -e 'process.stdout.write(encodeURIComponent(process.argv[1]))' "$FC_TESTNET_RPC_URL")"
    node web/scripts/read-receipt-in-browser.ts "?coin=$FC_TESTNET_COIN&rpc=$rpc" any "$evidence/receipts-page.png" \
      || finish FAIL "the testnet page shows no receipt"
    finish PASS "testnet page shows a receipt (evidence target/verify-evidence/$run_id/)"
    ;;

  *)
    echo "usage: .claude/skills/verify/verify.sh [sandbox | doctor | testnet --builder-starts-testnet]" >&2
    finish FAIL "unknown mode"
    ;;
esac
