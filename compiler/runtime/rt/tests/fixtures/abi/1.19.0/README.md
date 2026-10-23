# ABI 1.19.0: `v1.0.0-beta.6`

- Release tag: `v1.0.0-beta.6` (`70c64513666c3f2d818e722bf9b2b005f3f8b536`), whose
  `runtime/rt/src/abi.rs` records `ABI_VERSION = 1 << 16 | 19 << 8`.
- `sakura --version`: `sakura 1.0.0-beta.6 (70c64513666c3f2d818e722bf9b2b005f3f8b536)`.
- Install channel: GitHub release archive, `sakuralang-1.0.0-beta.6-aarch64-apple-darwin.tar.gz`,
  verified against the release's `SHA256SUMS` (archive SHA-256:
  `441cc561f30709b8226699b1ae96a02cb17a56399920b2a1ce854cc7ccdd95d3`).
- Captured 2026-10-23 on macOS arm64, as [the fixture README](../README.md) describes:
  `sakura build --target wasm --out-dir out main.sak` and `sakura run main.sak > expected.stdout`
  outside the checkout.
- `main.wasm` SHA-256: `38750e96d1c4b222720507b72af33eff2ebda4020df4832552030b2e2161cce5`.
