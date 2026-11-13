#!/usr/bin/env bash
# Waits until a coin answers fc_rules on a NEAR RPC, then exits 0.
#
#   scripts/wait-for-coin.sh <rpc url> <coin account>
#
# While the coin does not exist yet (UNKNOWN_ACCOUNT) it asks again every second, for 30 minutes
# at most. On a 429 or a 5xx it backs off (FC_WAIT_FIRST_BACKOFF_S, default 2 s, then double, up to
# 60 s) and gives up after 5 such answers in a row. On a 403, a failed request or any answer it
# does not know, it stops at once and exits 1 with the reason. It never prints a key or a token;
# the RPC URL must hold none.
set -uo pipefail

rpc="${1:?usage: scripts/wait-for-coin.sh <rpc url> <coin account>}"
coin="${2:?usage: scripts/wait-for-coin.sh <rpc url> <coin account>}"
wait_s="${FC_WAIT_FIRST_BACKOFF_S:-2}"
request="{\"jsonrpc\":\"2.0\",\"id\":\"fc\",\"method\":\"query\",\"params\":{\"request_type\":\"call_function\",\"finality\":\"final\",\"account_id\":\"$coin\",\"method_name\":\"fc_rules\",\"args_base64\":\"e30=\"}}"
body="$(mktemp)"
trap 'rm -f "$body"' EXIT
deadline=$(( $(date +%s) + 1800 ))
busy=0

while :; do
  code="$(curl -s -m 10 -o "$body" -w '%{http_code}' "$rpc" -H 'Content-Type: application/json' -d "$request")" || code="000"
  case "$code" in
    200)
      busy=0
      # A call_function answer holds the return bytes as "result":[...] and no "error".
      if grep -q '"result":\[' "$body" && ! grep -q '"error"' "$body"; then
        echo "wait-for-coin: $coin exists"
        exit 0
      elif grep -q 'UNKNOWN_ACCOUNT' "$body"; then
        :
      else
        echo "wait-for-coin: unknown answer from the RPC for $coin; stopped, no retry" >&2
        exit 1
      fi
      sleep 1
      ;;
    403)
      echo "wait-for-coin: the RPC answered 403; stopped, no retry" >&2
      exit 1
      ;;
    429|5??)
      busy=$((busy + 1))
      if [ "$busy" -ge 5 ]; then
        echo "wait-for-coin: the RPC answered HTTP $code $busy times in a row (429 or 5xx); stopped" >&2
        exit 1
      fi
      echo "wait-for-coin: HTTP $code; waiting ${wait_s} s" >&2
      sleep "$wait_s"
      wait_s=$(( wait_s == 0 ? 0 : (wait_s * 2 > 60 ? 60 : wait_s * 2) ))
      ;;
    *)
      echo "wait-for-coin: unknown answer (HTTP $code) from the RPC for $coin; stopped, no retry" >&2
      exit 1
      ;;
  esac
  if [ "$(date +%s)" -ge "$deadline" ]; then
    echo "wait-for-coin: $coin did not appear in 30 minutes; stopped" >&2
    exit 1
  fi
done
