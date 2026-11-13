#!/usr/bin/env bash
# Waits until a coin answers fc_rules on a NEAR RPC, then exits 0.
#
#   scripts/wait-for-coin.sh <rpc url> <coin account>
#
# While the coin does not exist yet (UNKNOWN_ACCOUNT) it asks again every FC_WAIT_POLL_S seconds
# (default 5: the public RPC rate limits one address, which the run and the timing tool share),
# for FC_WAIT_DEADLINE_S at most (default 5400 s: the run can wait on a Keychain prompt). On a 429 or a 5xx it backs off (FC_WAIT_FIRST_BACKOFF_S, default 2 s,
# then double, up to 60 s; a longer Retry-After wins). It waits at most twice in all (a 200 in
# between does not reset the count) and stops when the waits would pass FC_WAIT_BUSY_LIMIT_S in
# all (default 900 s): the builder's 429 rule. On a 403, a failed request or any answer it does
# not know, it stops at once and exits 1 with the reason. It never prints a key or a token;
# the RPC URL must hold none.
set -uo pipefail

rpc="${1:?usage: scripts/wait-for-coin.sh <rpc url> <coin account>}"
coin="${2:?usage: scripts/wait-for-coin.sh <rpc url> <coin account>}"
wait_s="${FC_WAIT_FIRST_BACKOFF_S:-2}"
poll_s="${FC_WAIT_POLL_S:-5}"
busy_limit_s="${FC_WAIT_BUSY_LIMIT_S:-900}"
busy_waited=0
busy_waits=0
max_busy_waits=2
request="{\"jsonrpc\":\"2.0\",\"id\":\"fc\",\"method\":\"query\",\"params\":{\"request_type\":\"call_function\",\"finality\":\"final\",\"account_id\":\"$coin\",\"method_name\":\"fc_rules\",\"args_base64\":\"e30=\"}}"
body="$(mktemp)"
headers="$(mktemp)"
trap 'rm -f "$body" "$headers"' EXIT
deadline=$(( $(date +%s) + ${FC_WAIT_DEADLINE_S:-5400} ))

while :; do
  code="$(curl -s -m 10 -D "$headers" -o "$body" -w '%{http_code}' "$rpc" -H 'Content-Type: application/json' -d "$request")" || code="000"
  case "$code" in
    200)
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
      sleep "$poll_s"
      ;;
    403)
      echo "wait-for-coin: the RPC answered 403; stopped, no retry" >&2
      exit 1
      ;;
    429|5??)
      this_wait="$wait_s"
      after="$(tr -d '\r' < "$headers" | awk 'tolower($1) == "retry-after:" {print $2}' | tail -1)"
      case "$after" in
        ''|*[!0-9]*) ;;
        *) [ "$after" -gt "$this_wait" ] && this_wait="$after" ;;
      esac
      if [ "$busy_waits" -ge "$max_busy_waits" ] || [ $((busy_waited + this_wait)) -gt "$busy_limit_s" ]; then
        echo "wait-for-coin: the RPC kept answering HTTP $code (429 or 5xx) past the wait limit (${max_busy_waits} waits, ${busy_limit_s} s in all); stopped, no retry" >&2
        exit 1
      fi
      echo "wait-for-coin: HTTP $code; waiting ${this_wait} s" >&2
      sleep "$this_wait"
      busy_waited=$((busy_waited + this_wait))
      busy_waits=$((busy_waits + 1))
      wait_s=$(( wait_s == 0 ? 0 : (wait_s * 2 > 60 ? 60 : wait_s * 2) ))
      ;;
    *)
      echo "wait-for-coin: unknown answer (HTTP $code) from the RPC for $coin; stopped, no retry" >&2
      exit 1
      ;;
  esac
  if [ "$(date +%s)" -ge "$deadline" ]; then
    echo "wait-for-coin: $coin did not appear in time; stopped" >&2
    exit 1
  fi
done
