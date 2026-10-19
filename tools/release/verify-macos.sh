#!/bin/bash
set -euo pipefail

binary=${1:?usage: verify-macos.sh BINARY}
codesign --verify --strict -R 'anchor apple generic and certificate leaf[subject.OU] = "P62B65A837"' "$binary"
codesign --verify --strict -R notarized --check-notarization "$binary"
