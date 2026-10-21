# ABI 1.16.0: `v1.0.0-beta.3`

- Release tag: `v1.0.0-beta.3` (`472dea56e094017df4c9313170a1bf249c3878b8`), whose
  `runtime/rt/src/abi.rs` records `ABI_VERSION = 1 << 16 | 16 << 8`.
- `sakura --version`: `sakura 1.0.0-beta.3 (472dea56e094017df4c9313170a1bf249c3878b8)`.
- Install channel: GitHub release archive, `sakuralang-1.0.0-beta.3-aarch64-apple-darwin.tar.gz`,
  verified against the release's `SHA256SUMS` (archive SHA-256:
  `4b15bde2296a31127236e782ab7f982a8f1c7fc9645586ba819a9e9295085fee`).
- Captured 2026-10-21 on macOS arm64, as [the fixture README](../README.md) describes:
  `sakura build --target wasm --out-dir out main.sak` and `sakura run main.sak > expected.stdout`
  outside the checkout.
- `main.wasm` SHA-256: `bd82d80764e60c01c2289556d69bc875200b4865f16a29a740cc11b85540e8ef`.
