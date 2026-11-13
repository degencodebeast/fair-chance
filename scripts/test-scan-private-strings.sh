#!/usr/bin/env bash
# Tests for scripts/scan-private-strings.sh. Each case makes a throwaway git repo, plants one
# string, and checks the scan's exit code. The planted strings are built from parts so that this
# file itself never matches the scan.
set -euo pipefail
scan="$(cd "$(dirname "$0")" && pwd)/scan-private-strings.sh"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
failures=0

new_repo() {
  local dir="$scratch/$1"
  mkdir -p "$dir"
  git -C "$dir" init -q
  git -C "$dir" config user.email test@example.invalid
  git -C "$dir" config user.name test
  echo "a clean line" >"$dir/clean.txt"
  git -C "$dir" add -A
  git -C "$dir" commit -q -m "First commit"
  echo "$dir"
}

expect() { # expect <name> <want exit code> <repo>
  local name="$1" want="$2" repo="$3" got=0
  "$scan" "$repo" >"$scratch/out.txt" 2>&1 || got=$?
  local want_line="private-string scan: clean"
  [ "$want" = 0 ] || want_line="private-string scan: FAIL"
  if [ "$got" = "$want" ] && grep -q "^$want_line" "$scratch/out.txt"; then
    echo "ok   $name"
  else
    echo "FAIL $name: exit $got, want $want and a line \"$want_line\""
    sed 's/^/     /' "$scratch/out.txt"
    failures=$((failures + 1))
  fi
}

plant() { # plant <repo> <text>: a tracked file with the text
  printf 'line one\n%s\n' "$2" >"$1/planted.md"
  git -C "$1" add planted.md
}

repo="$(new_repo clean)"
expect "a clean repo passes" 0 "$repo"

repo="$(new_repo ticket-number)"; plant "$repo" "see tick""et 17 for this"
expect "a ticket number fails" 1 "$repo"

repo="$(new_repo ticket-number-joined)"; plant "$repo" "branch tick""et/02"
expect "a ticket branch name fails" 1 "$repo"

repo="$(new_repo part-number)"; plant "$repo" "as in Par""t 7 of the plan"
expect "a part number fails" 1 "$repo"

repo="$(new_repo map-ticket)"; plant "$repo" "the ma""p ticket says"
expect "the words for a build-plan item fail" 1 "$repo"

repo="$(new_repo home-path)"; plant "$repo" "open /Us""ers/someone/project/file"
expect "a home folder path fails" 1 "$repo"

repo="$(new_repo workspace-folder)"; plant "$repo" "read .scr""atch/notes"
expect "a private workspace folder fails" 1 "$repo"

repo="$(new_repo build-map)"; plant "$repo" "fair-chance""-map/build"
expect "the build map folder name fails" 1 "$repo"

repo="$(new_repo untracked)"; echo "tick""et 17" >"$repo/new-file.txt"
expect "an untracked file that git would add is scanned" 1 "$repo"

repo="$(new_repo ignored)"; echo "ignored.txt" >"$repo/.gitignore"; echo "tick""et 17" >"$repo/ignored.txt"
git -C "$repo" add .gitignore
expect "an ignored file is not scanned" 0 "$repo"

repo="$(new_repo commit-message)"; echo "more" >>"$repo/clean.txt"
git -C "$repo" commit -q -am "Finish tick""et 17"
expect "a commit message with a ticket number fails" 1 "$repo"

repo="$(new_repo known-commit)"; echo "more" >>"$repo/clean.txt"
git -C "$repo" commit -q -am "Finish tick""et 17"
mkdir -p "$repo/scripts"
printf "# reason\n%s\n" "$(git -C "$repo" rev-parse HEAD)" >"$repo/scripts/private-string-scan-known-commits.txt"
git -C "$repo" add scripts
expect "a commit listed as known is skipped" 0 "$repo"

repo="$(new_repo plain-words)"; plant "$repo" "Part of the plan. A ticket price of 17 NEAR. Users can sell."
expect "plain words that only look close pass" 0 "$repo"

if [ "$failures" -gt 0 ]; then
  echo "test-scan-private-strings: $failures failed"
  exit 1
fi
echo "test-scan-private-strings: all passed"
