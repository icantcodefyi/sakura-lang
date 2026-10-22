//! The `exec_native` target (plan M5 row S, design §17.308 (1e)): `exec` with the native
//! backend as a fourth executor beside JS, WASM and the evaluators, held to WASM's whole run
//! (stdout, ending, defect record and trace). Its own ledger target, outside `v1.0.0`'s gate.
//! The target is `exec.rs` (see its `native` module); it needs the `native` feature, a system
//! C linker, and what `exec` needs.
#![no_main]

#[allow(unused_attributes)] // `exec.rs`'s `#![no_main]`, which this crate root carries.
#[path = "exec.rs"]
mod exec;
