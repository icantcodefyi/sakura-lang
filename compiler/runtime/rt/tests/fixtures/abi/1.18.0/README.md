# ABI 1.18.0: `v1.0.0-beta.4`

- Release tag: `v1.0.0-beta.4` (`40af20ce096074012bbd2344129da441589d3e6c`), whose
  `runtime/rt/src/abi.rs` records `ABI_VERSION = 1 << 16 | 18 << 8`. No release shipped 1.17.0,
  so there is no `1.17.0/` (design §17.310 (3c)).
- `sakura --version`: `sakura 1.0.0-beta.4 (40af20ce096074012bbd2344129da441589d3e6c)`.
- Install channel: GitHub release archive, `sakuralang-1.0.0-beta.4-aarch64-apple-darwin.tar.gz`,
  verified against the release's `SHA256SUMS` (archive SHA-256:
  `1ff98c5a70da66a91ff567dfc869610de925472e700a7fe5a0b4f72ae64fcb3c`).
- Captured 2026-10-22 on macOS arm64, as [the fixture README](../README.md) describes:
  `sakura build --target wasm --out-dir out main.sak` and `sakura run main.sak > expected.stdout`
  outside the checkout.
- `main.wasm` SHA-256: `18be2fea3c8bb7f67f3be726882710ea5675e6b4bc2ca4e6051d62635b1080f6`.
