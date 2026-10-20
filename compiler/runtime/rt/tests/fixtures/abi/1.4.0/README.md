# ABI 1.4.0: `v0.1.0`

- Release tag: `v0.1.0` (`2014e821b87a0d30cb8061b6158f99723f0f676d`), whose
  `runtime/rt/src/abi.rs` records `ABI_VERSION = 1 << 16 | 4 << 8`.
- `sakura --version`: `sakura 0.1.0 (2014e821b87a0d30cb8061b6158f99723f0f676d)`.
- Install channel: npm, `sakuralang@0.1.0` (the published `sakuralang-darwin-arm64` binary).
- Captured 2026-10-21 on macOS arm64, as [the fixture README](../README.md) describes:
  `sakura build --target wasm --out-dir out main.sak` and `sakura run main.sak > expected.stdout`
  outside the checkout.
- `main.wasm` SHA-256: `b1cd30fa3569440446b204253b865b701bf522bc1a4702fcea0ece36c5a924cc`.
