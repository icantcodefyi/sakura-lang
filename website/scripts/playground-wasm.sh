#!/usr/bin/env bash
# Builds the playground's compiler (compiler/crates/sakura_playground) for wasm32 and writes
# public/play/sakura_playground.wasm with its .sha256. The artifact is committed because
# Vercel's build has no Rust toolchain; `bun run build` checks it against the checksum, and CI
# rebuilds it and runs scripts/playground-smoke.mjs on the result so it cannot rot.
#
# Size: release, fat LTO, opt-level z, one codegen unit, panic=abort, stripped. wasm-opt is not
# run: binaryen 132's -Oz/-Os/-O2 shrink the raw file but make the brotli and gzip transfer
# 5% larger than this output.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
compiler="$here/../../compiler"
out="$here/../public/play"

cd "$compiler"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$compiler/target/playground}"
export CARGO_PROFILE_RELEASE_LTO=true
export CARGO_PROFILE_RELEASE_OPT_LEVEL=z
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
export CARGO_PROFILE_RELEASE_PANIC=abort
export CARGO_PROFILE_RELEASE_STRIP=true
# Panic messages name source files: keep the checkout and the registry out of the bytes.
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
export RUSTFLAGS="--remap-path-prefix=$compiler=/sakura --remap-path-prefix=$cargo_home=/cargo"

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build --locked -p sakura_playground --release --target wasm32-unknown-unknown

mkdir -p "$out"
cp "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/sakura_playground.wasm" "$out/"
cd "$out"
if command -v sha256sum >/dev/null; then
  sha256sum sakura_playground.wasm >sakura_playground.wasm.sha256
else
  shasum -a 256 sakura_playground.wasm >sakura_playground.wasm.sha256
fi
cat sakura_playground.wasm.sha256
echo "$(wc -c <sakura_playground.wasm) bytes, $(gzip -9c sakura_playground.wasm | wc -c) gzipped"
