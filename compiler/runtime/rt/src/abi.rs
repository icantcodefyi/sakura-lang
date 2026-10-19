// The ABI revision and the size of `stats()` (ABI §0, §10). `sakura_codegen_wasm` includes
// this file, so the emitter and the runtime object cannot disagree, and its tests hold the
// Node host's constants to it.

/// `sakura_abi_version` (ABI §0): `MAJOR << 16 | MINOR << 8 | PATCH`.
pub const ABI_VERSION: u32 = 1 << 16 | 4 << 8;

/// The number of `u64` counters `stats()` returns (ABI §10).
pub const STATS: usize = 15;
