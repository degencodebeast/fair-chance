#!/usr/bin/env bash
# Runs a command while it holds a lock folder, so runs at the same time take turns on it.
#   scripts/run-with-lock.sh <lock folder> <command> [args...]
# A lock left by a run that is gone (its pid is dead) is taken over. Gives up after 20 minutes.
set -euo pipefail
lock="$1"
shift
mkdir -p "$(dirname "$lock")"
waited=0
until mkdir "$lock" 2>/dev/null; do
  holder="$(cat "$lock/pid" 2>/dev/null || true)"
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
    rm -rf "$lock"
    continue
  fi
  if [ "$waited" -ge 1200 ]; then
    echo "run-with-lock: $lock still held by pid ${holder:-unknown} after 20 minutes" >&2
    exit 1
  fi
  [ "$waited" -eq 0 ] && echo "run-with-lock: another run holds $lock; waiting"
  sleep 1
  waited=$((waited + 1))
done
echo $$ >"$lock/pid"
trap 'rm -rf "$lock"' EXIT
"$@"
