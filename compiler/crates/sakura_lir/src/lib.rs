//! Sakura Lang's low-level IR for the WASM backend (design §17.82, §17.83; plan §3.5 M4-LIR).
//!
//! The data model is [`lir::Module`], laid out by [`layout`] (ABI 1.0.1 §3.4), checked by
//! [`validate`] and printed by [`print`].

pub mod layout;
pub mod lir;
pub mod print;
pub mod validate;

/// `sakura-core-snapshot-v1` (§17.107), the shared committed Core.
pub use sakura_core_snapshot as snapshot;
