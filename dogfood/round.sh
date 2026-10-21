#!/usr/bin/env bash
# The dogfood corpus against an installed `sakura` (design §17.225 (2), §17.301 (3d)): for each
# program, `check`, then `test` on js and on wasm, never the source build. Used by
# .github/workflows/dogfood.yml and by each per-beta upgrade round, whose MIGRATION.md records
# the result. Usage: dogfood/round.sh [path/to/sakura] (default: `sakura` on PATH).
# Prints one line per program and step; exits 1 if any step failed.
set -u
sakura=${1:-sakura}
root=$(cd "$(dirname "$0")" && pwd)
"$sakura" --version || exit 1
failed=0
for dir in "$root"/*/; do
  [ -f "$dir/sakura.toml" ] || continue
  program=$(basename "$dir")
  for step in "check" "test" "test --target wasm"; do
    log=$(mktemp)
    # shellcheck disable=SC2086 # $step is a word list
    if (cd "$dir" && "$sakura" $step) >"$log" 2>&1; then
      echo "ok   $program: sakura $step"
    else
      echo "FAIL $program: sakura $step"
      sed 's/^/     /' "$log"
      failed=1
    fi
    rm -f "$log"
  done
done
exit "$failed"
