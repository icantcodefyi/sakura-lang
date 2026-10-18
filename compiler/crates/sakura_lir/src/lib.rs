//! Sakura Lang's low-level IR for the WASM backend (design §17.82, §17.83; plan §3.5 M4-LIR).
//!
//! Input is Core as the shared committed snapshot `sakura-core-snapshot-v1` (§17.107); output is
//! [`lir::Module`], checked by [`validate`] and run by [`interp`].

pub mod interp;
pub mod layout;
pub mod lir;
pub mod lower;
pub mod print;
pub mod show;
pub mod validate;

/// `sakura-core-snapshot-v1` (§17.107), the shared committed Core.
pub use sakura_core_snapshot as snapshot;
