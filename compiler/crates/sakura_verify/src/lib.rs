//! `sakura verify`'s engine (design §7.5, §17.84–§17.86, §17.98): verification conditions from
//! Core contract checks and checked arithmetic, their SMT-LIB2 encoding, and the status record of
//! each obligation.
//!
//! Before the M3 gates this crate reads committed Core snapshots ([`core`]) instead of
//! `sakura_core`; nothing here can remove a runtime check (the removal hook is V3).

pub mod core;
pub mod record;
pub mod smt;
pub mod vc;
