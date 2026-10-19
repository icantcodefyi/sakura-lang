#!/bin/bash
set -euo pipefail

binary=${1:?usage: verify-macos.sh BINARY}
if ! codesign --verify --strict -R 'anchor apple generic and certificate leaf[subject.OU] = "P62B65A837"' "$binary"; then
  echo 'verify-macos: identity or signature requirement failed' >&2
  exit 1
fi
# A freshly accepted ticket can take minutes to reach Apple's online lookup.
attempts=10
for attempt in $(seq "$attempts"); do
  if codesign --verify --strict -R notarized --check-notarization "$binary"; then exit 0; fi
  echo "verify-macos: online notarization check failed (attempt $attempt/$attempts)" >&2
  [ "$attempt" -lt "$attempts" ] && sleep "${SAKURA_NOTARY_RETRY_SECONDS:-30}"
done
exit 1
