#!/usr/bin/env bash
# Checks scripts/wait-for-coin.sh against a local fake RPC: it stops at once on a 403 and on an
# answer it does not know, backs off on 429 and 5xx and gives up after a few tries, and goes on
# when the coin appears. FC_PYTHON is the Python that runs the fake server.
set -uo pipefail
cd "$(dirname "$0")/.."
python="${FC_PYTHON:?set FC_PYTHON to the Python that runs the fake server}"
tmp="$(mktemp -d)"
trap 'kill $(jobs -p) 2>/dev/null; rm -rf "$tmp"' EXIT
failures=0

check() { # check <mode> <want exit 0|1> <text the output must hold> <max requests>
  local mode="$1" want="$2" text="$3" max="$4" port out code=0 count
  elapsed=0
  : > "$tmp/port"
  "$python" -I scripts/testdata/fake_rpc_server.py "$mode" "$tmp/count-$mode" > "$tmp/port" &
  local pid=$!
  for _ in $(seq 50); do [ -s "$tmp/port" ] && break; sleep 0.1; done
  port="$(cat "$tmp/port")"
  local started=$SECONDS
  out="$(FC_WAIT_POLL_S=0 FC_WAIT_FIRST_BACKOFF_S=1 FC_WAIT_BUSY_LIMIT_S=6 timeout 30 scripts/wait-for-coin.sh "http://127.0.0.1:$port" coin.testnet 2>&1)" || code=$?
  elapsed=$((SECONDS - started))
  kill "$pid" 2>/dev/null
  count="$(cat "$tmp/count-$mode" 2>/dev/null || echo 0)"
  local ok=1
  if [ "$want" = 0 ]; then [ "$code" = 0 ] || ok=0; else { [ "$code" != 0 ] && [ "$code" != 124 ]; } || ok=0; fi
  printf '%s' "$out" | grep -q -- "$text" || ok=0
  [ "$count" -le "$max" ] || ok=0
  if [ "$ok" = 1 ]; then echo "ok   $mode: exit $code after $count requests"; else
    echo "FAIL $mode: exit $code after $count requests (max $max): $out"; failures=$((failures + 1)); fi
}

check 403 1 "403" 1
check unknown 1 "unknown answer" 1
# Busy waits of 1, 2 and 4 s pass the 6 s limit at the third: three requests, then stop.
check 429 1 "429" 3
check 500 1 "HTTP 500" 3
check launch 0 "exists" 3
check retryafter 0 "exists" 2
if [ "${elapsed:-0}" -lt 2 ]; then
  echo "FAIL retryafter: read again after ${elapsed:-0} s, before the Retry-After of 2 s"; failures=$((failures + 1))
fi
[ "$failures" = 0 ] && echo "wait-for-coin: ok" && exit 0
echo "wait-for-coin: $failures failed"
exit 1
