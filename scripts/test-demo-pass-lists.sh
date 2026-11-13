#!/usr/bin/env bash
# Runs the judge-path script and the Option T script on fresh sandboxes and checks the README
# contract: each exits 0, prints no FAIL line, and ends with its PASS list, one line per check,
# in this order. Needs what scripts/judge-path.sh needs.
set -uo pipefail
cd "$(dirname "$0")/.."
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
failures=0

expect_pass_list() { # expect_pass_list <script> <check name>...
  local script="$1" got=0
  shift
  "$script" >"$scratch/out.txt" 2>"$scratch/err.txt" || got=$?
  local want_count=$#
  local tail_lines
  tail_lines="$(grep -E '^(PASS|FAIL) ' "$scratch/out.txt")"
  local ok=1
  [ "$got" = 0 ] || ok=0
  ! grep -q '^FAIL ' "$scratch/out.txt" || ok=0
  [ "$(printf '%s\n' "$tail_lines" | grep -c '^PASS ')" = "$want_count" ] || ok=0
  local i=1
  for name in "$@"; do
    printf '%s\n' "$tail_lines" | sed -n "${i}p" | grep -q "^PASS $name: " || ok=0
    i=$((i + 1))
  done
  # The PASS list is the end of the output.
  [ "$(tail -n "$want_count" "$scratch/out.txt")" = "$tail_lines" ] || ok=0
  if [ "$ok" = 1 ]; then
    echo "ok   $script"
  else
    echo "FAIL $script (exit $got); output:"
    cat "$scratch/out.txt"
    tail -20 "$scratch/err.txt"
    failures=$((failures + 1))
  fi
}

expect_pass_list scripts/judge-path.sh \
  "opening buy above 4% refused" \
  "coin launched" \
  "opening buy marked" \
  "early-window sell refused" \
  "large buy" \
  "large send refused" \
  "limited sell filled in part" \
  "cap full" \
  "receipt on the coin page" \
  "alert"

expect_pass_list scripts/option-t.sh \
  "twenty buyers bought in blocks 0 to 17" \
  "every buy is marked" \
  "one shared allowance" \
  "cap full"

[ "$failures" = 0 ] && echo "demo pass lists: ok" && exit 0
echo "demo pass lists: $failures failed"
exit 1
