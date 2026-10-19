#!/bin/bash
set +x
set -euo pipefail
umask 077

[ "$#" -gt 0 ] || { echo 'usage: sign-macos.sh BINARY...' >&2; exit 1; }
: "${RUNNER_TEMP:?RUNNER_TEMP is required}"
for name in MACOS_CERT_P12_BASE64 MACOS_CERT_PASSWORD APPLE_API_KEY_P8 APPLE_API_KEY_ID APPLE_API_ISSUER_ID; do
  if [ -z "${!name:-}" ]; then echo "::error::Missing signing secret $name"; exit 1; fi
done

identity='Developer ID Application: Aniruddh Dubge (P62B65A837)'
work="$RUNNER_TEMP/sakura-signing"
mkdir -m 700 "$work"
keychain="$work/signing.keychain-db"
cleanup() {
  status=$?
  trap - EXIT
  if [ -f "$keychain" ]; then
    if ! security delete-keychain "$keychain" >/dev/null 2>&1; then
      echo '::error::Could not delete the temporary signing keychain'
      status=1
    fi
  fi
  rm -rf "$work"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

python3 -I - <<'PY'
import os
for name in ("MACOS_CERT_P12_BASE64", "MACOS_CERT_PASSWORD", "APPLE_API_KEY_P8", "APPLE_API_KEY_ID", "APPLE_API_ISSUER_ID"):
    value = os.environ[name]
    for item in [value, *value.splitlines()]:
        if item:
            escaped = item.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
            print(f"::add-mask::{escaped}")
PY
password=$(openssl rand -hex 32)
echo "::add-mask::$password"
printf '%s' "$MACOS_CERT_P12_BASE64" | base64 --decode > "$work/certificate.p12"
printf '%s\n' "$APPLE_API_KEY_P8" > "$work/AuthKey.p8"

# Tool diagnostics can contain certificate/key material; keep them private until cleanup.
if ! {
  security create-keychain -p "$password" "$keychain" &&
  security set-keychain-settings -lut 21600 "$keychain" &&
  security unlock-keychain -p "$password" "$keychain" &&
  security import "$work/certificate.p12" -P "$MACOS_CERT_PASSWORD" -k "$keychain" -T /usr/bin/codesign &&
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" &&
  security find-identity -v -p codesigning "$keychain" > "$work/identities"
} > "$work/import.log" 2>&1; then
  echo '::error::Could not import the signing certificate into the temporary keychain'
  exit 1
fi
if ! grep -Fq "\"$identity\"" "$work/identities"; then
  echo '::error::The temporary keychain lacks the required P62B65A837 Developer ID identity'
  exit 1
fi

index=0
for binary in "$@"; do
  index=$((index + 1))
  if ! codesign --force --options runtime --timestamp --keychain "$keychain" --sign "$identity" "$binary" > "$work/codesign.log" 2>&1; then
    echo '::error::Developer ID signing failed'
    exit 1
  fi
  ditto -c -k "$binary" "$work/submission-$index.zip"
  if ! xcrun notarytool submit "$work/submission-$index.zip" --wait --timeout 20m \
    --key "$work/AuthKey.p8" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID" \
    --output-format json > "$work/notary.json" 2> "$work/notary.log"; then
    echo '::error::Apple notarization submission failed; no signed artifact will be released'
    exit 1
  fi
  if ! python3 -I - "$work/notary.json" <<'PY'
import json, sys
with open(sys.argv[1]) as source:
    accepted = json.load(source).get("status") == "Accepted"
sys.exit(0 if accepted else 1)
PY
  then
    echo '::error::Apple did not accept the notarization submission'
    exit 1
  fi
  if ! bash "$(dirname "$0")/verify-macos.sh" "$binary" > "$work/verify.log" 2>&1; then
    echo '::error::The signed binary failed identity, signature, or online notarization verification'
    exit 1
  fi
  echo "Signed and notarized $(basename "$binary") with the pinned P62B65A837 identity."
done
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  cat >> "$GITHUB_STEP_SUMMARY" <<'SUMMARY'
macOS signing and Apple notarization succeeded with the pinned P62B65A837 identity.
The bare Mach-O cannot carry a stapled ticket; Gatekeeper may need network access.
`spctl -a -t install` applies to installer packages, which this release does not ship.
Bare-code verification uses `codesign --verify --strict` and `-R notarized --check-notarization`.
SUMMARY
fi
