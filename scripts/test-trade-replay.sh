#!/usr/bin/env bash
# Runs the trade replay tests (analysis/smdd-replay/tests) on the synthetic fixture.
# Python: the repository's .venv when it exists (next to the main .git folder, so worktrees find
# it too), else python3. Always isolated mode (-I).
# Prints the last lines, and on a failure each FAIL and ERROR block. A test that reads the real
# trade file (test_repository_holds_no_trade_data) prints only its name and the error type, never
# a value, a path or a line of the real data.
set -euo pipefail
cd "$(dirname "$0")/.."
common="$(git rev-parse --path-format=absolute --git-common-dir)"
python="$common/../.venv/bin/python"
[ -x "$python" ] || python="python3"
log="$(mktemp)"
trap 'rm -f "$log"' EXIT
status=0
"$python" -I -m unittest discover -s analysis/smdd-replay/tests >"$log" 2>&1 || status=$?
if [ "$status" -ne 0 ]; then
  "$python" -I scripts/trade-replay-failures.py "$log"
fi
tail -3 "$log"
exit "$status"
