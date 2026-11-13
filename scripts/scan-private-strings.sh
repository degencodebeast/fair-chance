#!/usr/bin/env bash
# Scans a git repo for strings that must never be public: private build-plan names (ticket and
# part numbers) and paths into the builder's private workspace or home folder.
# It reads every file git tracks or would add (ignored files are skipped) and every commit
# message reachable from HEAD.
#   scripts/scan-private-strings.sh [repo folder]    (default: this repo)
# Prints "private-string scan: clean ..." and exits 0, or prints each hit and
# "private-string scan: FAIL ..." and exits 1.
# A commit listed in scripts/private-string-scan-known-commits.txt is skipped and counted.
# Each pattern starts with a bracket class, so this file never matches itself.
set -euo pipefail
repo="${1:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$repo"

case_blind_patterns=(
  '[t]icket[ _/#-]?[0-9]+'
  '[m]ap ticket'
  '[f]air-chance-map'
  '[n]ear-hack'
  '(^|[^A-Za-z0-9_])[.]scratch/'
  '[p]laybooks/'
  '[s]pace[.]md'
  '[P]ROJECT_WORKFLOW'
  '[C]ONTROLLER_HANDOFF'
  '[c]laude-design-output'
)
case_exact_patterns=(
  '[P]art [0-9]+'
  '/[U]sers/[A-Za-z0-9._-]+/'
  '/[h]ome/[A-Za-z0-9._-]+/'
)

blind_args=() exact_args=()
for p in "${case_blind_patterns[@]}"; do blind_args+=(-e "$p"); done
for p in "${case_exact_patterns[@]}"; do exact_args+=(-e "$p"); done

hits="$(mktemp)"
trap 'rm -f "$hits"' EXIT
files=0
while IFS= read -r -d '' file; do
  [ -f "$file" ] || continue
  files=$((files + 1))
  { grep -I -n -H -i -E "${blind_args[@]}" -- "$file" || true
    grep -I -n -H -E "${exact_args[@]}" -- "$file" || true; } >>"$hits"
done < <(git ls-files -z --cached --others --exclude-standard)

known_file=scripts/private-string-scan-known-commits.txt
known=" "
[ -f "$known_file" ] && known=" $(grep -v -E "^(#|$)" "$known_file" | tr "\n" " ")"
commits=0 skipped=0
if git rev-parse -q --verify HEAD >/dev/null; then
  while IFS= read -r commit; do
    commits=$((commits + 1))
    case "$known" in *" $commit "*) skipped=$((skipped + 1)); continue ;; esac
    message="$(git log -1 --format=%B "$commit")"
    { grep -n -i -E "${blind_args[@]}" <<<"$message" || true
      grep -n -E "${exact_args[@]}" <<<"$message" || true; } | sed "s/^/commit $commit message:/" >>"$hits"
  done < <(git rev-list HEAD)
fi

if [ -s "$hits" ]; then
  sort -u "$hits"
  echo "private-string scan: FAIL ($(sort -u "$hits" | wc -l | tr -d ' ') hits in $files files and $commits commit messages)"
  exit 1
fi
echo "private-string scan: clean ($files files, $commits commit messages, $skipped known commits skipped: see $known_file)"
